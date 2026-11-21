//! Terminal-mode ownership: the Kitty keyboard push/pop stack, editor
//! handoff ownership, and the termination gate that keeps a dying process
//! from re-arming modes the following shell would inherit.
//!
//! Split out of `tui/mod.rs` and `tui/app/commands.rs` to keep those
//! oversized files flat; the code is a pure relocation.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::tui::keyboard_protocol;

pub(crate) fn keyboard_enhancement_flags() -> crossterm::event::KeyboardEnhancementFlags {
    use crossterm::event::KeyboardEnhancementFlags;

    if std::env::var_os("KCODE_DISABLE_KITTY_KEYBOARD").is_some() {
        return KeyboardEnhancementFlags::empty();
    }
    // Only disambiguate escape codes. REPORT_EVENT_TYPES makes terminals emit
    // press AND release events; hosts that half-support the kitty protocol
    // (herdr/conpty panes) leak releases into the prompt as "[<code>;1:3u".
    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
}

/// Whether this process still owns an unbalanced entry on the terminal's
/// Kitty keyboard-protocol stack.
///
/// The stack is shared with every program on the terminal: an editor spawned
/// via `suspend_terminal_for_editor` may push its own entry after jcode popped
/// ours. A later cleanup (signal handler, guard drop, picker exit) must not pop
/// again, or it would remove the editor's entry instead of ours.
static KITTY_KEYBOARD_PUSH_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Whether an external editor currently owns the terminal.
///
/// During editor handoff jcode has already restored cooked input, left the
/// alternate screen, and disabled its reporting modes. If jcode receives a
/// process-only termination signal while the editor remains alive, cleanup must
/// not write mode disables again because the editor may have enabled those modes
/// for itself.
static EDITOR_TERMINAL_OWNERSHIP: AtomicBool = AtomicBool::new(false);

/// Whether a termination signal has already begun the process's final cleanup.
///
/// Once termination started, the process is exiting and the shell behind it
/// must inherit a fully restored terminal. Re-arming any mode afterwards (an
/// editor reclaim/resume that lost the race against the signal cleanup) would
/// leave modes enabled after death because no further cleanup runs (greptile
/// #4232838288). Recorded under the kitty state lock so it cannot interleave
/// with the reclaim's ownership swap.
static TERMINATION_STARTED: AtomicBool = AtomicBool::new(false);

/// Serializes the terminal writes and the ownership flag transitions of the
/// Kitty push/pop pair. A termination signal (tokio task, not a UNIX signal
/// handler, so taking a lock is allowed) runs its terminal cleanup on the
/// event loop; this lock makes that cleanup wait for an in-flight push or pop
/// to finish instead of observing a half-applied state and skipping the pop.
static KITTY_KEYBOARD_STATE_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) static TERMINAL_OWNERSHIP_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Request Kitty keyboard reporting and tmux's extended-key mode.
///
/// Avoids REPORT_ALL_KEYS_AS_ESCAPE_CODES: with it, kitty/Alacritty/Warp report
/// printable keys as base key plus modifiers instead of the text the active
/// layout produces, and crossterm cannot reconstruct shifted symbols for every
/// layout. Prefer the terminal-delivered character and only synthesize ASCII
/// letter casing in the input fallback.
///
/// On Windows, keep native console key records: Crossterm has no CSI-u decoder
/// there. Focus/resume must follow the same policy even if the config enables it.
///
/// Returns whether the requests were written, not whether the terminal supports them.
pub fn enable_keyboard_enhancement() -> bool {
    let _guard = kitty_keyboard_state_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if termination_started() {
        crate::logging::info("Keyboard enhancement request skipped: termination already started");
        return false;
    }
    let result = enable_keyboard_enhancement_to(&mut std::io::stdout(), inside_tmux()).is_ok();
    crate::logging::info(&format!(
        "Keyboard enhancement request: {}",
        if result { "sent" } else { "FAILED" }
    ));
    if result {
        // Set while still holding the state lock so a signal cleanup cannot
        // observe the push write without the ownership flag.
        kitty_keyboard_push_active().store(true, Ordering::Release);
    }
    result
}

/// Adopt a Kitty keyboard entry the previous process left on the stack when
/// it exec'd into this one (restart, reload, rebuild, update).
///
/// The exec handoff deliberately keeps the terminal in raw mode with all
/// modes armed instead of popping and re-pushing, so this process must record
/// that the inherited entry is ours to pop on exit. Otherwise every cleanup
/// gate skips the pop and the following shell is left in CSI u mode.
pub fn adopt_keyboard_enhancement_ownership() {
    let _guard = kitty_keyboard_state_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if termination_started() {
        return;
    }
    kitty_keyboard_push_active().store(true, Ordering::Release);
}

fn inside_tmux() -> bool {
    std::env::var_os("TMUX").is_some_and(|value| !value.is_empty())
}

fn enable_keyboard_enhancement_to(
    writer: &mut impl std::io::Write,
    inside_tmux: bool,
) -> std::io::Result<()> {
    keyboard_protocol::write_mode(
        writer,
        keyboard_protocol::Action::Push,
        inside_tmux,
        keyboard_enhancement_flags().bits(),
    )
}

/// Reset extended keyboard reporting only for a decoder that enabled it.
///
/// The pop is skipped when this process has no unbalanced push on the
/// terminal's stack (a suspended editor or the calling shell may own the top
/// entry), so an extra pop cannot remove someone else's keyboard settings.
pub fn disable_keyboard_enhancement() {
    if !disable_keyboard_enhancement_if_owned() {
        crate::logging::info("Keyboard enhancement pop skipped: no active push owned");
    }
}

/// Pop the Kitty keyboard entry this process pushed, if it is still unbalanced.
///
/// Returns whether a pop was performed. The state lock orders this against
/// a concurrent push or signal cleanup, so the flag swap and the pop write
/// happen as one unit: termination waits for the pop instead of exiting with
/// the entry still active.
pub fn disable_keyboard_enhancement_if_owned() -> bool {
    disable_keyboard_enhancement_if_owned_to(&mut std::io::stdout(), inside_tmux())
}

/// Writer-backed form of [`disable_keyboard_enhancement_if_owned`] so tests
/// can verify the pop bytes without writing to the developer's terminal or
/// resetting tmux extended keys.
fn disable_keyboard_enhancement_if_owned_to(
    writer: &mut impl std::io::Write,
    inside_tmux: bool,
) -> bool {
    let _guard = kitty_keyboard_state_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if !kitty_keyboard_push_active().swap(false, std::sync::atomic::Ordering::AcqRel) {
        return false;
    }
    let _ = disable_keyboard_enhancement_to(writer, inside_tmux);
    true
}

/// Test hook: reset the ownership flag without touching the terminal.
#[cfg(test)]
fn reset_keyboard_enhancement_ownership_for_test() {
    kitty_keyboard_push_active().store(false, Ordering::Release);
}

fn kitty_keyboard_push_active() -> &'static AtomicBool {
    &KITTY_KEYBOARD_PUSH_ACTIVE
}

fn kitty_keyboard_state_lock() -> &'static Mutex<()> {
    &KITTY_KEYBOARD_STATE_LOCK
}

pub fn editor_owns_terminal() -> bool {
    EDITOR_TERMINAL_OWNERSHIP.load(Ordering::Acquire)
}

pub fn set_editor_terminal_ownership(owns_terminal: bool) {
    EDITOR_TERMINAL_OWNERSHIP.store(owns_terminal, Ordering::Release);
}

/// Publish that an external editor owns the terminal, atomically with the
/// signal cleanup's ownership check (both take the kitty state lock).
///
/// Callers must only publish AFTER jcode's own terminal restores finished
/// (`suspend_terminal_for_editor` completed): while the restores are in flight
/// the flag must stay false so a termination signal in that window performs a
/// full cleanup instead of skipping one and leaving the shell in raw mode
/// (greptile #4230626960).
pub fn publish_editor_terminal_handoff() {
    with_kitty_state_lock(|| {
        set_editor_terminal_ownership(true);
    });
}

/// Reclaim the terminal after the editor exited, atomically with the signal
/// cleanup's ownership check. Called before `resume_terminal_after_editor`
/// re-arms jcode's own modes: a signal arriving after the editor exited must
/// run the full cleanup against whatever state the resume loop has armed so
/// far, not skip it.
pub fn reclaim_editor_terminal_handoff() {
    with_kitty_state_lock(|| {
        // The signal cleanup released the lock but the process has not exited
        // yet: this reclaim runs in that gap, and letting the subsequent resume
        // re-arm modes would leave them enabled after death because no further
        // cleanup runs (greptile #4232838288). Leave ownership set so the
        // resume loop sees the terminating state and writes nothing.
        if termination_started() {
            return;
        }
        set_editor_terminal_ownership(false);
    });
}

/// Run `cleanup` while holding the kitty state lock, so an in-flight editor
/// handoff publication or reclaim (which takes the same lock) is ordered
/// against it. A signal arriving mid-handoff then sees a consistent state:
/// either jcode still owns the terminal (restore everything) or the editor
/// does (skip).
pub fn with_kitty_state_lock<R>(cleanup: impl FnOnce() -> R) -> R {
    let lock = kitty_keyboard_state_lock();
    let _state = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    cleanup()
}

/// Record that termination has begun, under the kitty state lock.
///
/// Called by the termination-signal cleanup as soon as it decides what to do
/// with the terminal. Recording under the same lock as the editor handoff's
/// publish/reclaim closes the gap between this cleanup releasing the lock and
/// the process actually exiting: a reclaim racing into that gap observes the
/// flag and leaves the terminal modes alone, so the shell receives normal
/// input after jcode dies instead of escape sequences (greptile #4232838288).
pub fn record_termination_started() {
    with_kitty_state_lock(|| {
        TERMINATION_STARTED.store(true, Ordering::Release);
    });
}

fn termination_started() -> bool {
    TERMINATION_STARTED.load(Ordering::Acquire)
}

/// Test hook: clear the shared termination flag without touching the terminal.
#[cfg(test)]
pub(crate) fn reset_termination_started_for_test() {
    TERMINATION_STARTED.store(false, Ordering::Release);
}

fn disable_keyboard_enhancement_to(
    writer: &mut impl std::io::Write,
    inside_tmux: bool,
) -> std::io::Result<()> {
    keyboard_protocol::write_mode(
        writer,
        keyboard_protocol::Action::Pop,
        inside_tmux,
        keyboard_enhancement_flags().bits(),
    )
}

fn reapply_keyboard_enhancement_to(
    writer: &mut impl std::io::Write,
    inside_tmux: bool,
) -> std::io::Result<()> {
    keyboard_protocol::write_mode(
        writer,
        keyboard_protocol::Action::Set,
        inside_tmux,
        keyboard_enhancement_flags().bits(),
    )
}

/// Reassert terminal modes that terminals may clear while the TUI remains alive.
///
/// Kitty keyboard enhancement uses its `set` form rather than the stack-based
/// `push`, keeping the shutdown pop balanced. Enabling focus reporting may itself
/// produce a focus event, so focus-event handlers must pass `focus_change = false`.
pub(crate) fn reapply_terminal_modes_to(
    writer: &mut impl std::io::Write,
    mouse_capture: bool,
    keyboard_enhanced: bool,
    focus_change: bool,
) -> std::io::Result<()> {
    use crossterm::QueueableCommand;
    use crossterm::event::{EnableBracketedPaste, EnableFocusChange, EnableMouseCapture};

    writer.queue(EnableBracketedPaste)?;
    if focus_change {
        writer.queue(EnableFocusChange)?;
    }
    if mouse_capture {
        writer.queue(EnableMouseCapture)?;
        // Crossterm toggles Win32 console mouse input on Windows, but ConPTY
        // hosts such as VS Code also need the VT tracking modes reasserted.
        #[cfg(windows)]
        writer.write_all(b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1015h\x1b[?1006h")?;
    }
    if keyboard_enhanced {
        reapply_keyboard_enhancement_to(writer, inside_tmux())?;
    }
    writer.flush()
}

pub(crate) fn reapply_configured_terminal_modes_after_focus() {
    let policy = crate::perf::tui_policy();
    if let Err(error) = reapply_terminal_modes_after_focus_to(
        &mut std::io::stdout(),
        policy.enable_mouse_capture,
        policy.enable_keyboard_enhancement,
    ) {
        crate::logging::warn(&format!("failed to reapply terminal modes: {error}"));
    }
}

fn reapply_terminal_modes_after_focus_to(
    writer: &mut impl std::io::Write,
    mouse_capture: bool,
    keyboard_enhanced: bool,
) -> std::io::Result<()> {
    reapply_terminal_modes_to(
        writer,
        mouse_capture,
        keyboard_enhanced,
        // Ghostty reports its current focus when mode 1004 is enabled. Re-arming
        // it from FocusGained would feed that reply back into this handler forever.
        // Startup and resume-after-editor still enable focus reporting normally.
        false,
    )
}

#[cfg(test)]
mod terminal_mode_tests {
    #[cfg(not(windows))]
    use super::{
        disable_keyboard_enhancement_to, enable_keyboard_enhancement_to,
        kitty_keyboard_push_active, reapply_keyboard_enhancement_to,
        reapply_terminal_modes_after_focus_to, reapply_terminal_modes_to,
        reset_keyboard_enhancement_ownership_for_test,
    };

    #[test]
    #[cfg(not(windows))]
    fn disable_skips_kitty_pop_when_no_push_is_owned() {
        let _guard = super::TERMINAL_OWNERSHIP_TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        reset_keyboard_enhancement_ownership_for_test();

        // Greptile #4228191597: suspend_terminal_for_editor already popped our
        // entry while the editor runs, so an extra pop (signal handler firing in
        // that window) must be a no-op instead of removing the editor's own
        // stack entry.
        assert!(!super::disable_keyboard_enhancement_if_owned());
        assert!(!kitty_keyboard_push_active().load(std::sync::atomic::Ordering::Acquire));

        reset_keyboard_enhancement_ownership_for_test();
    }

    #[test]
    fn disable_pops_kitty_entry_exactly_once_then_ownership_is_released() {
        let _guard = super::TERMINAL_OWNERSHIP_TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        reset_keyboard_enhancement_ownership_for_test();

        // Greptile #4228894633: run the pop against a buffer so the test never
        // writes the pop (or the tmux extended-keys reset) to the developer's
        // real terminal.
        kitty_keyboard_push_active().store(true, std::sync::atomic::Ordering::Release);
        let mut output = Vec::new();
        assert!(super::disable_keyboard_enhancement_if_owned_to(
            &mut output,
            false
        ));
        assert!(!output.is_empty());
        assert!(!kitty_keyboard_push_active().load(std::sync::atomic::Ordering::Acquire));

        // A second cleanup (SIGTERM during the editor window, or a duplicated
        // teardown) must not pop again now that the ownership is released.
        let mut second = Vec::new();
        assert!(!super::disable_keyboard_enhancement_if_owned_to(
            &mut second,
            false
        ));
        assert!(second.is_empty());

        reset_keyboard_enhancement_ownership_for_test();
    }

    #[test]
    fn adopt_records_inherited_entry_without_pushing() {
        let _guard = super::TERMINAL_OWNERSHIP_TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        reset_keyboard_enhancement_ownership_for_test();

        // Greptile #4228894613: after an exec handoff the entry is already on
        // the stack, so adopting must only record ownership — no push bytes —
        // and the pop must then balance the inherited entry.
        super::adopt_keyboard_enhancement_ownership();
        assert!(kitty_keyboard_push_active().load(std::sync::atomic::Ordering::Acquire));

        let mut output = Vec::new();
        assert!(super::disable_keyboard_enhancement_if_owned_to(
            &mut output,
            false
        ));
        assert_eq!(output, b"\x1b[<1u");

        reset_keyboard_enhancement_ownership_for_test();
    }

    #[test]
    fn cleanup_waits_for_in_flight_kitty_push_to_finish() {
        let _guard = super::TERMINAL_OWNERSHIP_TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        reset_keyboard_enhancement_ownership_for_test();

        // Greptile #4228894626: a termination cleanup must not observe the
        // half-applied state between the push write and the ownership flag,
        // so the disable waits for the state lock instead of racing it. Hold
        // the lock, set the flag as if a push just completed, and confirm a
        // concurrent cleanup stays blocked until the lock is released.
        let state_guard = super::kitty_keyboard_state_lock()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        kitty_keyboard_push_active().store(true, std::sync::atomic::Ordering::Release);

        let cleanup = std::thread::spawn(|| {
            // Buffer-backed so the blocked cleanup writes no bytes to the
            // developer's terminal; the flag swap is the observable effect.
            let mut sink = Vec::new();
            super::disable_keyboard_enhancement_if_owned_to(&mut sink, false)
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
        // The cleanup is still blocked on the state lock we hold, so the flag
        // must be untouched and the thread must not have returned yet.
        assert!(!cleanup.is_finished());
        assert!(kitty_keyboard_push_active().load(std::sync::atomic::Ordering::Acquire));

        drop(state_guard);
        assert!(cleanup.join().expect("cleanup thread"));
        assert!(!kitty_keyboard_push_active().load(std::sync::atomic::Ordering::Acquire));

        reset_keyboard_enhancement_ownership_for_test();
    }

    #[test]
    #[cfg(not(windows))]
    fn tmux_keyboard_lifecycle_requests_and_resets_extended_keys() {
        let mut output = Vec::new();
        enable_keyboard_enhancement_to(&mut output, true).unwrap();
        assert_eq!(output, b"\x1b[>4;2m\x1b[>1u");

        output.clear();
        reapply_keyboard_enhancement_to(&mut output, true).unwrap();
        assert_eq!(output, b"\x1b[>4;2m\x1b[=1u");

        output.clear();
        disable_keyboard_enhancement_to(&mut output, true).unwrap();
        assert_eq!(output, b"\x1b[>4;0m\x1b[<1u");
    }

    #[test]
    #[cfg(not(windows))]
    fn outside_tmux_keyboard_lifecycle_keeps_kitty_protocol_only() {
        let mut output = Vec::new();
        enable_keyboard_enhancement_to(&mut output, false).unwrap();
        assert_eq!(output, b"\x1b[>1u");

        output.clear();
        reapply_keyboard_enhancement_to(&mut output, false).unwrap();
        assert_eq!(output, b"\x1b[=1u");

        output.clear();
        disable_keyboard_enhancement_to(&mut output, false).unwrap();
        assert_eq!(output, b"\x1b[<1u");
    }

    #[test]
    fn reapply_omits_keyboard_protocols_when_disabled() {
        let mut output = Vec::new();
        reapply_terminal_modes_to(&mut output, false, false, false).unwrap();
        assert_eq!(output, b"\x1b[?2004h");
    }

    #[test]
    fn reapply_omits_mouse_sequences_when_capture_is_disabled() {
        let mut output = Vec::new();
        reapply_terminal_modes_to(&mut output, false, true, true).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.starts_with("\x1b[?2004h\x1b[?1004h"));
        assert!(!output.contains("\x1b[?1000h"));
        assert_eq!(output.contains("\x1b[="), !cfg!(windows));
    }

    #[test]
    fn reapply_emits_configured_idempotent_modes_without_keyboard_push() {
        let mut output = Vec::new();
        reapply_terminal_modes_to(&mut output, true, true, true).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("\x1b[?2004h"));
        assert!(output.contains("\x1b[?1004h"));
        assert!(output.contains("\x1b[?1000h"));
        assert_eq!(output.contains("\x1b[="), !cfg!(windows));
        assert!(
            !output.contains("\x1b[>7u"),
            "must not push the Kitty keyboard stack"
        );
    }

    #[test]
    fn focus_reapply_preserves_other_modes_without_rearming_focus_reporting() {
        for mouse_capture in [false, true] {
            for keyboard_enhanced in [false, true] {
                let mut output = Vec::new();
                reapply_terminal_modes_after_focus_to(
                    &mut output,
                    mouse_capture,
                    keyboard_enhanced,
                )
                .unwrap();

                let output = String::from_utf8(output).unwrap();
                assert!(output.starts_with("\x1b[?2004h"));
                assert!(
                    !output.contains("\x1b[?1004h"),
                    "must not trigger a focus reply"
                );
                assert!(
                    !output.contains("\x1b[?1004l"),
                    "must keep focus reporting enabled"
                );
                assert_eq!(output.contains("\x1b[?1000h"), mouse_capture);
                assert_eq!(
                    output.contains("\x1b[=1u"),
                    keyboard_enhanced && !cfg!(windows)
                );
                assert!(
                    !output.contains("\x1b[>7u"),
                    "must not push the Kitty keyboard stack (tmux modifyOtherKeys is allowed)"
                );
            }
        }
    }
}

pub(crate) fn run_interactive_editor_with(
    command: &mut std::process::Command,
    suspend: impl FnOnce(),
    resume: impl FnOnce(),
) -> std::io::Result<std::process::ExitStatus> {
    suspend();
    // Publish only after the restores completed. The flag must never be
    // observable while jcode's own modes are still armed: a termination
    // signal firing mid-suspend would skip its cleanup and leave the shell in
    // raw mode (greptile #4230626960). While the disable writes are in flight
    // the flag is still false, so a signal in that window restores everything;
    // the flag is then published under the same kitty state lock the signal
    // cleanup takes, so cleanup either sees the completed handoff or runs in
    // full.
    crate::tui::publish_editor_terminal_handoff();
    let result = command.status();
    // Reclaim before resume re-arms our modes: while the flag is still set a
    // termination signal skips its cleanup, which must only happen while the
    // editor really is the one controlling the tty.
    crate::tui::reclaim_editor_terminal_handoff();
    // A termination signal that skipped its cleanup while the editor owned
    // the terminal recorded termination under the kitty state lock before
    // this reclaim ran, and the process is exiting. The reclaim leaves
    // ownership set in that case; re-arming modes now would leave them
    // enabled on the shell after death because no further cleanup runs
    // (greptile #4232838288), so skip the resume entirely.
    if crate::tui::editor_owns_terminal() {
        return result;
    }
    resume();
    result
}

pub(crate) fn suspend_terminal_for_editor() {
    use crossterm::event::{DisableBracketedPaste, DisableFocusChange, DisableMouseCapture};

    // The caller publishes the handoff only after this function returns, so a
    // termination signal firing between these restores and the publication
    // runs a full cleanup instead of skipping one (greptile #4230626960).
    //
    // These commands are safe even when a mode was not enabled. Disable them
    // before leaving the alternate screen so the child receives normal input.
    let _ = crossterm::execute!(
        std::io::stdout(),
        DisableBracketedPaste,
        DisableFocusChange,
        DisableMouseCapture
    );
    crate::tui::disable_keyboard_enhancement();
    jcode_tui_style::restore_terminal_quietly();
}

pub(crate) fn resume_terminal_after_editor() {
    use crossterm::event::{EnableBracketedPaste, EnableFocusChange, EnableMouseCapture};

    // Defense in depth for the run_interactive_editor_with early return: a
    // termination signal already recorded that the process is exiting, so
    // enabling modes here would leave them active on the shell after death
    // (greptile #4232838288).
    if crate::tui::editor_owns_terminal() {
        return;
    }
    // The caller reclaimed the handoff before entering here, so a termination
    // signal arriving while these modes are re-armed runs the full cleanup.
    // Re-enter the TUI before returning to the event loop. The existing
    // ratatui Terminal remains usable and the next loop iteration redraws it.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(ratatui::init));
    let policy = crate::perf::tui_policy();
    let _ = crossterm::execute!(std::io::stdout(), EnableBracketedPaste);
    if policy.enable_focus_change {
        let _ = crossterm::execute!(std::io::stdout(), EnableFocusChange);
    }
    if policy.enable_mouse_capture {
        let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    }
    if policy.enable_keyboard_enhancement {
        crate::tui::enable_keyboard_enhancement();
    }
}

#[cfg(all(test, unix))]
mod interactive_editor_tests {
    use super::run_interactive_editor_with;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn handoff_waits_for_editor_before_resuming_terminal() {
        let _guard = crate::tui::TERMINAL_OWNERSHIP_TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        crate::tui::set_editor_terminal_ownership(false);
        let events = Rc::new(RefCell::new(Vec::new()));
        let child_events = Rc::clone(&events);
        let mut command = std::process::Command::new("sh");
        command.args(["-c", "sleep 0.05"]);

        let started = std::time::Instant::now();
        let status = run_interactive_editor_with(
            &mut command,
            {
                let events = Rc::clone(&events);
                move || events.borrow_mut().push("suspend")
            },
            move || {
                assert!(
                    started.elapsed() >= std::time::Duration::from_millis(40),
                    "terminal resumed before the editor process exited"
                );
                child_events.borrow_mut().push("resume");
            },
        )
        .expect("editor command should run");

        assert!(status.success());
        assert_eq!(&*events.borrow(), &["suspend", "resume"]);
    }

    #[test]
    fn handoff_resumes_terminal_after_editor_failure_status() {
        let _guard = crate::tui::TERMINAL_OWNERSHIP_TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        crate::tui::set_editor_terminal_ownership(false);
        let resumed = Rc::new(RefCell::new(false));
        let resumed_after = Rc::clone(&resumed);
        let mut command = std::process::Command::new("sh");
        command.args(["-c", "exit 7"]);

        let status = run_interactive_editor_with(
            &mut command,
            || {},
            move || {
                *resumed_after.borrow_mut() = true;
            },
        )
        .expect("shell should launch");

        assert_eq!(status.code(), Some(7));
        assert!(*resumed.borrow());
        assert!(!crate::tui::editor_owns_terminal());
    }

    #[test]
    fn handoff_clears_editor_terminal_ownership_when_command_fails_to_launch() {
        let _guard = crate::tui::TERMINAL_OWNERSHIP_TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        crate::tui::set_editor_terminal_ownership(false);
        let mut command = std::process::Command::new("/definitely/missing/jcode-editor-test");

        let result = run_interactive_editor_with(&mut command, || {}, || {});

        assert!(result.is_err());
        assert!(!crate::tui::editor_owns_terminal());
    }

    #[test]
    fn handoff_publishes_ownership_only_while_editor_runs() {
        let _guard = crate::tui::TERMINAL_OWNERSHIP_TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        crate::tui::set_editor_terminal_ownership(false);
        let saw_ownership = Arc::new(AtomicBool::new(false));
        let owns_during_resume = Rc::new(RefCell::new(true));

        // The child stays alive until the sampler confirms the flag, so a busy
        // runner delaying the sampler cannot shrink the observation window
        // below what the sampler needs (greptile #4231010584). With a fixed
        // child sleep the test failed on correct code whenever the sampler
        // started late. The child gives up after ~5s so broken code that never
        // publishes still fails the assertions instead of hanging.
        let sentinel = std::env::temp_dir().join(format!(
            "jcode-editor-ownership-{}-{}.sentinel",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_file(&sentinel);

        let sampler_sentinel = Arc::clone(&saw_ownership);
        let sampler_saw = sentinel.clone();
        let sampler = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while std::time::Instant::now() < deadline {
                if crate::tui::editor_owns_terminal() {
                    sampler_sentinel.store(true, Ordering::SeqCst);
                    let _ = std::fs::write(&sampler_saw, b"");
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        });

        let mut command = std::process::Command::new("sh");
        command
            .env("JCODE_OWNERSHIP_SENTINEL", &sentinel)
            .args([
                "-c",
                "i=0; while [ ! -f \"$JCODE_OWNERSHIP_SENTINEL\" ] && [ \"$i\" -lt 500 ]; do sleep 0.01; i=$((i+1)); done; [ -f \"$JCODE_OWNERSHIP_SENTINEL\" ]",
            ]);
        let owns_during_resume_after = Rc::clone(&owns_during_resume);
        let status = run_interactive_editor_with(
            &mut command,
            // The restores are still in flight here: the flag must stay false
            // so a termination signal in this window cleans up in full instead
            // of skipping it and leaving the shell in raw mode (greptile
            // #4230626960).
            || assert!(!crate::tui::editor_owns_terminal()),
            move || *owns_during_resume_after.borrow_mut() = crate::tui::editor_owns_terminal(),
        )
        .expect("shell should launch");

        sampler.join().expect("sampler thread");
        let _ = std::fs::remove_file(&sentinel);

        // Exit 0 means the sampler confirmed ownership while the child was
        // still alive; a nonzero exit means the child gave up waiting, which
        // only happens when the flag was never published.
        assert_eq!(
            status.code(),
            Some(0),
            "editor exited before the sampler observed ownership"
        );
        assert!(
            saw_ownership.load(Ordering::SeqCst),
            "ownership was never published while the editor ran"
        );
        // Reclaimed before resume re-armed jcode's modes.
        assert!(!*owns_during_resume.borrow());
        assert!(!crate::tui::editor_owns_terminal());
    }

    #[test]
    fn reclaim_resume_does_not_reenable_modes_after_termination_started() {
        let _guard = crate::tui::TERMINAL_OWNERSHIP_TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        crate::tui::set_editor_terminal_ownership(false);

        // Greptile #4232838288: reproduce the race window. The signal cleanup
        // saw the editor handoff, skipped its restore, recorded termination
        // under the kitty state lock, and released the lock while the process
        // was still alive. The editor then exited before the process did, so
        // this reclaim runs inside that gap and must not let the resume
        // re-arm modes that no later cleanup would ever disable again.
        crate::tui::publish_editor_terminal_handoff();
        crate::tui::record_termination_started();

        let resumed = Rc::new(RefCell::new(false));
        let mut command = std::process::Command::new("sh");
        command.args(["-c", "exit 0"]);
        let resumed_after = Rc::clone(&resumed);
        let status = run_interactive_editor_with(
            &mut command,
            || {},
            move || {
                *resumed_after.borrow_mut() = true;
            },
        )
        .expect("shell should launch");

        assert!(status.success());
        // The reclaim refused while terminating, so ownership stays published
        // and the resume that re-arms every mode never runs.
        assert!(
            crate::tui::editor_owns_terminal(),
            "reclaim cleared ownership while termination had already started"
        );
        assert!(
            !*resumed.borrow(),
            "resume re-armed terminal modes after termination started"
        );
        // A late keyboard push is refused too, so no entry is left unpopped
        // for the shell to inherit.
        assert!(!crate::tui::enable_keyboard_enhancement());

        // Restore the process-global flags for the other tests in this binary.
        crate::tui::set_editor_terminal_ownership(false);
        crate::tui::reset_termination_started_for_test();
    }
}
