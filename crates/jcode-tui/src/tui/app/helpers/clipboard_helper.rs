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

#[cfg(not(any(windows, target_os = "macos")))]
impl HelperOutcome {
    pub(crate) fn accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    pub(crate) fn failed(&self) -> bool {
        matches!(self, Self::Failed)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn copy_via_clipboard_helper(
    program: &str,
    args: &[&str],
    text: &str,
) -> HelperOutcome {
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
                        let exit = match child.wait() {
                            Ok(status) if status.success() => HelperExit::ExitedZero,
                            _ => HelperExit::ExitedNonZero,
                        };
                        // Dropping the watch before the exit is observed is
                        // fine; the send fails and this thread has still
                        // reaped the child.
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

/// Test-only stub PATH harness shared by the helper and ordering tests.
#[cfg(all(test, not(any(windows, target_os = "macos"))))]
pub(super) mod test_support {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    /// Puts only the named stubs on PATH, so any helper not listed behaves as
    /// if it is not installed.
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
            crate::env::set_var("PATH", dir.path());
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
mod tests {
    use super::test_support::StubPath;
    use super::{
        Duration, HelperOutcome, HelperExit, Instant, TEST_SETTLE_WINDOW,
        copy_via_clipboard_helper_within,
    };

    /// Shorter than any real helper's lifetime, so "still running when the
    /// window closes" is reached deterministically instead of by luck.
    const LIVE_OWNER_WINDOW: Duration = Duration::from_millis(10);

    #[test]
    fn helper_that_exits_successfully_is_accepted() {
        assert!(matches!(
            copy_via_clipboard_helper_within("cat", &[], "hello", TEST_SETTLE_WINDOW),
            HelperOutcome::Accepted
        ));
    }

    /// Regression guard for the failing `Build & Test (desk-linux)` run: the
    /// nonzero exit must be observed, not guessed at from a wall-clock budget
    /// that a busy runner can miss.
    #[test]
    fn helper_that_exits_nonzero_falls_through() {
        assert!(matches!(
            copy_via_clipboard_helper_within("false", &[], "hello", TEST_SETTLE_WINDOW),
            HelperOutcome::Failed
        ));
    }

    /// A helper that starts slowly and only then fails must still report a
    /// fall-through, so the remaining fallbacks get their turn.
    #[test]
    fn helper_that_exits_nonzero_late_still_falls_through() {
        assert!(matches!(
            copy_via_clipboard_helper_within(
                "sh",
                &["-c", "sleep 1; exit 7"],
                "hello",
                TEST_SETTLE_WINDOW
            ),
            HelperOutcome::Failed
        ));
    }

    #[test]
    fn missing_helper_binary_falls_through() {
        assert!(matches!(
            copy_via_clipboard_helper_within(
                "kcode-nonexistent-clipboard-helper",
                &[],
                "hello",
                TEST_SETTLE_WINDOW
            ),
            HelperOutcome::Failed
        ));
    }

    /// The #231 defect: a helper that is still running at the deadline is not
    /// a copy. Its real (delayed) failure must arrive through the watch.
    #[test]
    fn delayed_nonzero_exit_is_unconfirmed_then_fails() {
        let _lock = crate::storage::lock_test_env();
        let _stubs = StubPath::with(&[("late-fail", "sleep 0.25; exit 1")]);
        let start = Instant::now();
        let outcome = copy_via_clipboard_helper_within(
            "late-fail",
            &[],
            "hello",
            LIVE_OWNER_WINDOW,
        );
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "spawn path blocked for {:?}",
            start.elapsed()
        );
        let HelperOutcome::Unconfirmed(watch) = outcome else {
            panic!("a helper that outlived the window was reported as {outcome:?}");
        };
        assert_eq!(watch.try_settle(), None, "unobserved helper must not settle");
        let mut settled = None;
        for _ in 0..200 {
            if let Some(exit) = watch.try_settle() {
                settled = Some(exit);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            settled,
            Some(HelperExit::ExitedNonZero),
            "the reaper must report the delayed nonzero exit"
        );
    }

    /// A genuinely resident helper (the real `wl-copy`/`xclip` shape) is
    /// unconfirmed while it runs, never a copy, and must not block the caller.
    /// Releasing it deterministically proves the reaper observed its exit.
    #[test]
    fn resident_helper_is_unconfirmed_and_reaped_without_blocking() {
        let _lock = crate::storage::lock_test_env();
        let dir = tempfile::tempdir().expect("temp dir");
        let sentinel = dir.path().join("release");
        let body = format!(
            "while [ ! -f '{sentinel}' ]; do sleep 0.02; done; exit 0",
            sentinel = sentinel.display()
        );
        let _stubs = StubPath::with(&[("resident-helper", body.as_str())]);

        let start = Instant::now();
        let outcome = copy_via_clipboard_helper_within(
            "resident-helper",
            &[],
            "hello",
            LIVE_OWNER_WINDOW,
        );
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(1),
            "resident helper blocked the caller for {elapsed:?}"
        );
        let HelperOutcome::Unconfirmed(watch) = outcome else {
            panic!("a resident helper was reported as {outcome:?}");
        };
        assert_eq!(
            watch.try_settle(),
            None,
            "a resident helper is never a confirmed copy"
        );

        std::fs::write(&sentinel, b"go").expect("release resident helper");
        let mut settled = None;
        for _ in 0..200 {
            if let Some(exit) = watch.try_settle() {
                settled = Some(exit);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            settled,
            Some(HelperExit::ExitedZero),
            "the reaper must report the resident helper's eventual exit"
        );
    }
}

/// Runtime ordering tests. `copy_to_clipboard` itself is short-circuited under
/// `cfg(test)` (it must never touch the developer's real clipboard), so these
/// exercise the same fallback chain it runs, using stub helpers on a temporary
/// PATH. That pins the property that actually matters: the first helper whose
/// write is accepted wins, and one that fails fast hands off to the next
/// instead of reporting a false success. An unconfirmed helper stops the chain
/// (it may own the selection; falling through could steal it) without claiming
/// a copy.
#[cfg(all(test, not(any(windows, target_os = "macos"))))]
mod ordering_tests {
    use super::test_support::StubPath;
    use super::{
        Duration, HelperOutcome, TEST_SETTLE_WINDOW, copy_via_clipboard_helper_within,
    };

    /// Same chain as the Linux arm of `copy_to_clipboard`, returning the first
    /// helper whose probe did not fail.
    fn first_helper_outcome() -> Option<(&'static str, HelperOutcome)> {
        for (program, args) in [
            ("wl-copy", &[][..]),
            ("xclip", &["-selection", "clipboard"][..]),
            ("xsel", &["--clipboard", "--input"][..]),
        ] {
            let outcome = copy_via_clipboard_helper_within(
                program,
                args,
                "payload",
                TEST_SETTLE_WINDOW,
            );
            if !outcome.failed() {
                return Some((program, outcome));
            }
        }
        None
    }

    fn first_helper_that_accepts() -> Option<&'static str> {
        first_helper_outcome()
            .filter(|(_, outcome)| outcome.accepted())
            .map(|(program, _)| program)
    }

    /// Regression guard: adding the X11 helpers must not steal the Wayland path
    /// that already worked.
    #[test]
    fn wayland_still_wins_when_wl_copy_works() {
        let _lock = crate::storage::lock_test_env();
        let _stubs = StubPath::with(&[("wl-copy", "cat > /dev/null; exit 0"), ("xclip", "exit 0")]);
        assert_eq!(first_helper_that_accepts(), Some("wl-copy"));
    }

    /// The actual #684 scenario: no Wayland display, so wl-copy fails fast and
    /// xclip must take over instead of falling through to arboard.
    #[test]
    fn xclip_takes_over_when_wl_copy_fails() {
        let _lock = crate::storage::lock_test_env();
        let _stubs = StubPath::with(&[
            ("wl-copy", "exit 1"),
            ("xclip", "cat > /dev/null; exit 0"),
            ("xsel", "exit 0"),
        ]);
        assert_eq!(first_helper_that_accepts(), Some("xclip"));
    }

    #[test]
    fn xsel_takes_over_when_wl_copy_and_xclip_are_missing() {
        let _lock = crate::storage::lock_test_env();
        let _stubs = StubPath::with(&[("xsel", "cat > /dev/null; exit 0")]);
        assert_eq!(first_helper_that_accepts(), Some("xsel"));
    }

    /// With no working helper the chain must report failure, so the real
    /// `copy_to_clipboard` continues to arboard and then OSC 52 rather than
    /// showing a false "Copied" toast.
    #[test]
    fn no_working_helper_reports_failure_so_later_fallbacks_run() {
        let _lock = crate::storage::lock_test_env();
        let _stubs = StubPath::with(&[
            ("wl-copy", "exit 1"),
            ("xclip", "exit 1"),
            ("xsel", "exit 1"),
        ]);
        assert_eq!(first_helper_outcome(), None);
    }

    /// #231: a resident helper must stop the chain without claiming a copy.
    /// Falling through to xclip/xsel would start a competing selection owner.
    #[test]
    fn unconfirmed_helper_stops_the_chain_without_a_copy_claim() {
        let _lock = crate::storage::lock_test_env();
        let dir = tempfile::tempdir().expect("temp dir");
        let sentinel = dir.path().join("release");
        let body = format!(
            "while [ ! -f '{sentinel}' ]; do sleep 0.02; done; exit 0",
            sentinel = sentinel.display()
        );
        let _stubs = StubPath::with(&[
            ("wl-copy", body.as_str()),
            ("xclip", "cat > /dev/null; exit 0"),
        ]);

        let (program, outcome) = first_helper_outcome().expect("wl-copy must be tried first");
        assert_eq!(program, "wl-copy");
        assert!(
            matches!(&outcome, HelperOutcome::Unconfirmed(_)),
            "resident helper must be unconfirmed, got {outcome:?}"
        );
        assert!(
            !outcome.accepted(),
            "an unconfirmed helper must never be reported as accepted"
        );

        std::fs::write(&sentinel, b"go").expect("release resident helper");
        // Give the reaper a moment to observe the exit so the temp dir and
        // child are cleaned up before the test returns.
        std::thread::sleep(Duration::from_millis(100));
    }
}
