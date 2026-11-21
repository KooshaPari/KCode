//! HERDR terminal runtime integration.
//!
//! Thin facade over [`jcode_herdr::global`], which owns the one process-wide
//! reporter. This module exists so the binary's existing `crate::herdr::` call
//! sites keep their names; it holds no state and duplicates no logic.
//!
//! All operations are no-ops when jcode is not running inside a HERDR pane.

use jcode_herdr::HerdrEnv;

pub use jcode_herdr::global::{
    init, init_with_env, is_active, on_session_start, report_session_id, report_state, shutdown,
};

/// Force-enable HERDR reporting for this process without mutating its
/// environment.
///
/// Mirrors the `--herdr` flag: the runtime already captured from the
/// environment is marked active even when `HERDR_ENV` is unset. No pane
/// identity is invented, so with no `HERDR_PANE_ID` and no `HERDR_SOCKET_PATH`
/// the reporter still reports itself inactive and pushes nothing.
pub fn init_forced(agent_label: &str) {
    init_with_env(agent_label, HerdrEnv::capture().with_forced_active());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression guard for the mutation this module used to perform.
    ///
    /// `init_forced` used to call `unsafe { std::env::set_var("HERDR_ENV", "1") }`,
    /// which renamed the process's own runtime for every later reader:
    /// `jcode-tui-mermaid` hid the outer terminal, and every child process
    /// spawned afterwards inherited a HERDR claim that was never true. The
    /// flag must select a runtime, not forge one.
    #[test]
    fn init_forced_does_not_mutate_process_env() {
        let key = "HERDR_ENV";
        let before = std::env::var(key).ok();

        // Keep the global `OnceLock` out of the way: this test is about the
        // environment, and the reporter itself is initialized elsewhere.
        init_forced("env-mutation-guard");

        let after = std::env::var(key).ok();
        assert_eq!(
            before, after,
            "init_forced must not write {key} into the process environment"
        );
    }
}
