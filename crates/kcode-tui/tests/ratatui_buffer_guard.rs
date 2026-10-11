//! Regression tests for the L3 ratatui buffer-overflow guard.
//!
//! These cover the cricket incident (2026-09-18) where `ratatui-core` 0.1.0
//! panicked at `buffer/buffer.rs:250:13` with `index outside of buffer` when
//! the TUI was driven into a 57x1 terminal pane.
//!
//! Test strategy
//! -------------
//! The size gate is exercised against `kcode_terminal_guard::check_minimum_terminal_size_at`,
//! which is the parameterised inner form of the gate. It takes `(cols, rows)`
//! directly so we can drive boundary conditions without spawning a real
//! terminal (option `c` from the spec). The real-terminal `check_minimum_terminal_size`
//! is the live wrapper that production calls.
//!
//! The recovery function (`render_recovered_panic_frame`) is exercised by a
//! unit test inside `crates/kcode-tui/src/tui/ui/draw_recovery.rs::tests`
//! rather than here. The function is intentionally `pub(super)` so the
//! kcode-tui public API surface stays small, which means an integration
//! test under `tests/` would need a `pub` re-export of recovery machinery
//! (or a feature-gated re-export) just to satisfy the visibility chain.
//! Keeping that test as a `#[cfg(test)] mod tests` inside the same file
//! preserves the privacy boundary. Total regression-test count across both
//! files is still five: four gate tests here, one recovery test in the
//! unit-test module.

use kcode_terminal_guard::{MIN_HEIGHT, MIN_WIDTH, check_minimum_terminal_size_at};

/// 1. A 0x0 terminal must be rejected. This is the canonical "no TTY" case
///    (piped stdin / non-interactive CI / detached tmux pane) and it must
///    bail with an actionable error mentioning the required minimums.
#[test]
fn gate_rejects_zero_size() {
    let err = check_minimum_terminal_size_at(0, 0)
        .err()
        .expect("0x0 must be rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains(&format!("{MIN_WIDTH}x{MIN_HEIGHT}")),
        "error must mention the required nonzero size, got: {msg}"
    );
    assert!(
        msg.contains("0x0"),
        "error must echo the actual size, got: {msg}"
    );
}

/// Nonzero tiny panes must remain usable by the compact picker.
#[test]
fn gate_accepts_tiny_panes() {
    for (cols, rows) in [(57, 1), (38, 14), (20, 5), (1, 1)] {
        assert!(check_minimum_terminal_size_at(cols, rows).is_ok());
    }
}

/// 3. The exact minimum (1x1) must be accepted. This is the boundary;
///    an off-by-one here would force users to resize unnecessarily.
#[test]
fn gate_accepts_exact_minimum() {
    assert!(
        check_minimum_terminal_size_at(MIN_WIDTH, MIN_HEIGHT).is_ok(),
        "exact minimums ({MIN_WIDTH}x{MIN_HEIGHT}) must be accepted"
    );
}

/// 4. A typical 80x24 terminal must be accepted with no surprises. This
///    covers the common case of a 24-line terminal at 80 columns.
#[test]
fn gate_accepts_normal_terminal() {
    assert!(
        check_minimum_terminal_size_at(80, 24).is_ok(),
        "80x24 must be accepted"
    );
}
