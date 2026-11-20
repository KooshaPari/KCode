use super::*;

pub(super) async fn read_handshake<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<NativeHandshake> {
    let frame = read_bounded_line(reader).await?;
    let handshake: NativeHandshake =
        serde_json::from_slice(&frame).context("invalid native SSH handshake JSON")?;
    if handshake.kind != "kcode-native-stdio" || handshake.protocol != PROTOCOL {
        bail!(
            "unsupported native SSH protocol {} ({})",
            handshake.protocol,
            handshake.kind
        );
    }
    if handshake.version.is_empty()
        || handshake.socket_path.is_empty()
        || handshake.working_dir.is_empty()
    {
        bail!("incomplete native SSH handshake metadata");
    }
    Ok(handshake)
}

pub(super) async fn read_bounded_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Vec<u8>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            bail!("remote closed before native SSH handshake");
        }
        let newline = available.iter().position(|b| *b == b'\n');
        let count = newline.map_or(available.len(), |n| n + 1);
        if frame.len() + count > HANDSHAKE_LIMIT {
            bail!("native SSH handshake exceeds {HANDSHAKE_LIMIT} bytes");
        }
        frame.extend_from_slice(&available[..count]);
        reader.consume(count);
        if newline.is_some() {
            break;
        }
    }
    Ok(frame)
}

pub(super) async fn verify_daemon_protocol<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin>(
    reader: &mut R,
    writer: &mut W,
) -> Result<()> {
    tokio::time::timeout(STARTUP_TIMEOUT, async {
        writer.write_all(b"{\"type\":\"ping\",\"id\":0}\n").await?;
        writer.flush().await?;
        let frame = read_bounded_line(reader).await?;
        let pong: serde_json::Value = serde_json::from_slice(&frame)?;
        if pong["type"] != "pong" || pong["id"].as_u64() != Some(0)
            || pong["native_ssh_protocol"].as_u64() != Some(u64::from(PROTOCOL)) {
            bail!("remote daemon does not support native SSH protocol {PROTOCOL}; update/reload the remote Jcode server or select a matching daemon socket");
        }
        Ok::<_, anyhow::Error>(())
    }).await.context("remote daemon native SSH capability handshake timed out")??;
    Ok(())
}
