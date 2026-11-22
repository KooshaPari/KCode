impl StatusSpinnerRenderer {
    pub(super) fn spinner_only_available(&self, app: &App) -> bool {
        status_spinner_only_symbol(app).is_some()
    }

    pub(super) fn invalidate(&mut self) {
        self.last_frame = None;
        self.last_full_frame_at = None;
        // Nothing is known to be seeded any more, so the next animation-only
        // repaint must do one full seed before it may copy just the animated
        // rows again.
        self.seeded_animation_area = None;
        self.last_full_frame_input.clear();
    }

    /// Present an animation frame and forget its working-buffer seed.
    ///
    /// `Terminal::swap_buffers` resets the buffer that the next tick will draw
    /// into, so cells outside the animation area are no longer seeded there.
    fn swap_after_animation_frame<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut ratatui::Terminal<B>,
    ) {
        terminal.swap_buffers();
        self.seeded_animation_area = None;
    }

    /// Whether the decorative idle-animation rows can be repainted on their own,
    /// reusing the rest of the previous frame.
    ///
    /// Available only when the previous full frame actually drew the animation
    /// and nothing else in the app needs a repaint this tick. On an idle screen
    /// the animation is the sole moving element, so a full render at animation
    /// FPS re-derives the transcript, header, status, and composer into
    /// byte-identical cells. That was measured at a ~50ms median per tick on an
    /// 8-core laptop, which is what made the animation visibly lag.
    /// Whether the composer differs from what the last full frame drew.
    ///
    /// The animation-only repaint reuses that frame everywhere except the
    /// decorative rows, so it cannot display a newer input line.
    fn composer_changed_since_last_full_frame(&self, input: &str) -> bool {
        input != self.last_full_frame_input
    }

    pub(super) fn idle_animation_only_available(&self, app: &App) -> bool {
        let blocked = idle_animation_fast_path_blocked_reason(&IdleAnimationFastPathInputs {
            has_previous_frame: self.last_frame.is_some(),
            animation_active: crate::tui::idle_donut_active(app),
            has_animation_area: crate::tui::ui::last_idle_animation_area().is_some(),
            force_full_redraw: app.force_full_redraw,
            force_full_repaint: app.force_full_repaint,
            composer_changed: self.composer_changed_since_last_full_frame(&app.input),
            command_palette_visible: slash_command_palette_may_be_visible(&app.input, || {
                !app.command_suggestions().is_empty()
            }),
        });
        if let Some(reason) = blocked {
            crate::tui::ui::note_idle_animation_fast_path_blocked(reason);
            return false;
        }

        let allowed = idle_animation_partial_repaint_allowed(
            crate::tui::periodic_redraw_required_excluding_idle_animation(app),
            self.last_full_frame_at.map(|at| at.elapsed()),
        );
        if !allowed {
            crate::tui::ui::note_idle_animation_fast_path_blocked("chrome_full_frame_due");
        }
        allowed
    }

    /// Repaint just the idle-animation rows over the previous frame.
    ///
    /// Returns `false` when the fast path cannot be used, in which case the
    /// caller must fall back to a full redraw.
    ///
    /// "Cheap" here is relative to a full render, but this used to be far more
    /// expensive than it needed to be: it cloned the entire screen buffer twice
    /// per frame (`clone_from` to seed the working buffer, then `clone` to keep a
    /// copy) even though only `area` changes. At 60fps on a 160x48 terminal that
    /// is ~920k cell copies a second to update ~2200 cells. Measured on a real
    /// session, the animation cost 0.257 CPU cores over an idle client and nearly
    /// doubled keystroke latency (p50 6.6ms vs 3.5ms). Now only the animated rows
    /// are touched, in both buffers.
    pub(super) fn draw_idle_animation_only(
        &mut self,
        app: &App,
        terminal: &mut DefaultTerminal,
    ) -> Result<bool> {
        let Some(previous_frame) = self.last_frame.as_ref() else {
            return Ok(false);
        };
        let Some(area) = crate::tui::ui::last_idle_animation_area() else {
            return Ok(false);
        };
        // The terminal may have been resized since that frame was captured.
        if !previous_frame.area.contains((area.x, area.y).into())
            || area.right() > previous_frame.area.right()
            || area.bottom() > previous_frame.area.bottom()
        {
            return Ok(false);
        }

        {
            let current_buffer = terminal.current_buffer_mut();
            if current_buffer.area != previous_frame.area {
                return Ok(false);
            }
            // Seed the working buffer from the last known frame. Only the
            // animated rows can differ once this path owns the screen (it is the
            // only writer between full frames), so a live seed can reuse just
            // those rows. `swap_buffers` resets the next working buffer, and the
            // presentation hook below invalidates the seed at that boundary.
            if self.seeded_animation_area == Some(area) {
                copy_cells_in(previous_frame, current_buffer, area);
            } else {
                current_buffer.clone_from(previous_frame);
                self.seeded_animation_area = Some(area);
            }
            crate::tui::ui::render_idle_animation_into(
                current_buffer,
                area,
                crate::tui::TuiState::animation_elapsed(app),
            );
        }

        // Same protocol as the one-cell spinner fast path: keep ratatui's
        // virtual buffers authoritative, flush the diff inside a synchronized
        // update, and preserve the user's cursor position.
        crossterm::queue!(
            terminal.backend_mut(),
            BeginSynchronizedUpdate,
            SavePosition
        )?;
        terminal.flush()?;
        crossterm::queue!(
            terminal.backend_mut(),
            RestorePosition,
            EndSynchronizedUpdate
        )?;
        self.swap_after_animation_frame(terminal);
        terminal.backend_mut().flush()?;
        // Keep the remembered frame current without cloning the whole screen: it
        // already matches everywhere except the rows just re-rendered. Re-render
        // the animation into those rows directly (deterministic for a given
        // elapsed time, and far cheaper than copying 7680 cells) so
        // `self.last_frame` stays byte-identical to what the terminal now shows.
        if let Some(last) = self.last_frame.as_mut() {
            crate::tui::ui::render_idle_animation_into(
                last,
                area,
                crate::tui::TuiState::animation_elapsed(app),
            );
        }
        // Without this the animation-only path was invisible in `draw-stats`:
        // `partial_repaints` stayed at 0 while this path served ~60 repaints a
        // second, which reads as "the cheap path never runs" and sends anyone
        // debugging redraw cost down the wrong branch.
        crate::tui::ui::note_idle_animation_partial_repaint();
        Ok(true)
    }

    pub(super) fn draw_full(
        &mut self,
        app: &mut App,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        // Painting a frame is progress, including during long streaming turns.
        crate::logging::watchdog::beat("tui.draw");
        app.refresh_terminal_title_metrics();
        app.sync_herdr_agent_state();
        let invalidation = full_frame_invalidation(app.force_full_redraw, app.force_full_repaint);
        let force_full_redraw = invalidation != FullFrameInvalidation::None;
        // Wrap the whole frame (optional clear + diff flush) in a synchronized update so the
        // terminal applies every cell change atomically. Without this, ratatui's crossterm
        // backend streams cells one-by-one and eagerly-repainting terminals (and slow/remote or
        // multiplexed sessions) show visible flicker. See issue #282.
        let sync = crossterm::execute!(terminal.backend_mut(), BeginSynchronizedUpdate).is_ok();
        match invalidation {
            FullFrameInvalidation::HardClear => {
                terminal.clear()?;
                self.invalidate();
            }
            FullFrameInvalidation::SoftRepaint => {
                invalidate_previous_terminal_buffer(terminal);
                self.invalidate();
            }
            FullFrameInvalidation::None => {}
        }
        app.force_full_redraw = false;
        app.force_full_repaint = false;

        let previous_frame = self.last_frame.as_ref();
        let draw_start = Instant::now();
        let mut render_elapsed = Duration::ZERO;
        // Synchronized update prevents tearing on supported terminals.
        let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x1b[?2026h");
        let completed = terminal.draw(|frame| {
            let render_start = Instant::now();
            crate::tui::ui::draw(frame, app);
            render_elapsed = render_start.elapsed();
        })?;
        let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x1b[?2026l");
        let total_elapsed = draw_start.elapsed();
        let changed_cells = previous_frame
            .filter(|previous| previous.area == completed.buffer.area)
            .map(|previous| {
                previous
                    .content
                    .iter()
                    .zip(completed.buffer.content.iter())
                    .filter(|(left, right)| left != right)
                    .count()
            });
        let total_cells = Some(completed.buffer.content.len());
        let completed_buffer = completed.buffer.clone();
        // `completed` borrows the terminal; it is unused past this point, so the
        // borrow ends here (NLL) before we touch the backend again below.
        if sync {
            let _ = crossterm::execute!(terminal.backend_mut(), EndSynchronizedUpdate);
        }
        crate::tui::ui::record_draw_call_attribution(crate::tui::ui::DrawCallAttribution {
            timestamp_ms: crate::tui::ui::wall_clock_ms(),
            total_ms: total_elapsed.as_secs_f64() * 1000.0,
            render_ms: render_elapsed.as_secs_f64() * 1000.0,
            backend_flush_ms: total_elapsed.saturating_sub(render_elapsed).as_secs_f64() * 1000.0,
            changed_cells,
            total_cells,
            force_full_redraw,
            input: crate::tui::ui::frame_input_attribution_snapshot(),
        });
        if crate::tui::ui::last_idle_animation_area().is_some() {
            crate::tui::ui::note_idle_animation_full_repaint();
        }
        self.last_frame = Some(completed_buffer);
        self.last_full_frame_at = Some(Instant::now());
        // A full frame rewrote the whole surface, so ratatui's working buffer no
        // longer matches `last_frame` outside the animated rows. Force the next
        // animation-only repaint to re-seed before it trusts that again.
        self.seeded_animation_area = None;
        // This frame drew the composer as it is now, so the animation-only path
        // may reuse it again until the input changes.
        if self.last_full_frame_input != app.input {
            self.last_full_frame_input.clear();
            self.last_full_frame_input.push_str(&app.input);
        }
        // Close the key-to-paint clock here rather than at render time: the user
        // sees the keystroke when the frame reaches the terminal, so anything
        // before the flush would understate the latency they feel.
        crate::tui::ui::note_frame_painted();
        Ok(())
    }

    pub(super) fn draw_status_spinner_only(
        &mut self,
        app: &App,
        terminal: &mut DefaultTerminal,
    ) -> Result<bool> {
        let status_symbol = status_spinner_only_symbol(app);
        if status_symbol.is_none() {
            return Ok(false);
        }
        let Some(previous_frame) = self.last_frame.as_ref() else {
            return Ok(false);
        };
        let status_area = crate::tui::ui::last_status_area();
        let status_patchable = status_symbol
            .zip(status_area)
            .is_some_and(|(symbol, area)| {
                render_status_spinner_into_buffer(previous_frame, area, symbol)
            });
        if !status_patchable {
            return Ok(false);
        }

        let next_frame = {
            let current_buffer = terminal.current_buffer_mut();
            current_buffer.clone_from(previous_frame);
            if let Some((symbol, area)) = status_symbol.zip(status_area)
                && status_patchable
            {
                render_status_spinner_into_buffer_mut(current_buffer, area, symbol);
            }
            current_buffer.clone()
        };

        // Keep ratatui's virtual buffers authoritative while preserving the user's cursor position.
        // The only terminal mutation outside ratatui here is cursor save/restore; cell contents still
        // go through Terminal::flush so the next full-frame diff remains synchronized. Wrap the
        // single-cell update in a synchronized update so it applies atomically (see issue #282).
        crossterm::queue!(
            terminal.backend_mut(),
            BeginSynchronizedUpdate,
            SavePosition
        )?;
        terminal.flush()?;
        crossterm::queue!(
            terminal.backend_mut(),
            RestorePosition,
            EndSynchronizedUpdate
        )?;
        terminal.swap_buffers();
        terminal.backend_mut().flush()?;
        self.last_frame = Some(next_frame);
        // This path `clone_from`s the whole previous frame and patches one cell,
        // so the animation-only repaint's "everything outside the animated rows
        // is already seeded" assumption no longer holds. Make it re-seed once.
        self.seeded_animation_area = None;
        crate::tui::ui::note_idle_animation_partial_repaint();
        Ok(true)
    }
}

fn render_status_spinner_into_buffer(buffer: &Buffer, area: Rect, symbol: &str) -> bool {
    area.width > 0
        && area.height > 0
        && buffer
            .cell((area.x, area.y))
            .is_some_and(|cell| jcode_tui_style::theme::is_activity_indicator_frame(cell.symbol()))
        && !symbol.is_empty()
}

fn render_status_spinner_into_buffer_mut(buffer: &mut Buffer, area: Rect, symbol: &str) {
    buffer.set_stringn(
        area.x,
        area.y,
        symbol,
        1,
        // The spinner cell is patched outside the full-frame draw, so apply
        // light-theme adaptation here explicitly (no-op on dark themes).
        Style::default().fg(jcode_tui_style::adapt_foreground_for_display(
            jcode_tui_style::theme::ai_color(),
            ratatui::style::Color::Reset,
        )),
    );
}

