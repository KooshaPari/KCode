use super::*;

pub(crate) async fn run_stdio(socket: PathBuf) -> Result<()> {
    use std::io::Write;
    // Tokio stdin is uncancellable and can keep runtime shutdown alive after
    // daemon death while SSH still holds stdin open. A plain thread cannot.
    let (input, mut writer) = std::os::unix::net::UnixStream::pair()?;
    input.set_nonblocking(true)?;
    let input = UnixStream::from_std(input)?;
    std::thread::Builder::new()
        .name("native-ssh-stdin".into())
        .spawn(move || {
            if let Err(error) = std::io::copy(&mut std::io::stdin().lock(), &mut writer) {
                crate::logging::warn(&format!("Native SSH stdin copy ended: {error}"));
            }
            if let Err(error) = writer.flush() {
                crate::logging::warn(&format!("Native SSH stdin flush failed: {error}"));
            }
            if let Err(error) = writer.shutdown(std::net::Shutdown::Write) {
                crate::logging::warn(&format!("Native SSH stdin shutdown failed: {error}"));
            }
        })?;
    bridge_stream(input, tokio::io::stdout(), socket).await
}

pub(super) async fn bridge_stream<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut input: R,
    mut output: W,
    socket: PathBuf,
) -> Result<()> {
    let daemon = tokio::time::timeout(STARTUP_TIMEOUT, UnixStream::connect(&socket))
        .await
        .context("native daemon connection timed out")??;
    let (read, mut write) = daemon.into_split();
    let mut read = BufReader::new(read);
    verify_daemon_protocol(&mut read, &mut write).await?;
    let handshake = NativeHandshake {
        kind: "kcode-native-stdio".into(),
        protocol: PROTOCOL,
        version: jcode_build_meta::version().to_string(),
        working_dir: std::env::current_dir()?.to_string_lossy().into_owned(),
        socket_path: socket.to_string_lossy().into_owned(),
    };
    let mut header = serde_json::to_vec(&handshake)?;
    header.push(b'\n');
    if header.len() > HANDSHAKE_LIMIT {
        bail!("native SSH handshake metadata too large");
    }
    output.write_all(&header).await?;
    output.flush().await?;
    {
        let upload = async {
            tokio::io::copy(&mut input, &mut write).await?;
            // A shell pipeline closes stdin after its last request. Forward
            // that half-close but do not discard replies already in flight.
            write.shutdown().await
        };
        let download = tokio::io::copy(&mut read, &mut output);
        tokio::pin!(upload, download);
        tokio::select! {
            result = &mut upload => {
                result?;
                // Detaching a subscribed client must not wait for a remote
                // model turn. Drain final protocol replies for a bounded time.
                if let Ok(result) = tokio::time::timeout(Duration::from_secs(5), &mut download).await {
                    result?;
                }
            },
            result = &mut download => { result?; },
        }
    }
    output.flush().await?;
    Ok(())
}
