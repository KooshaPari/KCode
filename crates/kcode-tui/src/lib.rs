#![allow(
    unknown_lints,
    clippy::collapsible_match,
    clippy::manual_checked_ops,
    clippy::unnecessary_sort_by,
    clippy::useless_conversion
)]

//! Presentation layer for kcode (terminal UI + offline replay export).
//!
//! This crate holds the `tui` and `video_export` modules that were extracted
//! out of the monolithic root `kcode` crate so they compile as a separate
//! rustc unit. The application core it builds on (server, agent, provider,
//! auth, session, tool, config, ...) lives in `kcode-app-core` and is
//! re-exported here via `pub use kcode_app_core::*`, so every existing
//! `crate::<module>` path (e.g. `crate::config`, `crate::server`) keeps
//! resolving unchanged across the tui code. The root `kcode` crate (cli + bin)
//! re-exports this crate via `pub use kcode_tui::*`.

// Application core: re-export every `kcode-app-core` module (which itself
// re-exports `kcode-base`) so `crate::<module>` paths resolve here exactly as
// they did before the split.
pub use kcode_app_core::*;

// Presentation layer (kept in this crate).
pub mod herdr;
pub mod tui;
pub mod video_export;
