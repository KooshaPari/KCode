#![allow(
    unknown_lints,
    clippy::collapsible_match,
    clippy::manual_checked_ops,
    clippy::unnecessary_sort_by,
    clippy::useless_conversion
)]

//! Presentation layer for jcode (terminal UI + offline replay export).
//!
//! This crate holds the `tui` and `video_export` modules that were extracted
//! out of the monolithic root `jcode` crate so they compile as a separate
//! rustc unit. The application core it builds on (server, agent, provider,
//! auth, session, tool, config, ...) lives in `jcode-app-core` and is
//! re-exported here via `pub use jcode_app_core::*`, so every existing
//! `crate::<module>` path (e.g. `crate::config`, `crate::server`) keeps
//! resolving unchanged across the tui code. The root `jcode` crate (cli + bin)
//! re-exports this crate via `pub use jcode_tui::*`.

// Application core: re-export every `jcode-app-core` module (which itself
// re-exports `jcode-base`) so `crate::<module>` paths resolve here exactly as
// they did before the split.
pub use jcode_app_core::*;

// Presentation layer (kept in this crate).
pub mod tui;
pub mod video_export;

#[cfg(test)]
mod herdr_global_tests {
    use jcode_herdr::{AgentState, global};

    /// The TUI reports lifecycle state through `jcode_herdr::global`, the same
    /// reporter the binary initializes at startup.
    ///
    /// It used to report through `crate::herdr`, a byte-identical local copy of
    /// the binary's module with its own `OnceLock` that nothing ever set, so
    /// every TUI state push (`Working` on turn start, `Blocked` on elicit,
    /// `Idle` on turn end) was silently dropped. Asserting the symbol identity
    /// fails if a local wrapper is reintroduced.
    #[test]
    fn reports_through_the_shared_global_reporter() {
        let name = std::any::type_name_of_val(&global::report_state);
        assert!(
            name.contains("jcode_herdr::global::report_state"),
            "TUI must report through the canonical global, got {name}"
        );
    }

    /// Reporting before any `init` is a no-op, not a panic or a hang: the TUI
    /// can fire a state push from an uninitialized process (tests, replay,
    /// `video_export`) without a started reporter.
    #[tokio::test]
    async fn reporting_before_init_is_harmless() {
        global::report_state(AgentState::Working).await;
        global::report_session_id("test-session".to_string()).await;
        global::on_session_start().await;
        let _ = global::is_active();
    }
}
