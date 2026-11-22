fn report_reload_interaction_gap() {
    let Ok(started) = std::env::var("KCODE_RELOAD_GAP_STARTED_MS") else {
        return;
    };
    crate::env::remove_var("KCODE_RELOAD_GAP_STARTED_MS");
    let Some(started_ms) = started.parse::<u128>().ok() else {
        return;
    };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return;
    };
    let gap_ms = now.as_millis().saturating_sub(started_ms);
    crate::logging::info(&format!(
        "client_reload_interaction_gap_ms={} milestone=first_frame",
        gap_ms
    ));
}
use crate::tui::TuiState;
use crossterm::cursor::{RestorePosition, SavePosition};
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use ratatui::{buffer::Buffer, layout::Rect, style::Style};
use std::io::Write;

const STATUS_SPINNER_FPS: f32 = 12.5;
pub(super) const STATUS_SPINNER_ONLY_INTERVAL: Duration = Duration::from_millis(80);

pub(super) fn redraw_timer(period: Duration) -> tokio::time::Interval {
    let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    // Redraw ticks represent visual liveness, not elapsed simulation steps. An
    // immediate first tick or Burst catch-up after a slow frame only schedules
    // redundant full renders and can lock the UI into a slow-frame loop.
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval
}

pub(super) fn status_spinner_interval() -> tokio::time::Interval {
    status_spinner_interval_after(STATUS_SPINNER_ONLY_INTERVAL)
}

pub(super) fn reset_status_spinner_interval(interval: &mut tokio::time::Interval, app: &App) {
    *interval = status_spinner_interval_after(status_spinner_delay_until_next_frame(
        status_spinner_elapsed(app),
    ));
}

fn status_spinner_interval_after(delay: Duration) -> tokio::time::Interval {
    let mut interval = tokio::time::interval_at(
        tokio::time::Instant::now() + delay,
        STATUS_SPINNER_ONLY_INTERVAL,
    );
    // The spinner is visual liveness, not simulated time. If terminal/input work delays a tick,
    // skip the missed frames instead of bursting them later.
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval
}

fn status_spinner_elapsed(app: &App) -> f32 {
    status_spinner_elapsed_for_sources(app.elapsed().map(|duration| duration.as_secs_f32()))
}

fn status_spinner_elapsed_for_sources(turn_elapsed: Option<f32>) -> f32 {
    turn_elapsed.unwrap_or(0.0).max(0.0)
}

fn status_spinner_delay_until_next_frame(elapsed: f32) -> Duration {
    if !elapsed.is_finite() {
        return STATUS_SPINNER_ONLY_INTERVAL;
    }

    let frame_secs = STATUS_SPINNER_ONLY_INTERVAL.as_secs_f64();
    let elapsed_secs = f64::from(elapsed.max(0.0));
    let into_frame = elapsed_secs % frame_secs;
    let remaining = if into_frame <= f64::EPSILON {
        frame_secs
    } else {
        frame_secs - into_frame
    };

    Duration::from_secs_f64(remaining.max(0.001))
}

pub(super) fn status_spinner_only_symbol(app: &App) -> Option<&'static str> {
    let policy = crate::perf::tui_policy();
    // The single-cell spinner fast path is intentionally available even when
    // decorative animations are disabled (Minimal tier, SSH, WSL, etc.). It
    // patches exactly one status cell between full redraws, so it stays very
    // cheap while keeping the "thinking/connecting/streaming" spinner feeling
    // responsive instead of choppy at the ~1 Hz passive-liveness redraw rate.
    // When decorative animations are off it advances at the smooth liveness
    // rate; otherwise it uses the full-rate spinner clock.
    if !app.is_processing
        || !app.streaming.streaming_text.is_empty()
        || app.centered_mode()
        || app.has_pending_mouse_scroll_animation()
        || app.remote_startup_phase_active()
    {
        return None;
    }

    // Slash suggestions are a late overlay and can cover the recorded status
    // row. Do not let the out-of-band one-cell redraw write through them. Check
    // the cheap prefix first so normal spinner ticks never rebuild suggestions.
    if slash_command_palette_may_be_visible(&app.input, || !app.command_suggestions().is_empty()) {
        return None;
    }

    if status_uses_primary_spinner(&app.status) {
        Some(jcode_tui_style::theme::activity_indicator(
            status_spinner_elapsed(app),
            STATUS_SPINNER_FPS,
            policy.enable_decorative_animations,
        ))
    } else {
        None
    }
}

fn is_slash_command_input(input: &str) -> bool {
    super::has_safe_slash_command_token(input)
}

/// Whether the floating command-suggestion palette may be on screen for the
/// current composer contents.
///
/// The palette is a late overlay (`draw_command_suggestions_overlay`) that
/// floats over the rows directly below the composer without reserving layout
/// height. Partial-repaint fast paths reuse or patch cells from the previous
/// full frame, so they must stand down while the palette is (possibly) up:
/// the one-cell spinner would write through it, and the animation-only repaint
/// resets and redraws the exact rows it floats over on a fresh idle screen.
///
/// `has_suggestions` is lazy so hot paths can check the cheap input prefix
/// first and skip building the suggestion list entirely.
pub(crate) fn slash_command_palette_may_be_visible(
    input: &str,
    has_suggestions: impl FnOnce() -> bool,
) -> bool {
    is_slash_command_input(input) && has_suggestions()
}

/// Statuses whose full status line starts with the primary green circular spinner.
///
/// Keep this in sync with `ui_input::draw_status`: these statuses can be safely
/// refreshed by the one-cell spinner fast path when the status line is left aligned.
/// Network waits use a static amber retry marker, so they do not belong here.
pub(crate) fn status_uses_primary_spinner(status: &ProcessingStatus) -> bool {
    matches!(
        status,
        ProcessingStatus::Sending
            | ProcessingStatus::Connecting(_)
            | ProcessingStatus::Thinking(_)
            | ProcessingStatus::Streaming
            | ProcessingStatus::RunningTool(_)
    )
}

/// How the next full frame should invalidate ratatui's diff state, if at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FullFrameInvalidation {
    /// `Terminal::clear()`: an ED2 Clear-All escape plus a full re-emit.
    /// Needed when the real screen diverged from ratatui's model in cells the
    /// next diff may not repaint (native terminal scroll, external commands).
    HardClear,
    /// Sentinel-invalidate the previous buffer: full re-emit with no
    /// intermediate clear escape, so the repaint stays atomic inside the
    /// synchronized update. Used for scroll-driven repaints (issue #404).
    SoftRepaint,
    /// Normal incremental diff.
    None,
}

/// Pure routing for `draw_full`: a hard clear supersedes a soft repaint.
pub(crate) fn full_frame_invalidation(
    force_full_redraw: bool,
    force_full_repaint: bool,
) -> FullFrameInvalidation {
    if force_full_redraw {
        FullFrameInvalidation::HardClear
    } else if force_full_repaint {
        FullFrameInvalidation::SoftRepaint
    } else {
        FullFrameInvalidation::None
    }
}

/// A cell no real frame produces: a Unicode noncharacter symbol with an
/// improbable style, so a diff against it sees every cell as changed.
fn full_repaint_sentinel_cell() -> ratatui::buffer::Cell {
    let mut cell = ratatui::buffer::Cell::EMPTY;
    cell.set_symbol("\u{FDD0}");
    cell.fg = ratatui::style::Color::Rgb(1, 2, 3);
    cell.bg = ratatui::style::Color::Rgb(3, 2, 1);
    cell
}

/// Fill ratatui's "previous" buffer with sentinel cells so the next
/// `Terminal::draw` diff re-emits every cell.
///
/// This is the flicker-free alternative to `Terminal::clear()` for repaints
/// that need full cell coverage (ratatui #2357 wide-grapheme ghosts on
/// scroll) but not a real screen wipe: `Terminal::clear()` emits an ED2
/// Clear-All escape before the frame is redrawn, and terminals that paint
/// image placeholder cells non-atomically flash blank during the
/// clear-then-repaint on every scroll tick (issue #404). Overwriting every
/// cell in place inside the surrounding synchronized update repaints
/// atomically instead.
pub(crate) fn invalidate_previous_terminal_buffer<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
) {
    // `swap_buffers` resets the inactive buffer and flips. Two swaps with a
    // sentinel fill in between leave: previous = all-sentinel, current = reset
    // and ready for the next `draw`.
    terminal.swap_buffers();
    let sentinel = full_repaint_sentinel_cell();
    for cell in terminal.current_buffer_mut().content.iter_mut() {
        *cell = sentinel.clone();
    }
    terminal.swap_buffers();
}

/// State the animation-only fast path consults before it may run, snapshotted
/// so the decision itself is a pure function (see
/// [`idle_animation_fast_path_blocked_reason`]).
struct IdleAnimationFastPathInputs {
    has_previous_frame: bool,
    animation_active: bool,
    has_animation_area: bool,
    force_full_redraw: bool,
    force_full_repaint: bool,
    composer_changed: bool,
    command_palette_visible: bool,
}

/// Why the animation-only partial repaint must not run, or `None` when it may.
///
/// Pure so the precedence and each individual guard are directly testable;
/// `idle_animation_only_available` feeds it live state and reports the reason
/// to `draw-stats`.
fn idle_animation_fast_path_blocked_reason(
    inputs: &IdleAnimationFastPathInputs,
) -> Option<&'static str> {
    if !inputs.has_previous_frame {
        Some("no_previous_frame")
    } else if !inputs.animation_active {
        Some("animation_inactive")
    } else if !inputs.has_animation_area {
        Some("no_animation_area")
    } else if inputs.force_full_redraw {
        Some("force_full_redraw")
    } else if inputs.force_full_repaint {
        Some("force_full_repaint")
    } else if inputs.composer_changed {
        // The user typed (or edited) since the last full frame. This path
        // reuses that frame outside the animation rows, so it physically
        // cannot show the new input line; taking it would consume the redraw
        // the keystroke requested and defer the glyph to a later full frame.
        Some("input_changed")
    } else if inputs.command_palette_visible {
        // The command palette floats over the rows directly below the
        // composer, which on a fresh idle screen are the animation rows.
        // This path resets and redraws exactly those rows, so it would erase
        // the palette milliseconds after every chrome full frame painted it:
        // pressing `/` in a fresh session made the menu blink in and out at
        // the chrome full-frame cadence. Stand down until the palette closes.
        Some("command_palette_visible")
    } else {
        None
    }
}

#[derive(Default)]
pub(super) struct StatusSpinnerRenderer {
    last_frame: Option<Buffer>,
    last_full_frame_at: Option<Instant>,
    /// Animated rectangle whose surrounding cells are currently seeded into
    /// ratatui's working buffer.
    ///
    /// The animation-only repaint used to `clone_from` the whole previous frame
    /// every tick just to re-render one rectangle over it: ~920k cell copies a
    /// second at 60fps on a 160x48 terminal, to update ~2200 cells. Once the
    /// working buffer has been seeded for a given rectangle, everything outside it
    /// is already correct (this path is the only writer between full frames), so
    /// later work can copy just those rows while that seed remains live. Cleared
    /// by [`Self::invalidate`] and after every buffer swap, because ratatui resets
    /// the next working buffer as part of the swap.
    seeded_animation_area: Option<Rect>,
    /// Composer contents as of the last full frame.
    ///
    /// The animation-only path reuses that frame for everything except the
    /// decorative rows, so it cannot show a newer input line. Without this it
    /// would "satisfy" the redraw a keystroke asked for while omitting the
    /// character, which then waited for a later full frame. Measured against a
    /// live client: typing echoed to the terminal in ~7ms but only reached the
    /// screen ~500ms later; with the guard the same keystroke paints in ~6ms.
    last_full_frame_input: String,
}

