use std::fmt;
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

#[derive(Debug)]
pub(in crate::tui) enum LocalSshGateError {
    TargetChanged,
    Incompatible,
    Timeout,
    ProtocolFailure,
    Unavailable,
}

impl fmt::Display for LocalSshGateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TargetChanged => write!(f, "SSH target metadata changed"),
            Self::Incompatible => write!(f, "SSH local adapter protocol is incompatible"),
            Self::Timeout => write!(f, "SSH local adapter readiness timed out"),
            Self::ProtocolFailure => write!(f, "SSH remote protocol failed"),
            Self::Unavailable => write!(f, "SSH local adapter closed before readiness"),
        }
    }
}

impl std::error::Error for LocalSshGateError {}

pub(super) async fn wait_ready<R: AsyncBufRead + Unpin>(reader: &mut R) -> anyhow::Result<()> {
    wait_ready_with_timeout(reader, Duration::from_secs(30)).await
}

async fn wait_ready_with_timeout<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    timeout: Duration,
) -> anyhow::Result<()> {
    let mut frame = Vec::new();
    let count = tokio::time::timeout(timeout, async {
        reader.take(64).read_until(b'\n', &mut frame).await
    })
    .await
    .map_err(|_| LocalSshGateError::Timeout)?
    .map_err(|_| LocalSshGateError::Unavailable)?;
    if count == 0 {
        return Err(LocalSshGateError::Unavailable.into());
    }
    match frame.as_slice() {
        b"KCODE-SSH-LOCAL/1 READY\n" => Ok(()),
        b"KCODE-SSH-LOCAL/1 REJECT\n" => Err(LocalSshGateError::TargetChanged.into()),
        _ => Err(LocalSshGateError::Incompatible.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncWriteExt, BufReader};

    #[tokio::test]
    async fn local_gate_accepts_only_exact_ready_and_refuses_mixed_or_malformed() {
        for (frame, expected) in [
            (b"KCODE-SSH-LOCAL/1 READY\n".as_slice(), true),
            (b"KCODE-SSH-LOCAL/1 REJECT\n".as_slice(), false),
            (b"KCODE-SSH-LOCAL/2 READY\n".as_slice(), false),
            (b"old-daemon-event\n".as_slice(), false),
        ] {
            let (mut writer, reader) = tokio::io::duplex(128);
            writer.write_all(frame).await.unwrap();
            let result =
                wait_ready_with_timeout(&mut BufReader::new(reader), Duration::from_secs(1)).await;
            assert_eq!(result.is_ok(), expected);
            if !expected {
                assert!(
                    result
                        .unwrap_err()
                        .downcast_ref::<LocalSshGateError>()
                        .is_some()
                );
            }
        }
    }

    #[tokio::test]
    async fn local_gate_times_out_without_sending_anything() {
        let (_writer, reader) = tokio::io::duplex(128);
        let error = wait_ready_with_timeout(&mut BufReader::new(reader), Duration::from_millis(20))
            .await
            .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<LocalSshGateError>(),
            Some(LocalSshGateError::Timeout)
        ));
    }

    #[tokio::test]
    async fn local_gate_eof_and_partial_frame_pause_without_readiness() {
        for frame in [b"".as_slice(), b"KCODE-SSH-LOCAL/1 RE".as_slice()] {
            let (mut writer, reader) = tokio::io::duplex(128);
            writer.write_all(frame).await.unwrap();
            drop(writer);
            let error = wait_ready_with_timeout(&mut BufReader::new(reader), Duration::from_secs(1))
                .await
                .unwrap_err();
            assert!(error.downcast_ref::<LocalSshGateError>().is_some());
        }
    }

    #[tokio::test]
    async fn ready_preface_preserves_following_native_bytes() {
        let (mut writer, reader) = tokio::io::duplex(128);
        writer
            .write_all(b"KCODE-SSH-LOCAL/1 READY\n{\"type\":\"session\"}\n")
            .await
            .unwrap();
        let mut reader = BufReader::new(reader);
        wait_ready_with_timeout(&mut reader, Duration::from_secs(1))
            .await
            .unwrap();
        let mut following = String::new();
        reader.read_line(&mut following).await.unwrap();
        assert_eq!(following, "{\"type\":\"session\"}\n");
    }
}
