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

fn write_mode_for_decoder(
    writer: &mut impl Write,
    action: Action,
    inside_tmux: bool,
    flags: u8,
    native_console: bool,
) -> io::Result<()> {
    if native_console {
        // Clear reporting left by an older client before the native decoder reads.
        writer.write_all(b"\x1b[=0u\x1b[>4;0m")?;
        writer.flush()?;
        return match action {
            Action::Push => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "kitty keyboard reporting is incompatible with the Windows console input decoder",
            )),
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
            // Focus/resume uses the configured flag even after Push failed.
            for _ in 0..3 {
                write_mode_for_decoder(&mut output, Action::Set, tmux, 7, true).unwrap();
            }
            write_mode_for_decoder(&mut output, Action::Pop, tmux, 7, true).unwrap();
            assert_eq!(output, b"\x1b[=0u\x1b[>4;0m".repeat(5));
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
        if cfg!(windows) {
            assert_eq!(output, b"\x1b[=0u\x1b[>4;0m");
        } else {
            assert_eq!(output, b"\x1b[>7u");
        }
    }
}
