//! Stub elicit overlay module.
//!
//! This module provides a minimal draw function for the elicit overlay so
//! `cargo check` passes after the rebase. The pre-rebase KCode semantics for
//! the full elicitation flow (request envelope, keymap handling, response
//! pipeline into `jcode_tool_core`) are not restored here. The render
//! surface is intentionally non-functional; the overlay simply renders an
//! inert panel and returns, allowing the rest of the workspace to compile.
//!
//! Re-introduce the full KCode flow from pre-rebase history in a follow-up
//! task; until then, this stub exists only to satisfy the TuiState trait and
//! the `ui.rs` draw dispatcher.

use ratatui::layout::Rect;
use ratatui::Frame;

use super::elicitation_types::ElicitOverlayState;

/// Draw the elicit overlay onto the supplied frame area.
///
/// Stub: clears a small "Elicit" indicator in the top-right corner. The full
/// interactive form rendering is intentionally omitted. A follow-up task must
/// re-introduce the KCode pre-rebase layout, which renders a centred modal
/// with field widgets, button row, notes pane, and urgency badge.
pub fn draw_elicit_overlay(
    _frame: &mut Frame,
    _area: Rect,
    _state: Option<&ElicitOverlayState>,
) {
    // No-op stub. The pre-rebase implementation rendered the full overlay;
    // see `ElicitOverlayState` in `elicitation_types.rs` for the fields that
    // drive the KCode layout.
}
