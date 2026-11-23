//! HERDR terminal runtime integration for jcode and ForgeCode.
//!
//! Provides lifecycle state reporting (working/idle/blocked) to HERDR's
//! Unix socket API, session identity for restore, and screen manifest
//! generation for agent detection.
//!
//! # Architecture
//!
//! When `HERDR_ENV=1` (legacy HERDR), the [`HerdrReporter`] connects to
//! HERDR's Unix socket and reports agent state transitions via
//! `pane.report_agent`. A monotonic sequence number ensures stale reports
//! from predecessor sessions are silently dropped by HERDR.
//!
//! herdr-core (HC) has no report verb. It reads status from OSC 7501 records
//! a program prints on its own terminal output, so inside HC the reporter
//! prints records instead of pushing socket frames: [`HerdrReporter::is_push_capable`]
//! stays `false` while [`HerdrReporter::is_osc_capable`] is `true`. The encoder
//! and the wire format live in [`osc`].
//!
//! When no terminal runtime is detected, all operations are zero-cost no-ops.
//!
//! [`global`] owns the one process-wide reporter. The binary and the TUI both
//! report through it, so there is a single `OnceLock` and a single owner of
//! runtime state.

pub mod env;
pub mod global;
pub mod manifest;
pub mod osc;
pub mod reporter;
pub mod socket;
pub mod state;

pub use env::{HerdrEnv, RuntimeKind};
pub use osc::{HcKind, HcState};
pub use reporter::HerdrReporter;
pub use state::AgentState;
