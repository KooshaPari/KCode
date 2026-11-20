use super::*;

fn hello() -> String {
    serde_json::to_string(&NativeHandshake {
        kind: "kcode-native-stdio".into(),
        protocol: PROTOCOL,
        version: "test-build".into(),
        working_dir: "/remote/home".into(),
        socket_path: "/remote/native.sock".into(),
    })
    .unwrap()
        + "\n"
}

#[test]
fn reconnect_rejects_changed_target_metadata() {
    let expected: NativeHandshake = serde_json::from_str(&hello()).unwrap();
    assert!(expected.verify_target_metadata(&expected).is_ok());
    for field in ["version", "working_dir", "socket_path"] {
        let mut observed = expected.clone();
        match field {
            "version" => observed.version.push_str("-changed"),
            "working_dir" => observed.working_dir.push_str("-changed"),
            _ => observed.socket_path.push_str("-changed"),
        }
        assert!(
            expected.verify_target_metadata(&observed).is_err(),
            "{field}"
        );
    }
}

#[tokio::test]
async fn mismatch_closes_owned_bridge_before_forwarding() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("forwarded");
    let script = format!(
        "printf '%s\\n' '{}'; if IFS= read -r line; then printf '%s' \"$line\" > '{}'; fi",
        hello().trim(),
        marker.display()
    );
    let ssh = SshConnection::spawn(shell(&script), STARTUP_TIMEOUT)
        .await
        .unwrap();
    let pid = ssh.child.0.id().unwrap() as i32;
    let daemon_socket = directory.path().join("independent-daemon.sock");
    let daemon = UnixListener::bind(&daemon_socket).unwrap();
    let mut independent = shell("exec sleep 60");
    independent.as_std_mut().process_group(0);
    independent.kill_on_drop(true);
    let mut independent = independent.spawn().unwrap();
    let mut expected: NativeHandshake = serde_json::from_str(&hello()).unwrap();
    expected.socket_path.push_str("-different");
    let (mut client, bridge) = UnixStream::pair().unwrap();
    client.write_all(b"must-not-forward\n").await.unwrap();
    let (_stop, stopped) = watch::channel(false);
    let error = forward_matching_connection(bridge, ssh, &expected, stopped)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("SSH reconnect refused"));
    assert!(!marker.exists());
    assert_eq!(
        unsafe { libc::kill(pid, 0) },
        -1,
        "owned bridge must be reaped"
    );
    let _client = UnixStream::connect(&daemon_socket).await.unwrap();
    let _accepted = daemon.accept().await.unwrap();
    assert!(
        independent.try_wait().unwrap().is_none(),
        "independent fixture must remain alive"
    );
    independent.start_kill().unwrap();
    independent.wait().await.unwrap();
}

#[tokio::test]
async fn matching_metadata_forwards_the_stream() {
    let script = format!(
        "printf '%s\\n' '{}'; IFS= read -r line; printf 'echo:%s\\n' \"$line\"",
        hello().trim()
    );
    let ssh = SshConnection::spawn(shell(&script), STARTUP_TIMEOUT)
        .await
        .unwrap();
    let expected: NativeHandshake = serde_json::from_str(&hello()).unwrap();
    let (mut client, bridge) = UnixStream::pair().unwrap();
    client.write_all(b"hello\n").await.unwrap();
    let (_stop, stopped) = watch::channel(false);
    forward_matching_connection(bridge, ssh, &expected, stopped)
        .await
        .unwrap();
    let mut reply = String::new();
    client.read_to_string(&mut reply).await.unwrap();
    assert_eq!(reply, "echo:hello\n");
}

fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(script);
    command
}

#[test]
fn rejects_option_and_shell_injection() {
    for host in [
        "",
        "-oProxyCommand=bad",
        "host;echo",
        "user@host@other",
        "$(id)",
        "user name@host",
        "host\nother",
    ] {
        assert!(SshOptions::new(host, "jcode").is_err(), "{host}");
    }
    for host in ["kcode-dev", "user@host", "[::1]", "host.example"] {
        assert!(SshOptions::new(host, "jcode").is_ok(), "{host}");
    }
    for binary in ["", "--help", "jcode\nfalse"] {
        assert!(SshOptions::new("host", binary).is_err());
    }
}

#[test]
fn command_is_owned_noninteractive_and_quotes_literal_paths() {
    let mut options = SshOptions::new("user@jcode-dev", "/a path/jcode'quoted").unwrap();
    options.daemon_socket = Some("/socket path/native'quoted".into());
    options.working_dir = Some("/workspace with 'quotes'".into());
    let command = options.command();
    let args: Vec<_> = command
        .as_std()
        .get_args()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    for required in [
        "-T",
        "BatchMode=yes",
        "StrictHostKeyChecking=yes",
        "ForwardAgent=no",
        "ClearAllForwardings=yes",
        "ControlMaster=no",
        "none",
        "ForkAfterAuthentication=no",
    ] {
        assert!(args.iter().any(|s| s == required), "{required}");
    }
    let remote = args.last().unwrap();
    assert!(remote.contains("exec '/a path/jcode'\\''quoted'"));
    assert!(remote.contains("--socket '/socket path/native'\\''quoted'"));
    assert!(remote.contains("--cwd '/workspace with '\\''quotes'\\'''"));
    assert!(remote.ends_with("server stdio"));
    assert_eq!(args[args.len() - 2], "user@jcode-dev");
}

#[tokio::test]
async fn handshake_preserves_following_native_bytes() {
    let wire = hello() + "{\"event\":\"native\"}\n";
    let mut reader = BufReader::new(wire.as_bytes());
    let header = read_handshake(&mut reader).await.unwrap();
    assert_eq!(header.working_dir, "/remote/home");
    let mut remainder = String::new();
    reader.read_to_string(&mut remainder).await.unwrap();
    assert_eq!(remainder, "{\"event\":\"native\"}\n");
}

#[tokio::test]
async fn handshake_rejects_contamination_unknown_protocol_and_oversize() {
    for wire in [
        "Welcome!\n".to_owned() + &hello(),
        hello().replace("\"protocol\":1", "\"protocol\":99"),
        "x".repeat(HANDSHAKE_LIMIT + 1),
        String::new(),
    ] {
        let mut reader = BufReader::new(wire.as_bytes());
        assert!(read_handshake(&mut reader).await.is_err());
    }
}

#[tokio::test]
async fn startup_timeout_kills_and_reaps_owned_process() {
    let directory = tempfile::tempdir().unwrap();
    let pid_file = directory.path().join("pid");
    let script = format!("echo $$ > '{}'; exec sleep 60", pid_file.display());
    let start = std::time::Instant::now();
    let result = SshConnection::spawn(shell(&script), Duration::from_millis(100)).await;
    let error = result.err().expect("must time out");
    assert!(format!("{error:#}").contains("timed out"));
    assert!(start.elapsed() < Duration::from_secs(3));
    let pid: i32 = std::fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "child must be reaped");
}

#[tokio::test]
async fn successful_connection_shutdown_reaps_child() {
    let script = format!("printf '{}'; exec sleep 60", hello());
    let mut connection = SshConnection::spawn(shell(&script), Duration::from_secs(2))
        .await
        .unwrap();
    let pid = connection.child.0.id().unwrap() as i32;
    assert_eq!(connection.handshake.protocol, PROTOCOL);
    connection.shutdown().await;
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
}

#[tokio::test]
async fn cancelled_handshake_retains_child_for_explicit_reaping() {
    let mut connection = SshConnection::spawn_process(shell("exec sleep 60")).unwrap();
    let pid = connection.child.0.id().unwrap() as i32;
    assert!(
        tokio::time::timeout(
            Duration::from_millis(20),
            connection.establish(STARTUP_TIMEOUT)
        )
        .await
        .is_err()
    );
    connection.shutdown().await;
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
}

#[tokio::test]
async fn guard_drop_removes_private_socket_and_signals_children() {
    use std::os::unix::fs::PermissionsExt;
    let directory = private_directory().unwrap();
    let root = directory.path().to_path_buf();
    assert_eq!(
        std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let socket = root.join("native.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let (stop, mut stopped) = watch::channel(false);
    let guard = NativeSsh {
        _directory: directory,
        socket: socket.clone(),
        host: "test".into(),
        handshake: serde_json::from_str(&hello()).unwrap(),
        stop,
        manager: None,
    };
    assert!(socket.exists());
    drop(guard);
    assert!(!socket.exists());
    assert!(!root.exists());
    stopped.changed().await.unwrap();
    assert!(*stopped.borrow());
    drop(listener);
}

#[tokio::test]
async fn explicit_close_waits_for_child_reaping_and_is_idempotent() {
    let mut child = SshConnection::spawn(
        shell(&format!("printf '{}'; exec sleep 60", hello())),
        Duration::from_secs(2),
    )
    .await
    .unwrap();
    let pid = child.child.0.id().unwrap() as i32;
    let directory = private_directory().unwrap();
    let socket = directory.path().join("native.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let (stop, mut stopped) = watch::channel(false);
    let manager = tokio::spawn(async move {
        let _ = stopped.changed().await;
        child.shutdown().await;
        drop(listener);
    });
    let mut guard = NativeSsh {
        _directory: directory,
        socket: socket.clone(),
        host: "test".into(),
        handshake: serde_json::from_str(&hello()).unwrap(),
        stop,
        manager: Some(manager),
    };
    guard.close().await.unwrap();
    assert!(!socket.exists());
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    guard.close().await.unwrap();
}

#[tokio::test]
async fn workspace_validation_rejects_empty_and_control_paths_before_ssh() {
    for path in ["", "bad\npath", "bad\0path"] {
        let error = NativeSsh::connect_with_workspace("test", "jcode", None, Some(path))
            .await
            .err()
            .expect("invalid path must fail");
        assert!(error.to_string().contains("remote working directory"));
    }
}

#[tokio::test]
async fn failed_startup_reports_bounded_stderr() {
    let error = SshConnection::spawn(
        shell("printf 'Host key verification failed' >&2; exit 255"),
        Duration::from_secs(2),
    )
    .await
    .err()
    .unwrap();
    assert!(format!("{error:#}").contains("Host key verification failed"));
    let script = format!(
        "i=0; while [ $i -lt 5000 ]; do printf 0123456789 >&2; i=$((i+1)); done; printf '{}' ; exec sleep 60",
        hello().trim_end()
    );
    // No newline ensures timeout after stderr has been fully drained.
    let error = SshConnection::spawn(shell(&script), Duration::from_millis(500))
        .await
        .err()
        .unwrap();
    assert!(format!("{error:#}").len() < STDERR_LIMIT + 1024);
}

#[tokio::test]
async fn bridge_exchanges_native_frames_and_exits_when_daemon_dies() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let (client, bridge) = tokio::io::duplex(4096);
    let (input, output) = tokio::io::split(bridge);
    let task = tokio::spawn(bridge_stream(input, output, socket));
    let (daemon, _) = listener.accept().await.unwrap();
    let (read, mut write) = daemon.into_split();
    let mut read = BufReader::new(read);
    let mut line = String::new();
    read.read_line(&mut line).await.unwrap();
    assert_eq!(line, "{\"type\":\"ping\",\"id\":0}\n");
    write
        .write_all(b"{\"type\":\"pong\",\"id\":0,\"native_ssh_protocol\":1}\n")
        .await
        .unwrap();
    let (mut client_read, mut client_write) = tokio::io::split(client);
    let mut client_read = BufReader::new(&mut client_read);
    read_handshake(&mut client_read).await.unwrap();
    client_write
        .write_all(b"{\"type\":\"ping\",\"id\":1}\n")
        .await
        .unwrap();
    line.clear();
    read.read_line(&mut line).await.unwrap();
    assert!(line.contains("ping"));
    write
        .write_all(b"{\"type\":\"pong\",\"id\":1}\n")
        .await
        .unwrap();
    line.clear();
    client_read.read_line(&mut line).await.unwrap();
    assert!(line.contains("pong"));
    drop(read);
    drop(write);
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    // client_write deliberately remains open: daemon EOF must still exit.
    drop(client_write);
}

#[tokio::test]
async fn bridge_does_not_claim_handshake_without_daemon() {
    let (client, bridge) = tokio::io::duplex(1024);
    let (input, output) = tokio::io::split(bridge);
    let directory = tempfile::tempdir().unwrap();
    assert!(
        bridge_stream(input, output, directory.path().join("missing.sock"))
            .await
            .is_err()
    );
    let mut client = client;
    let mut output = Vec::new();
    client.read_to_end(&mut output).await.unwrap();
    assert!(output.is_empty());
}

#[tokio::test]
async fn stdio_eof_half_closes_daemon_and_drains_last_reply() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let (client, bridge) = tokio::io::duplex(4096);
    let (input, output) = tokio::io::split(bridge);
    let task = tokio::spawn(bridge_stream(input, output, socket));
    let (daemon, _) = listener.accept().await.unwrap();
    let (read, mut write) = daemon.into_split();
    let mut read = BufReader::new(read);
    let mut line = String::new();
    read.read_line(&mut line).await.unwrap();
    write
        .write_all(b"{\"type\":\"pong\",\"id\":0,\"native_ssh_protocol\":1}\n")
        .await
        .unwrap();
    let (read_client, mut write_client) = tokio::io::split(client);
    let mut read_client = BufReader::new(read_client);
    read_handshake(&mut read_client).await.unwrap();
    write_client
        .write_all(b"{\"type\":\"ping\",\"id\":1}\n")
        .await
        .unwrap();
    write_client.shutdown().await.unwrap();
    line.clear();
    read.read_line(&mut line).await.unwrap();
    assert!(line.contains("ping"));
    line.clear();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), read.read_line(&mut line))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    // Deliberately reply only after observing the client's write EOF.
    write
        .write_all(b"{\"type\":\"pong\",\"id\":1}\n")
        .await
        .unwrap();
    drop(write);
    drop(read);
    read_client.read_to_string(&mut line).await.unwrap();
    assert!(line.contains("pong"));
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn daemon_capability_requires_supported_protocol_and_matching_ping() {
    for frame in [
        "{\"type\":\"pong\",\"id\":0}\n",
        "{\"type\":\"pong\",\"id\":0,\"native_ssh_protocol\":2}\n",
        "{\"type\":\"pong\",\"id\":1,\"native_ssh_protocol\":1}\n",
    ] {
        let mut reader = BufReader::new(frame.as_bytes());
        let mut sink = tokio::io::sink();
        let error = verify_daemon_protocol(&mut reader, &mut sink)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("update/reload"));
    }
    let frame = b"{\"type\":\"pong\",\"id\":0,\"native_ssh_protocol\":1}\nnative-frame\n";
    let mut reader = BufReader::new(&frame[..]);
    verify_daemon_protocol(&mut reader, &mut tokio::io::sink())
        .await
        .unwrap();
    let mut remaining = String::new();
    reader.read_to_string(&mut remaining).await.unwrap();
    assert_eq!(remaining, "native-frame\n");
}
