//! Spawning external clipboard helpers (`wl-copy`, `xclip`, `xsel`).
//!
//! Kept out of `helpers.rs` so the clipboard contract has one home and its
//! tests live next to it.
//!
//! What a spawned helper can actually tell us is its exit status. `wl-copy`,
//! `xclip`, and `xsel` fork a resident process that keeps serving paste
//! requests, so on a healthy desktop the success path usually produces no exit
//! status at all while the selection is owned. Therefore:
//!
//! - observed `exit 0` means the write was accepted. It is not a fresh
//!   observation of selection ownership.
//! - observed nonzero exit is a failure.
//! - still running at the deadline means **nothing is known**. The helper may
//!   become the owner, or it may be about to fail and simply has not been
//!   scheduled yet.
//!
//! The last state is reported as [`HelperOutcome::Unconfirmed`] and never as a
//! copy (issue #231). A background reaper owns the child, waits for it, and
//! reports the eventual exit through a [`ClipboardWatch`] so the UI can correct
//! itself on a later tick without ever blocking.

#[cfg(not(any(windows, target_os = "macos")))]
use super::{ClipboardWatch, HelperExit};
#[cfg(not(any(windows, target_os = "macos")))]
use std::time::{Duration, Instant};

/// Poll window used only to catch a helper that fails fast (e.g. no display
/// server) so the remaining fallbacks still run. It is not a deadline for
/// declaring ownership.
#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) const HELPER_FAIL_FAST_WINDOW: Duration = Duration::from_millis(150);

/// What the fail-fast probe observed.
#[cfg(not(any(windows, target_os = "macos")))]
#[derive(Debug)]
pub(crate) enum HelperOutcome {
    /// The helper spawned, accepted stdin, and exited zero.
    Accepted,
    /// Spawn error, stdin write error, or an observed nonzero exit.
    Failed,
    /// Still running when the window closed. Ownership is unproven; the
    /// returned watch reports the eventual exit (or stays silent for a
    /// genuinely resident helper).
    Unconfirmed(ClipboardWatch),
}

#[cfg(all(test, not(any(windows, target_os = "macos"))))]
impl HelperOutcome {
    // The production chain matches variants directly; these predicates exist so
    // the ordering tests read as intent rather than as pattern matches. Keeping
    // them test-only avoids a dead-code warning in the plain lib build.
    pub(crate) fn accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    pub(crate) fn failed(&self) -> bool {
        matches!(self, Self::Failed)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn copy_via_clipboard_helper(program: &str, args: &[&str], text: &str) -> HelperOutcome {
    copy_via_clipboard_helper_within(program, args, text, HELPER_FAIL_FAST_WINDOW)
}

/// Same contract as [`copy_via_clipboard_helper`], with the fail-fast poll
/// window supplied by the caller so tests are deterministic.
#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn copy_via_clipboard_helper_within(
    program: &str,
    args: &[&str],
    text: &str,
    window: Duration,
) -> HelperOutcome {
    use std::io::Write;

    let Ok(mut child) = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return HelperOutcome::Failed;
    };

    let wrote = match child.stdin.as_mut() {
        Some(stdin) => stdin.write_all(text.as_bytes()).is_ok(),
        None => false,
    };
    drop(child.stdin.take());
    if !wrote {
        let _ = child.kill();
        let _ = child.wait();
        return HelperOutcome::Failed;
    }

    let deadline = Instant::now() + window;
    loop {
        match child.try_wait() {
            // Exited zero: the write was accepted.
            Ok(Some(status)) if status.success() => return HelperOutcome::Accepted,
            // Exited nonzero (e.g. xclip with no DISPLAY): let the next
            // fallback try.
            Ok(Some(_)) => return HelperOutcome::Failed,
            Ok(None) => {
                if Instant::now() >= deadline {
                    // Unproven. The reaper owns the child from here, waits for
                    // it, and publishes the real exit status later.
                    let (watch, tx) = ClipboardWatch::channel();
                    std::thread::spawn(move || {
                        let exit = classify_wait_result(child.wait());
                        // A dropped watch discards the report. A failed wait
                        // leaves child termination and reaping unconfirmed.
                        let _ = tx.send(exit);
                    });
                    return HelperOutcome::Unconfirmed(watch);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return HelperOutcome::Failed,
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn classify_wait_result(result: std::io::Result<std::process::ExitStatus>) -> HelperExit {
    match result {
        Ok(status) if status.success() => HelperExit::ExitedZero,
        Ok(_) => HelperExit::ExitedNonZero,
        Err(_) => HelperExit::WaitFailed,
    }
}

/// Test-only stub PATH harness shared by the helper and ordering tests.
#[cfg(all(test, not(any(windows, target_os = "macos"))))]
pub(super) mod test_support {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    pub(super) struct ReleaseOnDrop {
        dir: Option<tempfile::TempDir>,
        pub(super) sentinel: std::path::PathBuf,
        settled: bool,
    }

    impl ReleaseOnDrop {
        pub(super) fn new(dir: tempfile::TempDir) -> Self {
            let sentinel = dir.path().join("release");
            Self {
                dir: Some(dir),
                sentinel,
                settled: false,
            }
        }

        pub(super) fn confirm_exit(&mut self) {
            self.settled = true;
        }
    }

    impl Drop for ReleaseOnDrop {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.sentinel, b"go");
            if !self.settled
                && let Some(dir) = self.dir.take()
            {
                // Keep the sentinel available for a helper that has not exited.
                let _ = dir.keep();
            }
        }
    }

    pub(super) fn bounded_sentinel_stub(path: &std::path::Path, exit: u8) -> String {
        let quoted = path.to_string_lossy().replace('\'', "'\\''");
        let marker = path.with_extension("observed");
        let marker = marker.to_string_lossy().replace('\'', "'\\''");
        format!(
            "i=0; while [ ! -f '{quoted}' ] && [ \"$i\" -lt 500 ]; do sleep 0.02; i=$((i+1)); done; [ -f '{quoted}' ] || exit 124; printf '%s' '{exit}' > '{marker}' || exit 125; exit {exit}"
        )
    }

    /// Puts the named stubs first on PATH. A listed helper always resolves to
    /// its stub; a helper that is not listed falls through to the real PATH, so
    /// tests that depend on a helper being absent must stub it as failing
    /// explicitly instead of relying on the host not having it installed.
    pub(super) struct StubPath {
        _dir: tempfile::TempDir,
        prev: Option<std::ffi::OsString>,
    }

    impl StubPath {
        pub(super) fn with(stubs: &[(&str, &str)]) -> Self {
            let dir = tempfile::tempdir().expect("temp dir");
            for (name, body) in stubs {
                let path = dir.path().join(name);
                let mut file = std::fs::File::create(&path).expect("create stub");
                writeln!(file, "#!/bin/sh").expect("write stub");
                writeln!(file, "{body}").expect("write stub");
                drop(file);
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                    .expect("chmod stub");
            }
            let prev = std::env::var_os("PATH");
            // Stub dir first so the named helpers win, while the real PATH stays
            // behind it: stub bodies legitimately run ordinary coreutils
            // (`sleep`, `cat`) and need them resolvable.
            let mut joined = dir.path().as_os_str().to_os_string();
            if let Some(prev) = prev.as_ref().filter(|p| !p.is_empty()) {
                joined.push(":");
                joined.push(prev);
            }
            crate::env::set_var("PATH", joined);
            Self { _dir: dir, prev }
        }
    }

    impl Drop for StubPath {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(prev) => crate::env::set_var("PATH", prev),
                None => crate::env::remove_var("PATH"),
            }
        }
    }
}

/// Tests for the external clipboard-helper spawn path. They use ordinary
/// coreutils instead of real clipboard tools so they pass on headless CI: what
/// matters is the contract (writes stdin, never blocks on a long-lived owner,
/// and never reports an unobserved helper as a copy).
///
/// Every spawn decision below passes its poll window explicitly. Asserting
/// against the production window raced the scheduler: on a loaded runner a
/// helper that exits nonzero can take longer than [`HELPER_FAIL_FAST_WINDOW`]
/// to be reaped, and the old "still alive means it owns the selection" branch
/// then reported a copy. Tests separate the two concerns: the settle window is
/// generous enough that an exit is always observed, and the live-owner window
/// is short enough that a `sleep` cannot possibly have exited inside it.
#[cfg(all(test, not(any(windows, target_os = "macos"))))]
pub(super) const TEST_SETTLE_WINDOW: std::time::Duration = std::time::Duration::from_secs(30);

#[cfg(all(test, not(any(windows, target_os = "macos"))))]
mod tests;

/// Runtime ordering tests. `copy_to_clipboard` itself is short-circuited under
/// `cfg(test)` (it must never touch the developer's real clipboard), so these
/// exercise the same fallback chain it runs, using stub helpers on a temporary
/// PATH. That pins the property that actually matters: the first helper whose
/// write is accepted wins, and one that fails fast hands off to the next
/// instead of reporting a false success. An unconfirmed helper stops the chain
/// (it may own the selection; falling through could steal it) without claiming
/// a copy.
#[cfg(all(test, not(any(windows, target_os = "macos"))))]
mod ordering_tests;
