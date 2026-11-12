//! Native TUI transport over owned, non-interactive OpenSSH connections.
//!
//! The private local socket is only an adapter. Each connection carries the
//! native Request/ServerEvent protocol, not the SDK harness API. Closing it
//! closes SSH and its bridge, never the remote shared daemon.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::watch;
use tokio::task::{JoinHandle, JoinSet};

mod bridge;
mod handshake;
#[cfg(test)]
use bridge::bridge_stream;
pub(crate) use bridge::run_stdio;
use handshake::{read_handshake, verify_daemon_protocol};

const PROTOCOL: u32 = 1;
const HANDSHAKE_LIMIT: usize = 8192;
const STDERR_LIMIT: usize = 16 * 1024;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CONNECTIONS: usize = 32;

fn private_directory() -> Result<tempfile::TempDir> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = std::fs::Permissions::from_mode(0o700);
    // tempfile's directory default is 0777 subject to umask, unlike its file
    // default. Set the creation mode, so there is no permissive chmod window.
    let directory = tempfile::Builder::new()
        .prefix("kcode-ssh-")
        .permissions(permissions.clone())
        .tempdir()?;
    // Restore owner access even under an unusually restrictive owner umask.
    std::fs::set_permissions(directory.path(), permissions)?;
    Ok(directory)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NativeHandshake {
    pub kind: String,
    pub protocol: u32,
    pub version: String,
    pub working_dir: String,
    pub socket_path: String,
}

impl NativeHandshake {
    fn verify_target_metadata(&self, observed: &Self) -> Result<()> {
        let changed = if self.working_dir != observed.working_dir {
            Some("workspace")
        } else if self.socket_path != observed.socket_path {
            Some("socket")
        } else if self.version != observed.version {
            Some("bridge version")
        } else {
            None
        };
        if let Some(changed) = changed {
            bail!("SSH reconnect refused: remote {changed} changed; reconnect explicitly")
        }
        Ok(())
    }
}

/// Lifetime guard for the private socket and all its SSH children.
pub(crate) struct NativeSsh {
    _directory: tempfile::TempDir,
    socket: PathBuf,
    host: String,
    handshake: NativeHandshake,
    stop: watch::Sender<bool>,
    manager: Option<JoinHandle<()>>,
}

impl NativeSsh {
    pub async fn connect_with_workspace(
        host: &str,
        remote_binary: &str,
        daemon_socket: Option<&str>,
        remote_working_dir: Option<&str>,
    ) -> Result<Self> {
        let mut options = SshOptions::new(host, remote_binary)?;
        if daemon_socket
            .is_some_and(|socket| socket.is_empty() || socket.chars().any(char::is_control))
        {
            bail!(
                "remote daemon socket must be a literal nonempty path without control characters"
            );
        }
        options.daemon_socket = daemon_socket.map(str::to_owned);
        if remote_working_dir
            .is_some_and(|path| path.is_empty() || path.chars().any(char::is_control))
        {
            bail!(
                "remote working directory must be a literal nonempty path without control characters"
            );
        }
        options.working_dir = remote_working_dir.map(str::to_owned);
        // Fail before entering the TUI, with authentication/host-key diagnostics.
        let mut probe = SshConnection::connect(&options).await?;
        let handshake = probe.handshake.clone();
        probe.shutdown().await;
        // Remote --cwd already verified the directory and the header contains
        // its resolved path. Keep reconnects on that same verified workspace.
        options.working_dir = Some(handshake.working_dir.clone());

        let directory = private_directory()?;
        let socket = directory.path().join("native.sock");
        let listener = UnixListener::bind(&socket)?;
        let (stop, stopped) = watch::channel(false);
        let manager = tokio::spawn(accept_connections(
            listener,
            options,
            handshake.clone(),
            stopped,
        ));
        Ok(Self {
            _directory: directory,
            socket,
            host: host.into(),
            handshake,
            stop,
            manager: Some(manager),
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket
    }
    pub fn host(&self) -> &str {
        &self.host
    }
    pub fn handshake(&self) -> &NativeHandshake {
        &self.handshake
    }
    pub fn remote_working_dir(&self) -> &str {
        &self.handshake.working_dir
    }

    /// Close and reap owned SSH children before the Tokio runtime shuts down.
    /// Keep the guard outside a signal/TUI select and await this on either exit.
    pub async fn close(&mut self) -> Result<()> {
        let _ = self.stop.send(true);
        let _ = std::fs::remove_file(&self.socket);
        let Some(mut manager) = self.manager.take() else {
            return Ok(());
        };
        match tokio::time::timeout(Duration::from_secs(5), &mut manager).await {
            Ok(result) => result.context("native SSH cleanup task failed"),
            Err(_) => {
                // Dropping the manager's JoinSet drops every owned-child guard.
                manager.abort();
                let _ = manager.await;
                bail!("native SSH cleanup timed out; owned child tasks were aborted")
            }
        }
    }
}

impl Drop for NativeSsh {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        // Remove the address immediately so nobody can dial after guard drop.
        let _ = std::fs::remove_file(&self.socket);
    }
}

#[derive(Clone)]
struct SshOptions {
    host: String,
    remote_binary: String,
    daemon_socket: Option<String>,
    working_dir: Option<String>,
}

impl SshOptions {
    fn new(host: &str, remote_binary: &str) -> Result<Self> {
        let valid_user = |s: &str| {
            !s.is_empty()
                && !s.starts_with('-')
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        };
        let hostname = match host.split_once('@') {
            Some((user, hostname)) if valid_user(user) => hostname,
            Some(_) => bail!("invalid SSH user in host"),
            None => host,
        };
        if hostname.is_empty()
            || hostname.starts_with('-')
            || !hostname
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-:[]%".contains(&b))
        {
            bail!("invalid SSH host: use a configured alias, hostname, or user@host");
        }
        if remote_binary.is_empty()
            || remote_binary.starts_with('-')
            || remote_binary.chars().any(char::is_control)
        {
            bail!(
                "remote binary must be an executable name or literal path without control characters"
            );
        }
        Ok(Self {
            host: host.into(),
            remote_binary: remote_binary.into(),
            daemon_socket: None,
            working_dir: None,
        })
    }

    fn command(&self) -> Command {
        let mut command = Command::new("ssh");
        command.args([
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "ForwardAgent=no",
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=2",
            "-o",
            "PermitLocalCommand=no",
            "-o",
            "ForkAfterAuthentication=no",
            "-o",
            "StdinNull=no",
            "-o",
            "RemoteCommand=none",
            "-o",
            "SessionType=default",
            "-o",
            "ControlMaster=no",
            "-S",
            "none",
            "-o",
            "ConnectTimeout=30",
        ]);
        let binary = format!("'{}'", self.remote_binary.replace('\'', "'\\''"));
        let socket = self
            .daemon_socket
            .as_ref()
            .map(|socket| format!(" --socket '{}'", socket.replace('\'', "'\\''")))
            .unwrap_or_default();
        let cwd = self
            .working_dir
            .as_ref()
            .map(|path| format!(" --cwd '{}'", path.replace('\'', "'\\''")))
            .unwrap_or_default();
        let remote = format!(
            "PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:$PATH\"; export PATH; exec {binary} --no-update --no-selfdev{socket}{cwd} server stdio"
        );
        command.arg("--").arg(&self.host).arg(remote);
        command
    }
}

struct OwnedChild(Child);
impl OwnedChild {
    fn kill(&mut self) {
        if let Some(pid) = self.0.id() {
            // Only the dedicated group we created, including ProxyCommand helpers.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
            let _ = self.0.start_kill();
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        self.kill();
    }
}

struct SshConnection {
    child: OwnedChild,
    reader: BufReader<ChildStdout>,
    writer: ChildStdin,
    stderr: Arc<Mutex<Vec<u8>>>,
    stderr_task: JoinHandle<()>,
    handshake: NativeHandshake,
}

impl SshConnection {
    async fn connect(options: &SshOptions) -> Result<Self> {
        Self::spawn(options.command(), STARTUP_TIMEOUT)
            .await
            .with_context(|| format!("connecting native Jcode on {}", options.host))
    }

    async fn spawn(command: Command, deadline: Duration) -> Result<Self> {
        let mut connection = Self::spawn_process(command)?;
        if let Err(error) = connection.establish(deadline).await {
            connection.shutdown().await;
            return Err(error.context(connection.diagnostic()));
        }
        Ok(connection)
    }

    fn spawn_process(mut command: Command) -> Result<Self> {
        command.as_std_mut().process_group(0);
        #[cfg(target_os = "linux")]
        {
            let parent_pid = std::process::id();
            // SIGKILL/process::exit do not run guards. Do not leave SSH holding
            // a remote bridge open if the local UI disappears abruptly.
            unsafe {
                command.as_std_mut().pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::getppid() as u32 != parent_pid {
                        libc::raise(libc::SIGKILL);
                    }
                    Ok(())
                });
            }
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = OwnedChild(command.spawn().context("starting system ssh")?);
        let reader = BufReader::new(child.0.stdout.take().context("SSH stdout missing")?);
        let writer = child.0.stdin.take().context("SSH stdin missing")?;
        let mut stderr_pipe = child.0.stderr.take().context("SSH stderr missing")?;
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let buffer = Arc::clone(&stderr);
        let stderr_task = tokio::spawn(async move {
            let mut chunk = [0u8; 4096];
            while let Ok(n) = stderr_pipe.read(&mut chunk).await {
                if n == 0 {
                    break;
                }
                if let Ok(mut bytes) = buffer.lock() {
                    bytes.extend_from_slice(&chunk[..n]);
                    if bytes.len() > STDERR_LIMIT {
                        let excess = bytes.len() - STDERR_LIMIT;
                        bytes.drain(..excess);
                    }
                }
            }
        });
        Ok(Self {
            child,
            reader,
            writer,
            stderr,
            stderr_task,
            handshake: NativeHandshake {
                kind: String::new(),
                protocol: 0,
                version: String::new(),
                working_dir: String::new(),
                socket_path: String::new(),
            },
        })
    }

    async fn establish(&mut self, deadline: Duration) -> Result<()> {
        self.handshake = tokio::time::timeout(deadline, read_handshake(&mut self.reader))
            .await
            .context("SSH startup/handshake timed out")??;
        Ok(())
    }

    fn diagnostic(&self) -> String {
        let stderr = self
            .stderr
            .lock()
            .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
            .unwrap_or_default();
        format!(
            "Native SSH connection closed. Verify SSH credentials/known_hosts and remote `jcode server stdio` support. {stderr}"
        )
    }

    async fn shutdown(&mut self) {
        self.child.kill();
        let _ = tokio::time::timeout(Duration::from_secs(2), self.child.0.wait()).await;
        if tokio::time::timeout(Duration::from_millis(100), &mut self.stderr_task)
            .await
            .is_err()
        {
            self.stderr_task.abort();
        }
    }
}

impl Drop for SshConnection {
    fn drop(&mut self) {
        self.stderr_task.abort();
    }
}

async fn forward_matching_connection(
    stream: UnixStream,
    mut ssh: SshConnection,
    expected: &NativeHandshake,
    mut stopped: watch::Receiver<bool>,
) -> Result<()> {
    if let Err(error) = expected.verify_target_metadata(&ssh.handshake) {
        eprintln!("{error:#}");
        ssh.shutdown().await;
        return Err(error);
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

async fn accept_connections(
    listener: UnixListener,
    options: SshOptions,
    expected: NativeHandshake,
    mut stopped: watch::Receiver<bool>,
) {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            _ = stopped.changed() => break,
            Some(_) = connections.join_next(), if !connections.is_empty() => {},
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { break; };
                if connections.len() >= MAX_CONNECTIONS { continue; }
                let options = options.clone();
                let expected = expected.clone();
                let mut stopped = stopped.clone();
                connections.spawn(async move {
                    let mut ssh = match SshConnection::spawn_process(options.command()) {
                        Ok(ssh) => ssh,
                        Err(error) => { crate::logging::warn(&format!("Native SSH reconnect failed: {error:#}")); return; }
                    };
                    let connected = tokio::select! {
                        biased;
                        _ = stopped.changed() => None,
                        connected = ssh.establish(STARTUP_TIMEOUT) => Some(connected),
                    };
                    if !matches!(connected, Some(Ok(()))) {
                        ssh.shutdown().await;
                        if let Some(Err(error)) = connected {
                            crate::logging::warn(&format!("Native SSH reconnect failed: {error:#}; {}", ssh.diagnostic()));
                        }
                        return;
                    }
                    if let Err(error) = forward_matching_connection(stream, ssh, &expected, stopped).await {
                        crate::logging::warn(&format!("{error:#}"));
                    }
                });
            }
        }
    }
    drop(listener);
    // Guard drop wakes all children, which explicitly kill and reap SSH.
    while connections.join_next().await.is_some() {}
}

/// CLI-only stdin/stdout bridge to an already-running persistent native server.
#[cfg(test)]
#[path = "ssh_transport_tests.rs"]
mod tests;
