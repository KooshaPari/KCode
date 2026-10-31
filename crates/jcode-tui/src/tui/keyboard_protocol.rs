//! Match keyboard negotiation to the active input decoder.
//!
//! Crossterm 0.29 uses ReadConsoleInputW on Windows, not its Unix CSI-u
//! parser. ConPTY must keep delivering native key records: requesting kitty
//! or modifyOtherKeys reporting can otherwise lose special keys/modifiers.
use std::io::{self, Write};

#[derive(Clone, Copy)]
pub(super) enum Action {
    Push,
    Set,
    Pop,
}

pub(super) fn write_mode(
    writer: &mut impl Write,
    action: Action,
    inside_tmux: bool,
    flags: u8,
) -> io::Result<()> {
    write_mode_for_decoder(writer, action, inside_tmux, flags, cfg!(windows))
}

pub(super) fn prepare_input(writer: &mut impl Write, enhanced: bool) -> io::Result<bool> {
    prepare_input_for_decoder(writer, enhanced, cfg!(windows))
}

fn prepare_input_for_decoder(
    writer: &mut impl Write,
    enhanced: bool,
    native_console: bool,
) -> io::Result<bool> {
    if native_console {
        // A previous client can leave reporting enabled without owning a stack push.
        writer.write_all(b"\x1b[=0u\x1b[>4;0m")?;
        writer.flush()?;
    }
    Ok(enhanced && !native_console)
}

fn write_mode_for_decoder(
    writer: &mut impl Write,
    action: Action,
    inside_tmux: bool,
    flags: u8,
    native_console: bool,
) -> io::Result<()> {
    if native_console {
        // Startup must report that no enhancement was negotiated. Focus,
        // resume and teardown are harmless even when handed configured=true.
        return match action {
            Action::Push => {
                prepare_input_for_decoder(writer, false, native_console)?;
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "kitty keyboard reporting is incompatible with the Windows console input decoder",
                ))
            }
            Action::Set | Action::Pop => Ok(()),
        };
    }
    if inside_tmux {
        writer.write_all(match action {
            Action::Pop => b"\x1b[>4;0m",
            Action::Push | Action::Set => b"\x1b[>4;2m",
        })?;
    }
    match action {
        Action::Push => write!(writer, "\x1b[>{flags}u")?,
        Action::Set => write!(writer, "\x1b[={flags}u")?,
        Action::Pop => writer.write_all(b"\x1b[<1u")?,
    }
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_never_switches_native_key_records_to_csi_u() {
        for tmux in [false, true] {
            let mut output = Vec::new();
            let error =
                write_mode_for_decoder(&mut output, Action::Push, tmux, 7, true).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::Unsupported);
            assert_eq!(output, b"\x1b[=0u\x1b[>4;0m");
            output.clear();
            // Focus/resume uses the configured flag even after Push failed.
            for _ in 0..3 {
                write_mode_for_decoder(&mut output, Action::Set, tmux, 7, true).unwrap();
            }
            write_mode_for_decoder(&mut output, Action::Pop, tmux, 7, true).unwrap();
            assert!(
                output.is_empty(),
                "must not request CSI-u or modifyOtherKeys"
            );
        }
    }

    #[test]
    fn unix_keeps_balanced_kitty_stack_and_tmux_reporting() {
        for tmux in [false, true] {
            let mut output = Vec::new();
            for action in [Action::Push, Action::Set, Action::Pop] {
                write_mode_for_decoder(&mut output, action, tmux, 7, false).unwrap();
            }
            let expected: &[u8] = if tmux {
                b"\x1b[>4;2m\x1b[>7u\x1b[>4;2m\x1b[=7u\x1b[>4;0m\x1b[<1u"
            } else {
                b"\x1b[>7u\x1b[=7u\x1b[<1u"
            };
            assert_eq!(output, expected);
        }
    }

    #[test]
    fn platform_entry_matches_native_decoder() {
        let mut output = Vec::new();
        let result = write_mode(&mut output, Action::Push, false, 7);
        assert_eq!(result.is_err(), cfg!(windows));
        let expected: &[u8] = if cfg!(windows) {
            b"\x1b[=0u\x1b[>4;0m"
        } else {
            b"\x1b[>7u"
        };
        assert_eq!(output, expected);
    }

    #[test]
    fn native_startup_and_handoff_clear_stale_modes_without_popping() {
        for inherited in [false, true] {
            let mut output = Vec::new();
            for _ in 0..3 {
                assert!(!prepare_input_for_decoder(&mut output, inherited, true).unwrap());
            }
            assert_eq!(output, b"\x1b[=0u\x1b[>4;0m".repeat(3));
        }
    }

    #[test]
    fn unix_handoff_preserves_owned_keyboard_mode_without_output() {
        for inherited in [false, true] {
            let mut output = Vec::new();
            assert_eq!(
                prepare_input_for_decoder(&mut output, inherited, false).unwrap(),
                inherited
            );
            assert!(output.is_empty());
        }
    }

    #[test]
    fn native_reset_preserves_write_and_flush_errors() {
        struct FailingWriter(bool);
        impl Write for FailingWriter {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.0 {
                    Ok(bytes.len())
                } else {
                    Err(io::ErrorKind::BrokenPipe.into())
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
        }
        for fail_on_flush in [false, true] {
            let error = write_mode_for_decoder(
                &mut FailingWriter(fail_on_flush),
                Action::Push,
                false,
                7,
                true,
            )
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        }
    }

    #[test]
    fn platform_handoff_normalizes_the_actual_decoder() {
        for inherited in [false, true] {
            let mut output = Vec::new();
            assert_eq!(
                prepare_input(&mut output, inherited).unwrap(),
                inherited && !cfg!(windows)
            );
            assert_eq!(output.is_empty(), !cfg!(windows));
        }
    }
}
