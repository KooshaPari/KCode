use ratatui::{prelude::*, widgets::Paragraph};
use std::sync::atomic::{AtomicUsize, Ordering};

use kcode_core::panic_util::panic_payload_to_string;

use super::layout_support::clear_area;
use super::theme_support::dim_color;

/// Number of recovered panics while rendering the frame.
static DRAW_PANIC_COUNT: AtomicUsize = AtomicUsize::new(0);

pub(super) fn render_recovered_panic_frame(
    frame: &mut Frame,
    payload: &(dyn std::any::Any + Send),
) {
    // L3.draw_recovery: refuse to touch the buffer when the area is degenerate.
    // The cricket incident (2026-09-18) hit a 57x1 terminal and the recovery
    // path itself re-panicked inside `Buffer::index`; bailing out here keeps
    // the recovery function from re-entering the same panic class.
    //   - width < 4: too narrow to render the "rendering error recovered"
    //     modal without truncation
    //   - height < 2: 1-row frames are degenerate for most widgets (no
    //     room for a Block with borders, and many Paragraph layouts
    //     misbehave)
    let area = frame.area();
    if area.width < 4 || area.height < 2 {
        return;
    }
    let panic_count = DRAW_PANIC_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    let msg = panic_payload_to_string(payload);
    if panic_count <= 3 || panic_count.is_multiple_of(50) {
        crate::logging::error(&format!(
            "Recovered TUI draw panic #{}: {}",
            panic_count, msg
        ));
    }
    let area = frame.area().intersection(*frame.buffer_mut().area());
    if area.width == 0 || area.height == 0 {
        return;
    }
    clear_area(frame, area);
    let lines = vec![
        Line::from(Span::styled(
            "rendering error recovered",
            Style::default().fg(Color::Red),
        )),
        Line::from(Span::styled(
            "continuing with a safe fallback frame",
            Style::default().fg(dim_color()),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    //! Unit tests for the degenerate-frame early-return in
    //! `render_recovered_panic_frame`. The `pub(super)` visibility keeps the
    //! function out of the public crate API, so the integration-level
    //! `tests/ratatui_buffer_guard.rs` only exercises the size-gate helper.
    //! This module exercises the recovery function directly with a
    //! `ratatui::backend::TestBackend` to prove the early-return path
    //! doesn't re-panic on a 57x1 frame.
    use super::*;
    use ratatui::backend::TestBackend;

    /// `render_recovered_panic_frame` must early-return without panicking and
    /// without mutating the buffer when the frame area is smaller than the
    /// minimum it would need to draw its own modal text. Regression test for
    /// the `Buffer::index` panic the recovery function hit during the
    /// `cricket` incident (2026-09-18, 57x1 terminal).
    #[test]
    fn recovery_skips_tiny_frame() {
        let payload: Box<dyn std::any::Any + Send> = Box::new(String::from("synthetic"));
        for (cols, rows) in [(57u16, 1u16), (0u16, 0u16), (3u16, 1u16), (60u16, 0u16)] {
            let mut terminal =
                ratatui::Terminal::new(TestBackend::new(cols, rows)).expect("test backend");
            // Take a snapshot of the empty buffer so we can prove the
            // function did NOT write to it on a degenerate frame.
            let before = terminal.backend().buffer().clone();
            terminal
                .draw(|frame| render_recovered_panic_frame(frame, payload.as_ref()))
                .expect("recovery fn must not panic on degenerate frame");
            let after = terminal.backend().buffer().clone();
            assert_eq!(
                before, after,
                "recovery fn mutated the buffer on a {cols}x{rows} frame",
            );
        }
    }
}
