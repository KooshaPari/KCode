use super::test_support::{ReleaseOnDrop, StubPath, bounded_sentinel_stub};
use super::{
    Duration, HelperExit, HelperOutcome, TEST_SETTLE_WINDOW, copy_via_clipboard_helper_within,
};

/// Short enough that a never-exiting helper is reported unconfirmed quickly,
/// so the resident-helper case does not wait out the settle window.
const LIVE_OWNER_WINDOW: Duration = Duration::from_millis(10);

/// Same chain as the Linux arm of `copy_to_clipboard`, returning the first
/// helper whose probe did not fail. Tests that expect a resident (never
/// exiting) helper pass the short window so the suite does not wait out the
/// generous settle window.
fn first_helper_outcome() -> Option<(&'static str, HelperOutcome)> {
    first_helper_outcome_within(TEST_SETTLE_WINDOW)
}

fn first_helper_outcome_within(window: Duration) -> Option<(&'static str, HelperOutcome)> {
    for (program, args) in [
        ("wl-copy", &[][..]),
        ("xclip", &["-selection", "clipboard"][..]),
        ("xsel", &["--clipboard", "--input"][..]),
    ] {
        let outcome = copy_via_clipboard_helper_within(program, args, "payload", window);
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
fn xsel_takes_over_when_wl_copy_and_xclip_fail() {
    let _lock = crate::storage::lock_test_env();
    let _stubs = StubPath::with(&[
        ("wl-copy", "exit 1"),
        ("xclip", "exit 1"),
        ("xsel", "cat > /dev/null; exit 0"),
    ]);
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
    // `HelperOutcome` carries a `ClipboardWatch`, so it has no `PartialEq`.
    assert!(
        first_helper_outcome().is_none(),
        "a chain of failing helpers must report no outcome"
    );
}

/// #231: a resident helper must stop the chain without claiming a copy.
/// Falling through to xclip/xsel would start a competing selection owner.
#[test]
fn unconfirmed_helper_stops_the_chain_without_a_copy_claim() {
    let _lock = crate::storage::lock_test_env();
    let mut release = ReleaseOnDrop::new(tempfile::tempdir().expect("temp dir"));
    let sentinel = release.sentinel.clone();
    let body = bounded_sentinel_stub(&sentinel, 0);
    let _stubs = StubPath::with(&[
        ("wl-copy", body.as_str()),
        ("xclip", "cat > /dev/null; exit 0"),
    ]);

    let (program, outcome) =
        first_helper_outcome_within(LIVE_OWNER_WINDOW).expect("wl-copy must be tried first");
    assert_eq!(program, "wl-copy");
    assert!(
        matches!(&outcome, HelperOutcome::Unconfirmed(_)),
        "resident helper must be unconfirmed, got {outcome:?}"
    );
    assert!(
        !outcome.accepted(),
        "an unconfirmed helper must never be reported as accepted"
    );

    let HelperOutcome::Unconfirmed(watch) = outcome else {
        unreachable!()
    };
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
        "reaper must observe helper exit"
    );
    assert_eq!(
        std::fs::read(sentinel.with_extension("observed")).unwrap(),
        b"0"
    );
    release.confirm_exit();
}
