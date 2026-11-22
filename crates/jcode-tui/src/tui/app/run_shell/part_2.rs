impl App {
    /// Run the TUI application
    /// Returns Some(session_id) if hot-reload was requested
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<RunResult> {
        super::terminal_liveness::capture_initial_tty();
        let mut event_stream = EventStream::new();
        let mut redraw_period = crate::tui::redraw_interval(&self);
        let mut redraw_interval = redraw_timer(redraw_period);
        let mut status_spinner_interval = status_spinner_interval();
        let mut status_spinner_renderer = StatusSpinnerRenderer::default();
        let mut needs_redraw = true;
        let mut first_frame_reported = false;
        let mut handterm_native_scroll =
            super::handterm_native_scroll::HandtermNativeScrollClient::connect_from_env();
        // Subscribe to bus for background task completion notifications
        let mut bus_receiver = Bus::global().subscribe();
        if let Some(status) = Bus::global().latest_update_status() {
            self.handle_update_status(status);
        }

        loop {
            self.sync_sleep_guard();
            let desired_redraw = crate::tui::redraw_interval(&self);
            if desired_redraw != redraw_period {
                redraw_period = desired_redraw;
                redraw_interval = redraw_timer(redraw_period);
            }

            if needs_redraw {
                // On an idle animated screen, repaint just the animation rows
                // when nothing else is due. This is the single draw site, so
                // gating here covers every redraw source (ticks, input, bus
                // events), not just the animation tick.
                if status_spinner_renderer.idle_animation_only_available(&self)
                    && status_spinner_renderer.draw_idle_animation_only(&self, &mut terminal)?
                {
                    needs_redraw = false;
                } else {
                    status_spinner_renderer.draw_full(&mut self, &mut terminal)?;
                    if !first_frame_reported {
                        first_frame_reported = true;
                        report_reload_interaction_gap();
                    }
                    reset_status_spinner_interval(&mut status_spinner_interval, &self);
                    if let Some(native) = handterm_native_scroll.as_mut() {
                        native.sync_from_app(&self);
                    }
                    needs_redraw = false;
                }
            }

            if self.should_quit {
                break;
            }

            // Process pending turn OR wait for input/redraw
            if self.pending_turn {
                self.pending_turn = false;
                // Process turn while still handling input
                self.process_turn_with_input(&mut terminal, &mut event_stream, &mut bus_receiver)
                    .await;
                needs_redraw = true;
            } else if self.pending_queued_dispatch {
                self.pending_queued_dispatch = false;
                self.process_queued_messages(&mut terminal, &mut event_stream)
                    .await;
                local::finish_turn(&mut self);
                needs_redraw = true;
            } else {
                // Wait for input or redraw tick
                tokio::select! {
                    // Declaration-order polling: user input outranks timers and
                    // bus chatter (see the remote loop for the rationale).
                    biased;
                    event = event_stream.next() => {
                        if event.is_some() {
                            needs_redraw |= local::handle_terminal_event(&mut self, &mut terminal, event)?;
                        } else if super::terminal_liveness::terminal_abandoned() {
                            // Input EOF and the controlling terminal is gone:
                            // this client is an orphan (window died without a
                            // deliverable SIGHUP). Exit instead of looping
                            // forever holding ~100 MB. The session persists
                            // and can be resumed.
                            crate::logging::warn(
                                "Terminal input closed and controlling terminal is gone; exiting orphaned client",
                            );
                            self.should_quit = true;
                        } else {
                            tokio::time::sleep(redraw_period).await;
                        }
                    }
                    _ = status_spinner_interval.tick(), if status_spinner_renderer.spinner_only_available(&self) => {
                        if !status_spinner_renderer.draw_status_spinner_only(&self, &mut terminal)? {
                            needs_redraw = true;
                        }
                    }
                    _ = redraw_interval.tick() => {
                        needs_redraw |= local::handle_tick(&mut self);
                    }
                    command = async {
                        match handterm_native_scroll.as_mut() {
                            Some(native) => native.recv().await,
                            None => futures::future::pending::<Option<super::handterm_native_scroll::HostToApp>>().await,
                        }
                    } => {
                        if let Some(command) = command {
                            self.apply_handterm_native_scroll(command);
                            self.request_full_redraw();
                            needs_redraw = true;
                        } else {
                            handterm_native_scroll = None;
                        }
                    }
                    // Handle background task completion notifications
                    bus_event = bus_receiver.recv() => {
                        needs_redraw |= local::handle_bus_event(&mut self, bus_event);
                    }
                    // Handle elicitation requests from tool-core (agent → user)
                    elicit_msg = self.elicit_rx.recv() => {
                        if let Some(msg) = elicit_msg {
                            self.handle_elicit_msg(msg);
                            needs_redraw = true;
                        }
                    }
                }
            }
        }

        self.extract_session_memories().await;

        Ok(RunResult {
            reload_session: self.reload_requested.take(),
            rebuild_session: self.rebuild_requested.take(),
            update_session: self.update_requested.take(),
            restart_session: self.restart_requested.take(),
            cloud_handoff: self.cloud_handoff_requested.take(),
            exit_code: self.requested_exit_code,
            session_id: Some(self.session.id.clone()),
        })
    }

    /// Run the TUI in remote mode, connecting to a server
    pub async fn run_remote(
        mut self,
        mut terminal: DefaultTerminal,
        remote_working_dir: Option<String>,
    ) -> Result<RunResult> {
        if crate::tui::is_ssh_remote() {
            self.session.working_dir = remote_working_dir.clone();
        }
        super::terminal_liveness::capture_initial_tty();
        let mut event_stream = EventStream::new();
        let mut redraw_period = crate::tui::redraw_interval(&self);
        let mut redraw_interval = redraw_timer(redraw_period);
        let mut status_spinner_interval = status_spinner_interval();
        let mut status_spinner_renderer = StatusSpinnerRenderer::default();
        let mut needs_redraw = true;
        let mut first_frame_reported = false;
        // While unfocused and idle, redraws are throttled to this interval so a
        // backgrounded session does not repaint at full rate on shared-server bus
        // chatter. `None` means "no throttled frame drawn yet since losing focus".
        const UNFOCUSED_IDLE_REDRAW_MIN_INTERVAL: std::time::Duration =
            std::time::Duration::from_millis(1000);
        let mut last_unfocused_draw: Option<std::time::Instant> = None;
        let mut handterm_native_scroll =
            super::handterm_native_scroll::HandtermNativeScrollClient::connect_from_env();
        let mut remote_state = remote::RemoteRunState::default();

        'outer: loop {
            if self.display_messages.is_empty() {
                if self.server_spawning {
                    self.set_remote_startup_phase(super::RemoteStartupPhase::StartingServer);
                } else {
                    self.set_remote_startup_phase(super::RemoteStartupPhase::Connecting);
                }
            }
            if needs_redraw {
                status_spinner_renderer.draw_full(&mut self, &mut terminal)?;
                if !first_frame_reported {
                    first_frame_reported = true;
                    report_reload_interaction_gap();
                }
                // Close the startup-profile gap: `pre_run_remote` is the last
                // pre-loop mark, so the first completed paint here is the real
                // process-to-first-frame point. Logged once via a static guard so
                // the end-to-end launch cost (including the ~5ms first draw) is
                // visible in the startup profile without re-marking every frame.
                {
                    use std::sync::atomic::{AtomicBool, Ordering};
                    static FIRST_FRAME_MARKED: AtomicBool = AtomicBool::new(false);
                    if !FIRST_FRAME_MARKED.swap(true, Ordering::Relaxed) {
                        crate::startup_profile::mark("first_frame");
                        crate::startup_profile::report_to_log();
                    }
                }
                reset_status_spinner_interval(&mut status_spinner_interval, &self);
                needs_redraw = false;
            }

            let session_to_resume = self.reconnect_target_session_id();

            let mut remote_conn = match remote::connect_with_retry(
                &mut self,
                &mut terminal,
                &mut event_stream,
                &mut remote_state,
                session_to_resume.as_deref(),
                remote_working_dir.as_deref(),
            )
            .await?
            {
                remote::ConnectOutcome::Connected(remote) => remote,
                remote::ConnectOutcome::Retry => continue,
                remote::ConnectOutcome::Quit => break 'outer,
            };
            status_spinner_renderer.invalidate();

            match remote::handle_post_connect(
                &mut self,
                &mut terminal,
                &mut remote_conn,
                &mut remote_state,
                session_to_resume.as_deref(),
            )
            .await?
            {
                remote::PostConnectOutcome::Ready => {}
                remote::PostConnectOutcome::Quit => break 'outer,
            }
            status_spinner_renderer.invalidate();
            needs_redraw = true;

            let mut bus_receiver_remote = Bus::global().subscribe();
            if let Some(status) = Bus::global().latest_update_status() {
                self.handle_update_status(status);
                needs_redraw = true;
            }

            // Main event loop
            loop {
                self.sync_sleep_guard();
                let desired_redraw = crate::tui::redraw_interval(&self);
                if desired_redraw != redraw_period {
                    redraw_period = desired_redraw;
                    redraw_interval = redraw_timer(redraw_period);
                }

                if needs_redraw {
                    // Throttle idle full-frame renders while the terminal is
                    // backgrounded (FocusLost). An unfocused, idle session has
                    // nothing changing worth a 60fps repaint, so it should not
                    // repaint at full rate just because other sessions on the
                    // shared server broadcast bus updates -- that is what made a
                    // swarm of background windows saturate the CPU. We keep full-
                    // rate redraws while streaming/processing so visible-but-
                    // unfocused windows in a tiling WM still show live progress,
                    // and set_client_focused(true) forces a full repaint on refocus.
                    let allow_redraw = self.client_focused()
                        || self.unfocused_redraw_warranted()
                        || last_unfocused_draw
                            .map(|t| t.elapsed() >= UNFOCUSED_IDLE_REDRAW_MIN_INTERVAL)
                            .unwrap_or(true);
                    if allow_redraw {
                        // Idle animated screen: repaint only the animation rows
                        // when nothing else is due (see the local loop).
                        if status_spinner_renderer.idle_animation_only_available(&self)
                            && status_spinner_renderer
                                .draw_idle_animation_only(&self, &mut terminal)?
                        {
                            needs_redraw = false;
                        } else {
                            status_spinner_renderer.draw_full(&mut self, &mut terminal)?;
                            reset_status_spinner_interval(&mut status_spinner_interval, &self);
                            if let Some(native) = handterm_native_scroll.as_mut() {
                                native.sync_from_app(&self);
                            }
                            last_unfocused_draw =
                                (!self.client_focused()).then(std::time::Instant::now);
                            needs_redraw = false;
                        }
                    }
                    // When unfocused and throttled, leave needs_redraw set so the
                    // pending update is coalesced into the next allowed frame.
                }

                if self.should_quit {
                    break 'outer;
                }

                if self.pending_queued_dispatch
                    && !remote::has_unresolved_for_session(&self, &remote_state)
                {
                    self.pending_queued_dispatch = false;
                    remote::process_remote_followups_guarded(
                        &mut self,
                        &mut remote_conn,
                        &remote_state,
                    )
                    .await;
                    needs_redraw = true;
                    continue;
                }

                tokio::select! {
                    // Poll in declaration order so user input always wins the
                    // race against server/bus chatter. During heavy streaming
                    // the remote event branch is almost always ready; with the
                    // default random polling it repeatedly outcompetes buffered
                    // keystrokes, which shows up as a laggy, stuttering input
                    // line while a turn is running.
                    biased;
                    event = event_stream.next() => {
                        if event.is_some() {
                            needs_redraw |= remote::handle_terminal_event(&mut self, &mut remote_conn, &mut remote_state, event).await?;
                        } else if super::terminal_liveness::terminal_abandoned() {
                            // Input EOF with the controlling terminal gone:
                            // orphaned client (see local loop). Exit; the
                            // server-side session keeps running and can be
                            // reattached with --resume.
                            crate::logging::warn(
                                "Terminal input closed and controlling terminal is gone; exiting orphaned client",
                            );
                            self.should_quit = true;
                        } else {
                            tokio::time::sleep(redraw_period).await;
                        }
                    }
                    _ = status_spinner_interval.tick(), if status_spinner_renderer.spinner_only_available(&self) => {
                        if !status_spinner_renderer.draw_status_spinner_only(&self, &mut terminal)? {
                            needs_redraw = true;
                        }
                    }
                    _ = redraw_interval.tick() => {
                        needs_redraw |= remote::handle_tick(&mut self, &mut remote_conn, &mut remote_state).await;
                    }
                    event = remote_conn.next_event() => {
                        let (outcome, event_redraw) = remote::handle_remote_event(
                            &mut self,
                            &mut terminal,
                            &mut remote_conn,
                            &mut remote_state,
                            event,
                        )
                        .await?;
                        needs_redraw |= event_redraw;
                        match outcome {
                            remote::RemoteEventOutcome::Continue => {}
                            remote::RemoteEventOutcome::Reconnect => continue 'outer,
                            remote::RemoteEventOutcome::PauseProtocol => {
                                let reason = crate::tui::backend::ssh_gate::LocalSshGateError::ProtocolFailure;
                                match remote::pause_after_protocol_error(&mut self, &mut terminal, &mut event_stream, &mut remote_state, &reason).await? {
                                    remote::ConnectOutcome::Quit => break 'outer,
                                    _ => continue 'outer,
                                }
                            }
                            remote::RemoteEventOutcome::Quit => break 'outer,
                        }
                    }
                    command = async {
                        match handterm_native_scroll.as_mut() {
                            Some(native) => native.recv().await,
                            None => futures::future::pending::<Option<super::handterm_native_scroll::HostToApp>>().await,
                        }
                    } => {
                        if let Some(command) = command {
                            self.apply_handterm_native_scroll(command);
                            self.request_full_redraw();
                            needs_redraw = true;
                        } else {
                            handterm_native_scroll = None;
                        }
                    }
                    bus_event = bus_receiver_remote.recv() => {
                        needs_redraw |= remote::handle_bus_event(&mut self, &mut remote_conn, bus_event, &mut remote_state).await;
                    }
                    // Handle elicitation requests from tool-core (agent → user)
                    elicit_msg = self.elicit_rx.recv() => {
                        if let Some(msg) = elicit_msg {
                            self.handle_elicit_msg(msg);
                            needs_redraw = true;
                        }
                    }
                }
            }
        }

        Ok(RunResult {
            reload_session: self.reload_requested.take(),
            rebuild_session: self.rebuild_requested.take(),
            update_session: self.update_requested.take(),
            restart_session: self.restart_requested.take(),
            cloud_handoff: self.cloud_handoff_requested.take(),
            exit_code: self.requested_exit_code,
            session_id: if self.is_remote {
                self.remote_session_id.clone()
            } else {
                Some(self.session.id.clone())
            },
        })
    }

    /// Run the TUI in replay mode, playing back a timeline of events.
    pub async fn run_replay(
        self,
        terminal: DefaultTerminal,
        timeline: Vec<crate::replay::TimelineEvent>,
        speed: f64,
    ) -> Result<RunResult> {
        replay::run_replay(self, terminal, timeline, speed).await
    }

    /// Run an interactive swarm replay, rendering multiple sessions in tiled panes.
    pub async fn run_swarm_replay(
        terminal: DefaultTerminal,
        panes: Vec<crate::replay::PaneReplayInput>,
        speed: f64,
        centered_override: Option<bool>,
    ) -> Result<()> {
        replay::run_swarm_replay(terminal, panes, speed, centered_override).await
    }

    /// Run replay headlessly, rendering each frame to an in-memory buffer.
    /// Returns a list of (timestamp_secs, Buffer) pairs for video export.
    pub async fn run_headless_replay(
        mut self,
        timeline: &[crate::replay::TimelineEvent],
        speed: f64,
        width: u16,
        height: u16,
        fps: u32,
    ) -> Result<Vec<(f64, ratatui::buffer::Buffer)>> {
        use crate::replay::ReplayEvent;
        use ratatui::backend::TestBackend;

        let replay_events = crate::replay::timeline_to_replay_events(timeline);
        if replay_events.is_empty() {
            anyhow::bail!("No replay events to export");
        }

        let backend = TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend)?;
        let mut remote = crate::tui::backend::ReplayRemoteState::default();

        let frame_duration_ms: f64 = 1000.0 / fps as f64;
        let mut frames: Vec<(f64, ratatui::buffer::Buffer)> = Vec::new();
        let mut sim_time_ms: f64 = 0.0;
        let mut next_frame_at: f64 = 0.0;

        let total_duration_ms: f64 = replay_events.iter().map(|(d, _)| *d as f64 / speed).sum();

        let mut event_schedule: Vec<(f64, &ReplayEvent)> = Vec::new();
        {
            let mut abs_time: f64 = 0.0;
            for (delay_ms, evt) in &replay_events {
                abs_time += *delay_ms as f64 / speed;
                event_schedule.push((abs_time, evt));
            }
        }

        let mut event_cursor: usize = 0;
        let mut replay_turn_id: u64 = 0;

        let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x1b[?2026h");
        terminal.draw(|f| crate::tui::render_frame(f, &self))?;
        let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x1b[?2026l");
        frames.push((0.0, terminal.backend().buffer().clone()));

        let progress_interval = (total_duration_ms / 20.0).max(1000.0);
        let mut next_progress = progress_interval;

        while sim_time_ms <= total_duration_ms + frame_duration_ms {
            while event_cursor < event_schedule.len()
                && event_schedule[event_cursor].0 <= sim_time_ms
            {
                let (_t, event) = event_schedule[event_cursor];
                replay::apply_replay_event(
                    &mut self,
                    &mut remote,
                    event,
                    &mut replay_turn_id,
                    Some(sim_time_ms),
                );
                event_cursor += 1;
            }

            if sim_time_ms >= next_frame_at {
                replay::update_replay_elapsed_override(&mut self, sim_time_ms);
                let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x1b[?2026h");
                terminal.draw(|f| crate::tui::render_frame(f, &self))?;
                let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x1b[?2026l");
                frames.push((sim_time_ms / 1000.0, terminal.backend().buffer().clone()));
                next_frame_at = sim_time_ms + frame_duration_ms;
            }

            if sim_time_ms >= next_progress {
                let pct = (sim_time_ms / total_duration_ms * 100.0).min(100.0);
                eprint!("\r  Rendering... {:.0}%", pct);
                next_progress += progress_interval;
            }

            sim_time_ms += frame_duration_ms;
        }

        eprintln!("\r  Rendering... 100%  ({} frames captured)", frames.len());

        Ok(frames)
    }
}
