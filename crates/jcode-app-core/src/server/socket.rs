use super::Client;
use crate::transport::Stream;
use anyhow::Result;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn socket_path() -> PathBuf {
    if let Ok(custom) = std::env::var("KCODE_SOCKET") {
        return PathBuf::from(custom);
    }
    default_socket_path()
}

/// The shared-daemon socket for this user, ignoring any
/// `--socket`/`KCODE_SOCKET` override.
///
/// Deliberately resolved from a stable per-user directory rather than
/// `runtime_dir()`: the latter honors `TMPDIR` on macOS, so two launchers of the
/// same user could address two different shared sockets and a client would
/// silently attach to whichever stale daemon its `TMPDIR` happened to name.
pub fn default_socket_path() -> PathBuf {
    crate::storage::shared_runtime_dir().join("jcode.sock")
}

/// Path of the identity sidecar that sits beside `socket`.
///
/// The daemon publishes the provider/model it actually resolved to at this path
/// on startup. A client reads it to find out who is on the other end of a
/// socket *before* deciding to attach.
///
/// The sidecar is deliberately derived from the socket path rather than kept in
/// the server registry: `servers.json` is a single shared JSON map that
/// concurrent daemons load-modify-save, so one daemon silently overwrites
/// another's entry, and a client then cannot tell who it is about to attach to.
/// Anything reachable at the socket path is reachable here by construction.
pub fn identity_file_for_socket(socket: &std::path::Path) -> PathBuf {
    let mut name = socket
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_else(|| std::ffi::OsString::from("jcode.sock"));
    name.push(".identity");
    socket.with_file_name(name)
}

/// The identity the daemon at `socket` publishes, when it publishes one.
///
/// `None` means "unknown": too old a build, not a jcode daemon, or unreadable.
/// Callers must treat that as "cannot verify", never as "matches".
pub fn read_socket_identity(socket: &std::path::Path) -> Option<SocketIdentity> {
    let path = identity_file_for_socket(socket);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) => {
            // A missing sidecar is the normal case for an older daemon, so this
            // is not worth a warning; anything else is worth recording, because
            // a corrupt sidecar makes the client isolate instead of attaching.
            if error.kind() != std::io::ErrorKind::NotFound {
                crate::logging::warn(&format!(
                    "Failed to read socket identity at {}: {}",
                    path.display(),
                    error
                ));
            }
            return None;
        }
    };
    match serde_json::from_str(&raw) {
        Ok(identity) => Some(identity),
        Err(error) => {
            crate::logging::warn(&format!(
                "Failed to parse socket identity at {}: {}",
                path.display(),
                error
            ));
            None
        }
    }
}

/// Provider/model identity a daemon publishes beside its socket.
///
/// Re-exported from this module so a client outside this crate can read the
/// sidecar without going through the server registry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SocketIdentity {
    /// The provider the daemon actually resolved to, e.g. "cursor" or
    /// "openai-compatible". This is the *resolved* identity, never the launch
    /// flag: a daemon started with `--provider auto` can resolve to anything.
    pub provider: String,
    /// The model the daemon actually resolved to, when it has one.
    #[serde(default)]
    pub model: Option<String>,
}

/// Publish this daemon's resolved identity beside its socket.
pub fn write_socket_identity(
    socket: &std::path::Path,
    identity: &SocketIdentity,
) -> std::io::Result<()> {
    let path = identity_file_for_socket(socket);
    std::fs::write(path, serde_json::to_vec_pretty(identity)?)
}

/// Publish the identity this daemon actually resolved to, beside its socket.
///
/// Written from the live provider, never from the `--provider` launch flag: a
/// daemon started with `--provider auto` can resolve to anything, and publishing
/// the flag is exactly what let a stale daemon masquerade as a match for a
/// client that asked for a specific provider.
///
/// `KCODE_RUNTIME_PROVIDER` already holds the canonical runtime key
/// (`activation.rs` writes `RuntimeProviderId::key()` there), so it is
/// preferred; the provider's own display name is a human label
/// (`Provider::display_name()` - "NVIDIA NIM", "OpenRouter") that diverges from
/// both `as_arg_value()` and the runtime key, so it is only a last resort.
pub fn publish_daemon_identity(socket: &std::path::Path, provider: &dyn crate::provider::Provider) {
    let runtime_provider =
        std::env::var("KCODE_RUNTIME_PROVIDER").unwrap_or_else(|_| provider.display_name());
    let identity = SocketIdentity {
        provider: runtime_provider,
        model: Some(provider.model()),
    };
    if let Err(error) = write_socket_identity(socket, &identity) {
        crate::logging::warn(&format!(
            "Failed to publish socket identity at {}: {}",
            identity_file_for_socket(socket).display(),
            error
        ));
    }
}

/// Resolve a socket path to a comparable identity: absolute, with its parent
/// directory's symlinks collapsed.
///
/// `KCODE_SOCKET` carries a user-supplied *spelling*, and several spellings
/// name the same socket: `jcode.sock` relative to the runtime dir, a path
/// through a symlinked parent, a `..` segment. Comparing spellings would call
/// those "custom" and hand the shared socket to a process-scoped server that
/// then takes it down with the run, disconnecting every other client.
///
/// Only the *parent* is resolved; the final component is kept as written, even
/// when it currently exists as a symlink to another socket. Binding always
/// unlinks and recreates that last component (see `Server::run`), so the
/// socket a server ends up owning is the one named here: not whatever the
/// name happened to point at beforehand. Following the final link would pin
/// the daemon lock to the old target and then publish a different socket,
/// locking a path nobody is listening on (review of #1768, finding 2).
///
/// Falls back to the plain absolute path when the parent is absent too: two
/// absent paths still compare consistently with each other.
fn resolved_socket_identity(path: &std::path::Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };

    match (absolute.parent(), absolute.file_name()) {
        (Some(parent), Some(file_name)) => match std::fs::canonicalize(parent) {
            Ok(canonical_parent) => canonical_parent.join(file_name),
            Err(_) => absolute,
        },
        _ => absolute,
    }
}

/// True when `path` is the shared daemon socket for this runtime dir, however
/// it happens to be spelled.
/// Bind the daemon's main socket and publish its identity beside it.
///
/// The identity is written as part of preparing the socket: a client that
/// finds the socket must be able to learn who is serving it.
pub fn bind_and_publish(
    socket: &std::path::Path,
    provider: &dyn crate::provider::Provider,
) -> std::io::Result<crate::transport::Listener> {
    let listener = crate::transport::Listener::bind(socket)?;
    publish_daemon_identity(socket, provider);
    Ok(listener)
}

pub fn is_shared_socket(path: &std::path::Path) -> bool {
    resolved_socket_identity(path) == resolved_socket_identity(&default_socket_path())
}

/// True when `--socket`/`KCODE_SOCKET` points somewhere other than the shared
/// daemon socket for this runtime dir.
///
/// A custom socket means "this process wants its own server", which is what
/// lets an isolated server be started for it without disturbing the shared
/// daemon (issue #1748).
pub fn socket_path_is_custom() -> bool {
    !is_shared_socket(&socket_path())
}

/// Debug socket path for testing/introspection
/// Derived from main socket path
pub fn debug_socket_path() -> PathBuf {
    let main_path = socket_path();
    let filename = main_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("kcode.sock");
    let debug_filename = filename.replace(".sock", "-debug.sock");
    main_path.with_file_name(debug_filename)
}

pub(super) fn sibling_socket_path(path: &std::path::Path) -> Option<PathBuf> {
    let filename = path.file_name()?.to_str()?;

    if let Some(base) = filename.strip_suffix("-debug.sock") {
        return Some(path.with_file_name(format!("{}.sock", base)));
    }

    if let Some(base) = filename.strip_suffix(".sock") {
        return Some(path.with_file_name(format!("{}-debug.sock", base)));
    }

    None
}

/// Remove a socket file and its sibling (main/debug) if present.
pub fn cleanup_socket_pair(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    if let Some(sibling) = sibling_socket_path(path) {
        let _ = std::fs::remove_file(sibling);
    }
}

/// Connect to a socket path.
///
/// Do not unlink the path on connection-refused here. A client-side cleanup can
/// strand a live daemon behind an unlinked Unix socket pathname, leaving the
/// process running with the daemon lock held while new clients can no longer
/// discover or connect to it.
pub async fn connect_socket(path: &std::path::Path) -> Result<Stream> {
    match Stream::connect(path).await {
        Ok(stream) => Ok(stream),
        Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused && path.exists() => {
            Err(anyhow::Error::new(err).context(format!(
                "Socket exists but refused the connection at {}. Retry, or remove it after confirming no jcode server is running.",
                path.display()
            )))
        }
        Err(err) if err.raw_os_error() == Some(libc::EMFILE) => Err(anyhow::anyhow!(
            "{} ({})",
            err,
            crate::util::process_fd_diagnostic_snapshot()
        )),
        Err(err) => Err(err.into()),
    }
}

pub(super) async fn socket_has_live_listener(path: &std::path::Path) -> bool {
    #[cfg(windows)]
    {
        // `is_socket_path` performs one non-blocking named-pipe open and treats
        // ERROR_PIPE_BUSY as live. Do not follow it with a second connect: the
        // first probe can temporarily occupy the only published pipe instance
        // before the accept loop replaces it, making that second connect wait
        // forever inside the Windows ERROR_PIPE_BUSY retry loop.
        crate::transport::is_socket_path(path)
    }

    #[cfg(not(windows))]
    {
        crate::transport::is_socket_path(path) && Stream::connect(path).await.is_ok()
    }
}

/// Reap a provably-stale socket left behind by a dead daemon.
///
/// Background: after an upgrade or crash, the runtime socket file can survive
/// even though the daemon that owned it is gone. A client that finds the socket
/// path present but cannot connect/handshake gets wedged into a connect-retry
/// loop and never recovers on its own (see issues #277 and #291).
///
/// Safety: a live daemon holds an exclusive `flock` on `jcode-daemon.lock` for
/// its entire lifetime. So if (a) the socket has no live listener AND (b) we can
/// acquire that exclusive lock, then no daemon is running and the socket is
/// provably stale. Only then do we unlink the socket pair (main + debug) and the
/// lock file. This can never strand a live daemon, because a live daemon would
/// either still be answering on the socket or still be holding the lock.
///
/// Returns true if a stale socket was reaped.
#[cfg(unix)]
pub async fn reap_stale_socket_if_dead(path: &std::path::Path) -> bool {
    // Nothing to reap if the path isn't even present.
    if !crate::transport::is_socket_path(path) {
        return false;
    }

    // If a listener answers, the daemon is alive; never touch it.
    if socket_has_live_listener(path).await {
        return false;
    }

    // Try to grab the daemon lock. If a live daemon holds it, this fails and we
    // leave everything alone (the daemon may just be slow to answer).
    let lock_path = daemon_lock_path();
    let Ok(Some(_lock)) = try_acquire_daemon_lock(&lock_path) else {
        return false;
    };

    // Re-check the listener now that we hold the lock, to close the race where a
    // daemon bound the socket between our probe and acquiring the lock. (A real
    // daemon could not have given us the lock, but a brand-new daemon spawned in
    // the gap might be mid-startup before it acquired the lock itself.)
    if socket_has_live_listener(path).await {
        return false;
    }

    crate::logging::warn(&format!(
        "Reaping stale jcode socket with no live listener at {}",
        path.display()
    ));
    cleanup_socket_pair(path);
    // `_lock` (a DaemonLockGuard) removes the lock file when it drops at the end
    // of this scope, so the leftover lock is cleaned up too.
    true
}

#[cfg(not(unix))]
pub async fn reap_stale_socket_if_dead(_path: &std::path::Path) -> bool {
    // Windows named pipes do not leave filesystem socket nodes behind after a
    // process exits, so there is no stale artifact to reap. Probing and then
    // "cleaning" the pipe only consumes a live server instance temporarily.
    false
}

/// Return true if a live server process is listening on the socket path.
///
/// This is intentionally weaker than [`is_server_ready`]: a live listener may
/// still be finishing startup or be temporarily too busy to answer a ping
/// within the short readiness timeout. Callers that must avoid spawning a
/// duplicate daemon should prefer this check over a ping-only probe.
pub async fn has_live_listener(path: &std::path::Path) -> bool {
    socket_has_live_listener(path).await
}

#[cfg(unix)]
pub(super) fn daemon_lock_path() -> PathBuf {
    crate::storage::runtime_dir().join("kcode-daemon.lock")
}

#[cfg(unix)]
pub(super) struct DaemonLockGuard {
    _file: std::fs::File,
    path: PathBuf,
}

#[cfg(unix)]
impl Drop for DaemonLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
pub(super) fn try_acquire_daemon_lock(path: &std::path::Path) -> Result<Option<DaemonLockGuard>> {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    let fd = file.as_raw_fd();
    let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
    if ret == 0 {
        Ok(Some(DaemonLockGuard {
            _file: file,
            path: path.to_path_buf(),
        }))
    } else {
        Ok(None)
    }
}

#[cfg(unix)]
pub(super) fn acquire_daemon_lock() -> Result<DaemonLockGuard> {
    let path = daemon_lock_path();
    try_acquire_daemon_lock(&path)?.ok_or_else(|| {
        anyhow::anyhow!(
            "Another jcode server process is already running for runtime dir {}",
            crate::storage::runtime_dir().display()
        )
    })
}

#[cfg(unix)]
pub(super) fn mark_close_on_exec<T: std::os::fd::AsRawFd>(io: &T) {
    let fd = io.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags >= 0 {
        let _ = unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) };
    }
}

pub fn set_socket_path(path: &str) {
    crate::env::set_var("KCODE_SOCKET", path);
}

/// Parent-process identity variables that must never be inherited by a server
/// process.
///
/// A CLI client resolves its provider *before* it spawns the daemon, and that
/// resolution writes `KCODE_RUNTIME_PROVIDER` / `KCODE_ACTIVE_PROVIDER` (plus
/// the resuming/session flags) into its own environment. Because
/// [`std::process::Command`] inherits the parent environment by default, those
/// values leak into the daemon, whose own provider init then reads them as if
/// they were its own resolved identity. The result is a session stamped with a
/// provider that is not actually serving it — e.g. `provider_key = "openai"`
/// while the live provider serves an OpenAI-compatible model.
///
/// Genuine daemon configuration (`KCODE_OPENROUTER_*`, `KCODE_SCRATCH_DIR`,
/// `KCODE_DEFERRED_AUTH_BOOTSTRAP`, `KCODE_RELOAD_FAST_START`, `KCODE_READY_FD`,
/// `KCODE_RELOAD_AUTH_STATUS`) is deliberately left alone: the daemon needs it
/// and it does not describe a competing provider identity.
pub const INHERITED_PROVIDER_IDENTITY_VARS: &[&str] = &[
    "KCODE_RUNTIME_PROVIDER",
    "KCODE_ACTIVE_PROVIDER",
    "KCODE_INITIAL_PROVIDER_EXPLICIT",
    "KCODE_RESUMING",
    "KCODE_SESSION_ID",
];

/// Strip the parent client's provider identity from a server child command.
///
/// Applied on both spawn paths — the initial `spawn_server_with_lifetime`
/// handoff and the reload `exec` — so no daemon generation inherits the
/// previous process's identity. The replacement daemon re-derives its provider
/// from configuration and auto-detection instead.
pub fn scrub_inherited_provider_identity(cmd: &mut std::process::Command) {
    for var in INHERITED_PROVIDER_IDENTITY_VARS {
        cmd.env_remove(var);
    }
}

/// Spawn a server child process and wait until it signals readiness.
///
/// Creates an anonymous pipe, passes the write-end fd to the child via
/// `KCODE_READY_FD`, and awaits a single byte on the read end. The server
/// calls `signal_ready_fd()` once its accept loops are spawned, so the future
/// resolves only after the daemon can start servicing client requests.
///
/// Falls back to a short poll loop if the pipe read times out (e.g. server
/// built without ready-fd support, or crash before bind).
#[cfg(unix)]
pub async fn spawn_server_notify(cmd: &mut std::process::Command) -> Result<std::process::Child> {
    use std::os::unix::io::FromRawFd;
    use std::os::unix::process::CommandExt;

    // Create a pipe: fds[0] = read end, fds[1] = write end.
    // Use pipe2 with O_CLOEXEC on the read end (parent keeps it).
    // The write end needs CLOEXEC cleared so it survives exec in the child.
    let mut fds = [0i32; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        anyhow::bail!("pipe() failed: {}", std::io::Error::last_os_error());
    }
    let read_fd = fds[0];
    let write_fd = fds[1];

    // Set CLOEXEC on the read end (parent only)
    unsafe {
        let flags = libc::fcntl(read_fd, libc::F_GETFD);
        if flags >= 0 {
            libc::fcntl(read_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
        }
    }

    // Pass the write-end fd to the child and tell it the fd number.
    unsafe {
        cmd.pre_exec(move || {
            // Clear CLOEXEC on the write end so it survives exec
            let flags = libc::fcntl(write_fd, libc::F_GETFD);
            if flags >= 0 {
                libc::fcntl(write_fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
            }
            libc::setsid();
            Ok(())
        });
    }
    cmd.env("KCODE_READY_FD", write_fd.to_string());

    let mut child = cmd.spawn()?;

    // Close our copy of the write end so we get EOF if the child dies.
    unsafe { libc::close(write_fd) };

    // Wait for the ready signal (or timeout / child death).
    let read_file = unsafe { std::fs::File::from_raw_fd(read_fd) };
    let mut async_file = tokio::fs::File::from_std(read_file);
    let mut buf = [0u8; 1];
    match tokio::time::timeout(
        Duration::from_secs(10),
        tokio::io::AsyncReadExt::read(&mut async_file, &mut buf),
    )
    .await
    {
        Ok(Ok(1)) => {
            crate::logging::info("Server signalled ready via pipe");
        }
        Ok(Ok(_)) => {
            if let Some(status) = child.try_wait()? {
                handle_server_start_exit(&mut child, status).await?;
            }
            crate::logging::info(
                "Server closed ready pipe without signalling; falling back to poll",
            );
            wait_for_server_ready(&socket_path(), Duration::from_secs(5)).await?;
        }
        Ok(Err(e)) => {
            crate::logging::info(&format!(
                "Ready pipe read error: {}; falling back to poll",
                e
            ));
            wait_for_server_ready(&socket_path(), Duration::from_secs(5)).await?;
        }
        Err(_) => {
            crate::logging::info("Timed out waiting for server ready signal; falling back to poll");
            wait_for_server_ready(&socket_path(), Duration::from_secs(5)).await?;
        }
    }

    if let Some(mut stderr) = child.stderr.take() {
        // The shared daemon outlives the spawning client. Keep draining the
        // stderr pipe after startup so later reloads cannot die on SIGPIPE
        // when they emit provider/model selection notices during boot.
        std::thread::spawn(move || {
            let mut sink = std::io::sink();
            let _ = std::io::copy(&mut stderr, &mut sink);
        });
    }

    Ok(child)
}

/// Wait until a server socket is connectable and responds to a ping.
pub async fn wait_for_server_ready(path: &std::path::Path, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(Ok(mut client)) = tokio::time::timeout(
            Duration::from_millis(250),
            Client::connect_with_path(path.to_path_buf()),
        )
        .await
            && let Ok(Ok(true)) =
                tokio::time::timeout(Duration::from_millis(250), client.ping()).await
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    anyhow::bail!(
        "Timed out waiting for responsive server socket {}",
        path.display()
    );
}

async fn probe_server_ready(path: &std::path::Path, ping_timeout: Duration) -> bool {
    let Ok(Ok(mut client)) =
        tokio::time::timeout(ping_timeout, Client::connect_with_path(path.to_path_buf())).await
    else {
        return false;
    };

    matches!(
        tokio::time::timeout(ping_timeout, client.ping()).await,
        Ok(Ok(true))
    )
}

pub async fn is_server_ready(path: &std::path::Path) -> bool {
    probe_server_ready(path, Duration::from_millis(50)).await
}

#[cfg(unix)]
pub(super) fn take_server_start_stderr(child: &mut std::process::Child) -> String {
    use std::io::Read;

    child
        .stderr
        .take()
        .and_then(|mut stderr| {
            let mut buf = String::new();
            stderr.read_to_string(&mut buf).ok()?;
            Some(buf)
        })
        .unwrap_or_default()
}

#[cfg(unix)]
pub(super) fn server_start_matches_existing_server(stderr_output: &str) -> bool {
    stderr_output.contains("Another jcode server process is already running")
        || stderr_output.contains("Refusing to replace active server socket")
}

#[cfg(any(unix, test))]
pub(super) async fn wait_for_existing_server(path: &std::path::Path, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if is_server_ready(path).await || has_live_listener(path).await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[cfg(unix)]
pub(super) fn format_server_start_error(
    status: std::process::ExitStatus,
    stderr_output: &str,
) -> String {
    if stderr_output.trim().is_empty() {
        format!(
            "Server exited before signalling ready ({}). Check logs at ~/.kcode/logs/",
            status
        )
    } else {
        format!(
            "Server exited before signalling ready ({}):\n{}",
            status,
            stderr_output.trim()
        )
    }
}

#[cfg(unix)]
pub(super) async fn handle_server_start_exit(
    child: &mut std::process::Child,
    status: std::process::ExitStatus,
) -> Result<()> {
    let stderr_output = take_server_start_stderr(child);
    if server_start_matches_existing_server(&stderr_output) {
        let socket_path = socket_path();
        if wait_for_existing_server(&socket_path, Duration::from_secs(5)).await {
            crate::logging::info(
                "Server spawn raced with an existing daemon; treating startup as successful",
            );
            return Ok(());
        }
    }

    anyhow::bail!(format_server_start_error(status, &stderr_output));
}

/// Write a single byte to the fd in `KCODE_READY_FD` and close it.
/// Called after startup plumbing is ready so the parent process knows the
/// server can accept and service client requests. The env var is cleared so child
/// processes (e.g. tool subprocesses) don't inherit a stale fd.
pub(super) fn signal_ready_fd() {
    #[cfg(unix)]
    {
        use std::os::unix::io::FromRawFd;

        if let Ok(fd_str) = std::env::var("KCODE_READY_FD") {
            crate::env::remove_var("KCODE_READY_FD");
            if let Ok(fd) = fd_str.parse::<i32>() {
                let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
                let _ = std::io::Write::write_all(&mut file, b"R");
                // file is dropped here which closes the fd
            }
        }
    }
}
