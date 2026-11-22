use super::*;

pub(super) async fn forward_matching_connection(
    mut stream: UnixStream,
    mut ssh: SshConnection,
    expected: &NativeHandshake,
    mut stopped: watch::Receiver<bool>,
) -> Result<()> {
    if let Err(error) = expected.verify_target_metadata(&ssh.handshake) {
        eprintln!("{error:#}");
        if let Err(write_error) = stream.write_all(b"KCODE-SSH-LOCAL/1 REJECT\n").await {
            crate::logging::warn(&format!(
                "SSH local rejection could not be delivered: {write_error}"
            ));
        }
        ssh.shutdown().await;
        return Err(error);
    }
    if let Err(error) = stream.write_all(b"KCODE-SSH-LOCAL/1 READY\n").await {
        ssh.shutdown().await;
        return Err(error.into());
    }
    let (mut read, mut write) = stream.into_split();
    let outcome = tokio::select! {
        biased;
        _ = stopped.changed() => Ok(0),
        result = tokio::io::copy(&mut read, &mut ssh.writer) => result,
        result = tokio::io::copy(&mut ssh.reader, &mut write) => result,
    };
    ssh.shutdown().await;
    if let Err(error) = outcome {
        bail!("native SSH stream failed: {error}; {}", ssh.diagnostic());
    }
    Ok(())
}
