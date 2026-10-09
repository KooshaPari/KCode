//! Tracking of active session process IDs under `~/.kcode/active_pids`.
//!
//! This is pure filesystem state keyed by session ID, used to discover which
//! sessions are currently running (and to map a PID back to its session). It
//! lives in the storage crate because it only needs [`kcode_dir`] and is a
//! low-level concern shared by session management, dictation, and crash
//! recovery, none of which should pull the full `session` module into scope.

use crate::{kcode_dir, reject_dev_home_symlink_path};
use std::path::{Path, PathBuf};

fn write_marker(dir: &Path, session_id: &str, contents: &[u8]) -> std::io::Result<()> {
    if session_id.is_empty() || session_id == "." || session_id == ".." || session_id.contains('/')
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid session marker name",
        ));
    }
    let path = dir.join(session_id);
    if !path_allowed(&path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "unsafe session marker path",
        ));
    }
    std::fs::create_dir_all(dir)?;

    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::fs::OpenOptionsExt;

        let directory = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(dir)?;
        let name = CString::new(session_id)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
        write_marker_in_directory(&directory, &name, contents)
    }

    #[cfg(not(unix))]
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.write_all(contents)
    }
}

#[cfg(unix)]
pub(super) fn write_marker_in_directory(
    directory: &std::fs::File,
    name: &std::ffi::CStr,
    contents: &[u8],
) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd};

    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    file.write_all(contents)
}

fn path_allowed(path: &Path) -> bool {
    reject_dev_home_symlink_path(path).is_ok()
}

/// Directory holding one file per active session ID (`~/.kcode/active_pids`).
pub fn active_pids_dir() -> Option<PathBuf> {
    let path = kcode_dir().ok()?.join("active_pids");
    path_allowed(&path).then_some(path)
}

/// Directory holding per-session "currently streaming" markers. A marker file
/// exists only while a session is actively generating a model response. The
/// file content is the owning process PID so stale markers (from crashed
/// processes) can be detected and ignored.
pub fn streaming_pids_dir() -> Option<std::path::PathBuf> {
    let path = kcode_dir().ok()?.join("streaming_pids");
    path_allowed(&path).then_some(path)
}

/// Directory holding markers for internal sessions (debug/test sessions and
/// spawned children such as swarm workers). These sessions keep their normal
/// active-pid lifecycle markers, but presence UIs like the menu bar indicator
/// only want to show top-level sessions the user opened directly, so internal
/// ones are flagged here and filtered out of user-facing counts (issue #508).
pub fn internal_pids_dir() -> Option<std::path::PathBuf> {
    let path = kcode_dir().ok()?.join("internal_pids");
    path_allowed(&path).then_some(path)
}

/// Flag (or unflag) `session_id` as an internal session for presence UIs.
pub fn set_session_internal(session_id: &str, internal: bool) {
    let Some(dir) = internal_pids_dir() else {
        return;
    };
    let path = dir.join(session_id);
    if !path_allowed(&path) {
        return;
    }
    if internal {
        let _ = write_marker(&dir, session_id, b"");
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// Whether `session_id` is flagged as an internal session.
pub fn session_is_internal(session_id: &str) -> bool {
    internal_pids_dir().is_some_and(|dir| {
        let path = dir.join(session_id);
        path_allowed(&path) && path.exists()
    })
}

/// Record that `session_id` is owned by process `pid`.
pub fn register_active_pid(session_id: &str, pid: u32) {
    if let Some(dir) = active_pids_dir() {
        let _ = write_marker(&dir, session_id, pid.to_string().as_bytes());
    }
}

/// Remove the active-PID record for `session_id`, if present.
pub fn unregister_active_pid(session_id: &str) {
    if let Some(dir) = active_pids_dir() {
        let path = dir.join(session_id);
        if path_allowed(&path) {
            let _ = std::fs::remove_file(path);
        }
    }
    // A closed session is never streaming, and its internal flag is moot.
    unmark_streaming(session_id);
    set_session_internal(session_id, false);
}

/// Mark a session as actively streaming a model response.
pub fn mark_streaming(session_id: &str) {
    if let Some(dir) = streaming_pids_dir() {
        let _ = write_marker(&dir, session_id, std::process::id().to_string().as_bytes());
    }
}

/// Clear the streaming marker for a session (turn finished or interrupted).
pub fn unmark_streaming(session_id: &str) {
    if let Some(dir) = streaming_pids_dir() {
        let path = dir.join(session_id);
        if path_allowed(&path) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// RAII guard that marks a session as streaming for its lifetime and clears the
/// marker on drop. This guarantees the marker is cleared on every exit path
/// (normal return, `?` propagation, interrupt, or panic) so the menu bar count
/// never gets stuck showing a phantom streaming session.
pub struct StreamingGuard {
    session_id: String,
}

impl StreamingGuard {
    pub fn new(session_id: impl Into<String>) -> Self {
        let session_id = session_id.into();
        mark_streaming(&session_id);
        Self { session_id }
    }
}

impl Drop for StreamingGuard {
    fn drop(&mut self) {
        unmark_streaming(&self.session_id);
    }
}

/// Find the active session ID currently owned by the given process ID.
pub fn find_active_session_id_by_pid(pid: u32) -> Option<String> {
    let dir = active_pids_dir()?;
    for entry in std::fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        if !path_allowed(&entry.path()) {
            continue;
        }
        let session_id = entry.file_name().to_string_lossy().to_string();
        let stored = std::fs::read_to_string(entry.path()).ok()?;
        if stored.trim().parse::<u32>().ok()? == pid {
            return Some(session_id);
        }
    }
    None
}

/// List active session IDs currently tracked in `~/.kcode/active_pids`.
pub fn active_session_ids() -> Vec<String> {
    let Some(dir) = active_pids_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| path_allowed(&entry.path()))
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect()
}

#[cfg(unix)]
fn process_is_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn process_is_running(pid: u32) -> bool {
    // Best-effort fallback for platforms where this low-level storage crate does
    // not have a process API. The active PID file is still useful, and stale
    // entries are cleaned up by higher-level session lifecycle code.
    pid != 0
}

/// Live snapshot of how many kcode sessions are running, and how many of those
/// are actively streaming a model response right now. Used by the menu bar
/// indicator (`kcode menubar`) and any other presence UI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionCounts {
    /// Number of live sessions (registered PID is still running).
    pub total: usize,
    /// Number of live sessions currently streaming a model response.
    pub streaming: usize,
}

/// Live presence info for one running session, derived from the active-pid
/// registry and the streaming markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPresence {
    /// Session ID, e.g. `session_fox_1234567890_deadbeef`.
    pub session_id: String,
    /// PID of the process that owns the session.
    pub pid: u32,
    /// Whether the session is actively streaming a model response right now.
    pub streaming: bool,
    /// When the current streaming turn started (streaming marker mtime), if
    /// the session is streaming. Lets presence UIs show "working for 2m".
    pub streaming_since: Option<std::time::SystemTime>,
    /// Whether this is an internal session (debug/test or a spawned child
    /// such as a swarm worker) that user-facing presence UIs should hide.
    pub internal: bool,
}

/// Snapshot per-session presence by scanning the active-pid registry and
/// streaming markers, skipping any entries whose owning process is no longer
/// alive. This is a cheap O(n) scan over a handful of tiny files; used by the
/// menu bar indicator and other presence UI.
pub fn session_presence() -> Vec<SessionPresence> {
    let Some(active_dir) = active_pids_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&active_dir) else {
        return Vec::new();
    };

    let streaming_dir = streaming_pids_dir();
    let internal_dir = internal_pids_dir();
    let mut sessions = Vec::new();

    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if !path_allowed(&path) {
            continue;
        }
        let session_id = entry.file_name().to_string_lossy().to_string();
        let Some(pid) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| raw.trim().parse::<u32>().ok())
        else {
            continue;
        };
        if !process_is_running(pid) {
            continue;
        }

        let marker_path = streaming_dir.as_ref().map(|dir| dir.join(&session_id));
        let streaming = marker_path.as_ref().is_some_and(|marker| {
            path_allowed(marker)
                && std::fs::read_to_string(marker)
                    .ok()
                    .and_then(|raw| raw.trim().parse::<u32>().ok())
                    .is_some_and(process_is_running)
        });
        let streaming_since = if streaming {
            marker_path
                .as_ref()
                .filter(|marker| path_allowed(marker))
                .and_then(|marker| std::fs::metadata(marker).ok())
                .and_then(|meta| meta.modified().ok())
        } else {
            None
        };

        sessions.push(SessionPresence {
            internal: internal_dir.as_ref().is_some_and(|dir| {
                let path = dir.join(&session_id);
                path_allowed(&path) && path.exists()
            }),
            session_id,
            pid,
            streaming,
            streaming_since,
        });
    }

    sessions
}

/// Compute the current session counts from [`session_presence`].
pub fn session_counts() -> SessionCounts {
    let sessions = session_presence();
    SessionCounts {
        total: sessions.len(),
        streaming: sessions.iter().filter(|s| s.streaming).count(),
    }
}

/// [`session_presence`] filtered to sessions the user opened directly,
/// excluding internal debug/test sessions and spawned children such as swarm
/// workers (issue #508). Used by the menu bar indicator.
pub fn user_session_presence() -> Vec<SessionPresence> {
    session_presence()
        .into_iter()
        .filter(|s| !s.internal)
        .collect()
}

/// Session counts over [`user_session_presence`] only.
pub fn user_session_counts() -> SessionCounts {
    let sessions = user_session_presence();
    SessionCounts {
        total: sessions.len(),
        streaming: sessions.iter().filter(|s| s.streaming).count(),
    }
}

#[cfg(test)]
#[path = "active_pids_tests.rs"]
mod tests;
