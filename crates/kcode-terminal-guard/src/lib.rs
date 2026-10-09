//! Pre-flight terminal size gate + panic-safe `ratatui::init` wrapper.
//!
//! Background
//! ----------
//! `ratatui-core` 0.1.0 panicked at `buffer/buffer.rs:250:13` with
//! `index outside of buffer: the area is Rect { x: 0, y: 0, width: 57, height: 1 }
//! but index is (1, 1)` when the TUI tried to write into a single-row buffer
//! produced by a degenerate terminal (e.g. a horizontal split with
//! insufficient height, or a wrapped CI runner).
//!
//! The fix in upstream `ratatui-core` >= 0.1.2 removes that panic for the
//! `Buffer::index` site, but the in-tree callers in kcode had no defense: any
//! path that called `ratatui::init` on a too-small terminal would crash.
//!
//! This crate is the L3 defense-in-depth: a single gate that every TUI entry
//! point (root binary, session picker, permissions viewer) consults before
//! handing control to `ratatui::init`. If the terminal is missing
//! (`(0, 0)`) or below [`MIN_WIDTH`] x [`MIN_HEIGHT`], the function returns
//! an `anyhow::Error` that the caller propagates to the user, with a
//! actionable message ("resize the window or use --no-tui").
//!
//! The wrapper [`init_ratatui_with_size_check`] also turns the
//! `catch_unwind` boundary that was previously duplicated at three call sites
//! into a single panic-to-error mapping, so the size check and the panic
//! guard always travel together.

#![deny(unsafe_code)]
#![warn(missing_docs)]

use anyhow::Result;
use ratatui::DefaultTerminal;

/// Minimum terminal width in columns. Below this, the TUI's modal layout
/// (e.g. the session picker) cannot fit a usable table.
pub const MIN_WIDTH: u16 = 60;

/// Minimum terminal height in rows. The single-row crash reported upstream
/// shows that anything below this is unsafe even after the upstream fix.
pub const MIN_HEIGHT: u16 = 20;

/// Pre-flight check parameterized on raw `(cols, rows)` values.
///
/// This is the testable inner form: production code calls
/// [`check_minimum_terminal_size`] which fetches the size from
/// `crossterm::terminal::size()`; tests call this function with synthetic
/// values to exercise the boundary conditions without spawning a real
/// terminal.
pub fn check_minimum_terminal_size_at(cols: u16, rows: u16) -> Result<()> {
    if (cols, rows) == (0, 0) || cols < MIN_WIDTH || rows < MIN_HEIGHT {
        anyhow::bail!(
            "kcode requires a terminal of at least {MIN_WIDTH}x{MIN_HEIGHT} (got {cols}x{rows}); \
             resize the window or use --no-tui"
        );
    }
    Ok(())
}

/// Pre-flight check using the live terminal size.
///
/// Returns an error if the terminal is missing or below the documented
/// minimums. See [`check_minimum_terminal_size_at`] for the size semantics
/// and for the testable entry point.
pub fn check_minimum_terminal_size() -> Result<()> {
    let (cols, rows) = crossterm::terminal::size()?;
    check_minimum_terminal_size_at(cols, rows)
}

/// Initialize a ratatui default terminal, guarded by the size check and a
/// panic-to-error boundary.
///
/// This replaces the three ad-hoc `catch_unwind(AssertUnwindSafe(ratatui::init))`
/// blocks that previously lived in `src/cli/terminal.rs`,
/// `crates/kcode-tui/src/tui/session_picker.rs`, and
/// `crates/kcode-tui-permissions/src/lib.rs`.
///
/// On success, returns the initialized `DefaultTerminal`. On a too-small
/// terminal, returns the same error that [`check_minimum_terminal_size`]
/// would. On a panic inside `ratatui::init`, returns an `anyhow::Error`
/// containing the panic payload's string form.
pub fn init_ratatui_with_size_check() -> Result<DefaultTerminal> {
    check_minimum_terminal_size()?;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(ratatui::init)).map_err(|payload| {
        let msg = match payload.downcast_ref::<String>() {
            Some(s) => s.clone(),
            None => match payload.downcast_ref::<&'static str>() {
                Some(s) => (*s).to_string(),
                None => "<non-string panic payload>".to_string(),
            },
        };
        anyhow::anyhow!("failed to initialize terminal: {msg}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_accepts_exact_minimums() {
        assert!(check_minimum_terminal_size_at(MIN_WIDTH, MIN_HEIGHT).is_ok());
    }

    #[test]
    fn gate_rejects_zero_size() {
        let err = check_minimum_terminal_size_at(0, 0).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(&format!("{MIN_WIDTH}x{MIN_HEIGHT}")), "{msg}");
        assert!(msg.contains("0x0"), "{msg}");
    }

    #[test]
    fn gate_rejects_narrow_one_row() {
        let err = check_minimum_terminal_size_at(57, 1).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("57x1"), "{msg}");
    }

    #[test]
    fn gate_rejects_short_one_column() {
        let err = check_minimum_terminal_size_at(80, 5).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("80x5"), "{msg}");
    }

    #[test]
    fn gate_accepts_normal_terminal() {
        assert!(check_minimum_terminal_size_at(80, 24).is_ok());
    }

    #[test]
    fn gate_accepts_oversized() {
        assert!(check_minimum_terminal_size_at(200, 60).is_ok());
    }
}
