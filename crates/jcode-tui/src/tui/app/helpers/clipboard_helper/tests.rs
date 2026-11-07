use super::test_support::{ReleaseOnDrop, StubPath, bounded_sentinel_stub};
use super::{
    Duration, HelperExit, HelperOutcome, Instant, TEST_SETTLE_WINDOW,
    copy_via_clipboard_helper_within,
};

#[test]
fn release_guard_signals_during_unwind() {
    let dir = tempfile::tempdir().expect("temp dir");
    let sentinel = dir.path().join("release");
    let result = std::panic::catch_unwind(|| {
        let _guard = ReleaseOnDrop::new(dir);
        panic!("simulate failed assertion");
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"go");
}

#[test]
fn disconnected_reaper_is_not_an_exit_receipt() {
    let (watch, sender) = super::ClipboardWatch::channel();
    drop(sender);
    assert!(matches!(
        watch.try_observe_receipt(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));
}

#[test]
fn failed_wait_is_not_an_exit_receipt() {
    assert_eq!(
        super::classify_wait_result(Err(std::io::Error::other("wait failed"))),
        HelperExit::WaitFailed
    );
}

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
    let mut release = ReleaseOnDrop::new(tempfile::tempdir().expect("temp dir"));
    let sentinel = release.sentinel.clone();
    // The helper exits nonzero only once the test says so, so a descheduled
    // test thread cannot let it exit before the unconfirmed assertions run.
    let body = bounded_sentinel_stub(&sentinel, 1);
    let _stubs = StubPath::with(&[("late-fail", body.as_str())]);
    let start = Instant::now();
    let outcome = copy_via_clipboard_helper_within("late-fail", &[], "hello", LIVE_OWNER_WINDOW);
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "spawn path blocked for {:?}",
        start.elapsed()
    );
    let HelperOutcome::Unconfirmed(watch) = outcome else {
        panic!("a helper that outlived the window was reported as {outcome:?}");
    };
    assert_eq!(
        watch
            .try_observe_receipt()
            .expect("reaper receipt channel disconnected"),
        None,
        "unobserved helper must not settle"
    );
    std::fs::write(&sentinel, b"go").expect("release late-failing helper");
    let mut settled = None;
    for _ in 0..200 {
        if let Some(exit) = watch
            .try_observe_receipt()
            .expect("reaper receipt channel disconnected")
        {
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
    // The marker excludes the watchdog-only exit path. The production
    // watcher reports only zero/nonzero, so it cannot prove an exact code.
    assert_eq!(
        std::fs::read(sentinel.with_extension("observed")).unwrap(),
        b"1"
    );
    release.confirm_exit();
}

/// A genuinely resident helper (the real `wl-copy`/`xclip` shape) is
/// unconfirmed while it runs, never a copy, and must not block the caller.
/// Releasing it deterministically proves the reaper observed its exit.
#[test]
fn resident_helper_is_unconfirmed_and_reaped_without_blocking() {
    let _lock = crate::storage::lock_test_env();
    let mut release = ReleaseOnDrop::new(tempfile::tempdir().expect("temp dir"));
    let sentinel = release.sentinel.clone();
    let body = bounded_sentinel_stub(&sentinel, 0);
    let _stubs = StubPath::with(&[("resident-helper", body.as_str())]);

    let start = Instant::now();
    let outcome =
        copy_via_clipboard_helper_within("resident-helper", &[], "hello", LIVE_OWNER_WINDOW);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(1),
        "resident helper blocked the caller for {elapsed:?}"
    );
    let HelperOutcome::Unconfirmed(watch) = outcome else {
        panic!("a resident helper was reported as {outcome:?}");
    };
    assert_eq!(
        watch
            .try_observe_receipt()
            .expect("reaper receipt channel disconnected"),
        None,
        "a resident helper is never a confirmed copy"
    );

    std::fs::write(&sentinel, b"go").expect("release resident helper");
    let mut settled = None;
    for _ in 0..200 {
        if let Some(exit) = watch
            .try_observe_receipt()
            .expect("reaper receipt channel disconnected")
        {
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
    assert_eq!(
        std::fs::read(sentinel.with_extension("observed")).unwrap(),
        b"0"
    );
    release.confirm_exit();
}
