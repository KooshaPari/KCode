#[cfg(not(windows))]
use super::{
    disable_keyboard_enhancement_to, enable_keyboard_enhancement_to,
    reapply_keyboard_enhancement_to,
};
use super::{reapply_terminal_modes_after_focus_to, reapply_terminal_modes_to};

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
    let expected: &[u8] = if cfg!(windows) {
        b"\x1b[?2004h\x1b[=0u\x1b[>4;0m"
    } else {
        b"\x1b[?2004h"
    };
    assert_eq!(output, expected);
}

#[test]
fn reapply_omits_mouse_sequences_when_capture_is_disabled() {
    let mut output = Vec::new();
    reapply_terminal_modes_to(&mut output, false, true, true).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.starts_with("\x1b[?2004h\x1b[?1004h"));
    assert!(!output.contains("\x1b[?1000h"));
    assert!(output.contains(if cfg!(windows) {
        "\x1b[=0u"
    } else {
        "\x1b[=1u"
    }));
}

#[test]
fn reapply_emits_configured_idempotent_modes_without_keyboard_push() {
    let mut output = Vec::new();
    reapply_terminal_modes_to(&mut output, true, true, true).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("\x1b[?2004h"));
    assert!(output.contains("\x1b[?1004h"));
    assert!(output.contains("\x1b[?1000h"));
    assert!(output.contains(if cfg!(windows) {
        "\x1b[=0u"
    } else {
        "\x1b[=1u"
    }));
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
            reapply_terminal_modes_after_focus_to(&mut output, mouse_capture, keyboard_enhanced)
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
            assert_eq!(output.contains("\x1b[=0u\x1b[>4;0m"), cfg!(windows));
            assert!(!output.contains("\x1b[<1u"));
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

#[test]
#[cfg(windows)]
fn repeated_focus_resets_native_keyboard_without_changing_stack() {
    for keyboard_enhanced in [false, true] {
        let mut output = Vec::new();
        for _ in 0..3 {
            reapply_terminal_modes_after_focus_to(&mut output, false, keyboard_enhanced).unwrap();
        }
        assert_eq!(output, b"\x1b[?2004h\x1b[=0u\x1b[>4;0m".repeat(3));
    }
}

#[test]
#[cfg(windows)]
fn focus_reapply_propagates_write_and_flush_errors() {
    struct FailingWriter(bool);
    impl std::io::Write for FailingWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if !self.0 && bytes.starts_with(b"\x1b[=0u") {
                Err(std::io::ErrorKind::BrokenPipe.into())
            } else {
                Ok(bytes.len())
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
    }
    for fail_on_flush in [false, true] {
        let error =
            reapply_terminal_modes_after_focus_to(&mut FailingWriter(fail_on_flush), false, false)
                .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
}
