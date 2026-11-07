// Regression tests for issue #231: an external clipboard helper that is still
// running past the fail-fast window has not proven ownership, so the UI must
// never display the confirmed "Copied" wording for it. A late exit may correct
// the provisional notice, but only for the current copy and only while that
// provisional notice is still visible.

use crate::tui::app::helpers::{ClipboardCopy, ClipboardWatch, HelperExit};

fn unconfirmed() -> (ClipboardCopy, std::sync::mpsc::Sender<HelperExit>) {
    let (watch, tx) = ClipboardWatch::channel();
    (ClipboardCopy::unconfirmed(watch), tx)
}

#[test]
fn unconfirmed_never_displays_a_confirmed_copy() {
    let mut app = create_test_app();
    let (copy, _tx) = unconfirmed();
    let accepted = app.report_clipboard_copy(
        copy,
        "Copied selection",
        "Copy sent (unconfirmed)",
        "Failed to copy selection",
    );
    assert!(accepted, "unconfirmed must not trigger a fallback retry");
    assert_eq!(
        app.status_notice_text(),
        Some("Copy sent (unconfirmed)"),
        "an unconfirmed helper must never show the confirmed wording"
    );
}

#[test]
fn late_nonzero_exit_corrects_the_notice() {
    let mut app = create_test_app();
    let (copy, tx) = unconfirmed();
    app.report_clipboard_copy(
        copy,
        "Copied selection",
        "Copy sent (unconfirmed)",
        "Failed to copy selection",
    );
    assert!(!app.poll_clipboard_outcome(), "no exit has been observed");
    tx.send(HelperExit::ExitedNonZero).expect("send exit");
    assert!(app.poll_clipboard_outcome());
    assert_eq!(
        app.status_notice_text(),
        Some("Failed to copy selection"),
        "a late failure must replace the provisional notice"
    );
}

#[test]
fn late_zero_exit_promotes_the_notice() {
    let mut app = create_test_app();
    let (copy, tx) = unconfirmed();
    app.report_clipboard_copy(
        copy,
        "Copied selection",
        "Copy sent (unconfirmed)",
        "Failed to copy selection",
    );
    tx.send(HelperExit::ExitedZero).expect("send exit");
    assert!(app.poll_clipboard_outcome());
    assert_eq!(app.status_notice_text(), Some("Copied selection"));
}

#[test]
fn superseded_copy_cannot_deliver_a_late_outcome() {
    let mut app = create_test_app();
    let (copy_a, tx_a) = unconfirmed();
    app.report_clipboard_copy(copy_a, "Copied A", "Copy A (unconfirmed)", "Failed A");
    let (copy_b, _tx_b) = unconfirmed();
    app.report_clipboard_copy(copy_b, "Copied B", "Copy B (unconfirmed)", "Failed B");

    // Starting copy B replaces the pending copy A, which drops A's watch. A
    // therefore cannot deliver a late exit at all, so no code path exists by
    // which a superseded copy could overwrite the newer notice. This asserts
    // that seal rather than a token comparison that can never be reached.
    assert!(
        tx_a.send(HelperExit::ExitedNonZero).is_err(),
        "a superseded copy must not be able to deliver a late outcome"
    );
    assert!(!app.poll_clipboard_outcome(), "B has not resolved yet");
    assert_eq!(app.status_notice_text(), Some("Copy B (unconfirmed)"));
}

#[test]
fn late_outcome_does_not_replace_an_unrelated_notice() {
    let mut app = create_test_app();
    let (copy, tx) = unconfirmed();
    app.report_clipboard_copy(
        copy,
        "Copied selection",
        "Copy sent (unconfirmed)",
        "Failed to copy selection",
    );
    app.set_status_notice("Model switched");
    tx.send(HelperExit::ExitedNonZero).expect("send exit");
    assert!(!app.poll_clipboard_outcome());
    assert_eq!(app.status_notice_text(), Some("Model switched"));
}

#[test]
fn tick_poll_is_non_blocking_for_an_unresolved_watch() {
    let mut app = create_test_app();
    let (copy, _tx) = unconfirmed();
    app.report_clipboard_copy(
        copy,
        "Copied selection",
        "Copy sent (unconfirmed)",
        "Failed to copy selection",
    );
    let start = std::time::Instant::now();
    assert!(!app.poll_clipboard_outcome());
    assert!(!app.poll_clipboard_outcome());
    assert!(
        start.elapsed() < std::time::Duration::from_millis(50),
        "poll blocked for {:?}",
        start.elapsed()
    );
    assert_eq!(app.status_notice_text(), Some("Copy sent (unconfirmed)"));
}

#[test]
fn confirmed_and_failed_copies_use_existing_wording() {
    let mut app = create_test_app();
    assert!(app.report_clipboard_copy(
        ClipboardCopy::copied(),
        "Copied selection",
        "Copy sent (unconfirmed)",
        "Failed to copy selection",
    ));
    assert_eq!(app.status_notice_text(), Some("Copied selection"));

    assert!(!app.report_clipboard_copy(
        ClipboardCopy::failed(),
        "Copied selection",
        "Copy sent (unconfirmed)",
        "Failed to copy selection",
    ));
    assert_eq!(app.status_notice_text(), Some("Failed to copy selection"));
}

#[test]
fn disconnected_reaper_is_never_promoted_to_a_copy() {
    let mut app = create_test_app();
    let (copy, tx) = unconfirmed();
    app.report_clipboard_copy(
        copy,
        "Copied selection",
        "Copy sent (unconfirmed)",
        "Failed to copy selection",
    );
    // The reaper vanished without reporting an exit. Preserve the unconfirmed
    // notice and discard the dead watch rather than inventing failure.
    drop(tx);
    assert!(!app.poll_clipboard_outcome());
    assert!(app.clipboard.pending.is_none());
    assert_eq!(app.status_notice_text(), Some("Copy sent (unconfirmed)"));
    assert!(!app.poll_clipboard_outcome());
}

#[test]
fn failed_wait_preserves_unconfirmed_notice() {
    let mut app = create_test_app();
    let (copy, tx) = unconfirmed();
    app.report_clipboard_copy(copy, "Copied", "Unconfirmed", "Failed");
    tx.send(HelperExit::WaitFailed).expect("send wait failure");
    assert!(!app.poll_clipboard_outcome());
    assert!(app.clipboard.pending.is_none());
    assert_eq!(app.status_notice_text(), Some("Unconfirmed"));
    assert!(!app.poll_clipboard_outcome());
}

#[test]
fn unconfirmed_cut_keeps_the_input_and_never_claims_a_cut() {
    let mut app = create_test_app();
    app.input = "hello world".to_string();
    app.cursor_pos = 5;
    let (watch, tx) = ClipboardWatch::channel();
    // The copy seam is `FnMut`, so the non-`Copy` watch cannot be moved into
    // it. Hand it over through a slot that can be taken at most once.
    let slot = std::cell::RefCell::new(Some(watch));

    let cut = super::input::cut_input_line_to_clipboard_with(&mut app, |_text| {
        ClipboardCopy::unconfirmed(slot.borrow_mut().take().expect("one copy attempt"))
    });

    assert!(!cut, "an unconfirmed cut must not report success");
    assert_eq!(app.input(), "hello world", "input must be preserved");
    assert_eq!(app.cursor_pos(), 5);
    assert_eq!(
        app.status_notice_text(),
        Some("Cut copy sent (unconfirmed) · input kept so you can retry")
    );

    // A late confirming exit must not claim the input was cleared.
    tx.send(HelperExit::ExitedZero).expect("send exit");
    assert!(app.poll_clipboard_outcome());
    assert_eq!(app.input(), "hello world");
    assert_eq!(
        app.status_notice_text(),
        Some("Cut copy confirmed · input kept so you can clear it")
    );
}
