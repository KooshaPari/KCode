//! HERDR terminal runtime integration.
//!
//! Thin re-export of [`jcode_tui::herdr`] so the CLI entrypoint
//! (`cli::startup`) and the presentation layer share the **same** global
//! [`jcode_herdr::HerdrReporter`].
//!
//! # Why this is a re-export and not its own reporter
//!
//! This module used to define its own `static REPORTER: OnceLock<...>`,
//! duplicating `jcode_tui::herdr`. The CLI initialized *this* copy in
//! `cli::startup`, while every lifecycle transition (`working` / `idle` /
//! `blocked`) was emitted through the *jcode-tui* copy via
//! `jcode_tui::herdr::spawn_report`. Because the two statics were
//! independent, the TUI reporter was never initialized, so no
//! `pane.report_agent` request was ever sent — jcode connected to the
//! HERDR socket only to `pane.release_agent` on shutdown, and never
//! appeared in `herdr agent list`.
//!
//! Re-exporting the jcode-tui module guarantees a single reporter
//! instance: whatever the CLI initializes is exactly what the TUI uses.

pub use jcode_tui::herdr::*;
