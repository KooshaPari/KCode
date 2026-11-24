//! Termination policy for a background command that has exceeded its timeout.
//!
//! On Windows the direct child of a shell command is `cmd.exe` (or a PowerShell host), so killing
//! only that process leaves the things a timeout is meant to stop running: `ping.exe`, `rg.exe`,
//! `git.exe`, compilers, test runners. They keep the pipes open and the file locks held, so the
//! caller sees a "timed out" task whose work carries on invisibly. The unix arm and the background
//! cancel path already terminate the whole process tree through
//! [`crate::platform::signal_detached_process_group`]; Windows now does the same.
//!
//! The policy lives here rather than inline in `tool/bash.rs` so the arm stays one line and so the
//! regression tests can sit next to the code they describe.

/// Kills a background command's process tree after its timeout fired.
///
/// On Windows this walks the tree with `taskkill /T` (see
/// [`crate::platform::signal_detached_process_group`]) and falls back to a direct kill when the
/// leader id is already gone. Everywhere else it does what it always did: kill the direct child.
pub(crate) fn kill_timed_out_command(child: &mut tokio::process::Child) {
    #[cfg(windows)]
    {
        let tree_killed = child
            .id()
            .is_some_and(|pid| crate::platform::signal_detached_process_group(pid, 0).is_ok());
        if tree_killed {
            return;
        }
    }
    let _ = child.start_kill();
}

// `bash_tests.rs` is gated `not(windows)` because most of its fixtures are unix shells, so the
// Windows coverage of this policy needs its own module.
#[cfg(all(test, windows))]
#[path = "background_command_kill_tests.rs"]
mod windows_tests;
