//! Cross-process advisory file locks for session persistence.
//!
//! `write_json_fast` already publishes a snapshot atomically (temp file plus
//! rename), so a reader never observes a half-written transcript. Atomic
//! rename does not serialize two *writers*: process A can read the journal
//! length, process B can checkpoint and delete that journal, and A's append
//! then lands after B's snapshot or A's snapshot silently discards B's newer
//! messages. This module supplies the missing mutual exclusion using std's
//! advisory file locks (`File::try_lock`, backed by `flock` on Unix and
//! `LockFileEx` on Windows) so no extra crate is needed.
//!
//! Contention policy: wait briefly for the current holder, then fail loudly.
//! Dropping a save silently would lose transcript messages; blocking forever
//! would wedge a turn loop behind another process. On timeout the error names
//! the holder recorded next to the lock file. The record lives in a separate
//! `<lock>.owner` file because Windows exclusive byte-range locks also deny
//! reads of the locked bytes, so the lock file itself is unreadable while held.
//! The record is diagnostic only: the kernel lock, not the contents, is what
//! excludes writers.

use std::fmt;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MIN_RETRY_DELAY: Duration = Duration::from_millis(2);
const MAX_RETRY_DELAY: Duration = Duration::from_millis(25);
/// Cap for the holder description echoed in an error; the file is written by
/// whoever holds the lock, so never trust it with unbounded output.
const HOLDER_INFO_LIMIT: usize = 200;

/// Why an exclusive lock could not be acquired.
#[derive(Debug)]
pub enum LockError {
    /// The lock is still held by someone else after `waited` elapsed.
    Timeout {
        lock_path: PathBuf,
        /// Holder description recorded by the owning process, when readable.
        holder: Option<String>,
        waited: Duration,
    },
    /// The lock file itself could not be opened or queried.
    Io {
        lock_path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout {
                lock_path,
                holder,
                waited,
            } => {
                write!(
                    f,
                    "timed out after {:.1}s waiting for session save lock {}",
                    waited.as_secs_f64(),
                    lock_path.display()
                )?;
                match holder {
                    Some(holder) => write!(f, " (held by {holder})"),
                    None => write!(f, " (held by another process)"),
                }
            }
            Self::Io { lock_path, source } => write!(
                f,
                "cannot open session save lock {}: {source}",
                lock_path.display()
            ),
        }
    }
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Timeout { .. } => None,
        }
    }
}

/// Holds an exclusive advisory lock for as long as the guard is alive.
///
/// Releasing on drop (rather than on an explicit call) means a panic or an
/// early return cannot leak the lock; closing the descriptor releases it even
/// if this process dies.
#[derive(Debug)]
pub struct FileLockGuard {
    file: File,
    path: PathBuf,
    owner_path: PathBuf,
}

impl FileLockGuard {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        // Remove the diagnostic record while still holding the lock, so a new
        // holder's record is never deleted by the previous holder.
        let _ = std::fs::remove_file(&self.owner_path);
        // Closing the handle releases the lock even if this fails.
        let _ = self.file.unlock();
    }
}

/// Acquire an exclusive lock on `lock_path`, creating it if needed.
///
/// Retries with a short backoff until `timeout` elapses. Concurrent writers in
/// the same process and in other processes both contend: `flock` and
/// `LockFileEx` locks belong to the open file description, not the thread.
pub fn lock_exclusive(lock_path: &Path, timeout: Duration) -> Result<FileLockGuard, LockError> {
    if let Some(parent) = lock_path.parent()
        && !parent.as_os_str().is_empty()
    {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|source| LockError::Io {
            lock_path: lock_path.to_path_buf(),
            source,
        })?;
    let owner_path = owner_path(lock_path);

    let start = Instant::now();
    let mut delay = MIN_RETRY_DELAY;
    loop {
        match file.try_lock() {
            Ok(()) => {
                record_holder(&owner_path);
                return Ok(FileLockGuard {
                    file,
                    path: lock_path.to_path_buf(),
                    owner_path,
                });
            }
            Err(TryLockError::WouldBlock) => {
                let waited = start.elapsed();
                if waited >= timeout {
                    return Err(LockError::Timeout {
                        lock_path: lock_path.to_path_buf(),
                        holder: read_holder(&owner_path),
                        waited,
                    });
                }
                std::thread::sleep(delay.min(timeout.saturating_sub(waited)));
                delay = (delay * 2).min(MAX_RETRY_DELAY);
            }
            Err(TryLockError::Error(source)) => {
                return Err(LockError::Io {
                    lock_path: lock_path.to_path_buf(),
                    source,
                });
            }
        }
    }
}

fn owner_path(lock_path: &Path) -> PathBuf {
    let mut name = lock_path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(".owner");
    lock_path.with_file_name(name)
}

/// Best-effort holder description for diagnostics. Failing to record it must
/// never fail the save: the kernel lock is already held at this point.
fn record_holder(owner_path: &Path) {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string());
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let line = format!(
        "pid={} host={} since_unix={since}",
        std::process::id(),
        host
    );
    let _ = std::fs::write(owner_path, format!("{line}\n"));
}

fn read_holder(owner_path: &Path) -> Option<String> {
    let mut raw = String::new();
    File::open(owner_path).ok()?.read_to_string(&mut raw).ok()?;
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .take(HOLDER_INFO_LIMIT)
        .collect();
    let cleaned = cleaned.trim();
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn second_writer_times_out_and_names_the_holder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session_x.lock");
        let held = lock_exclusive(&path, Duration::from_millis(50)).unwrap();

        let error = lock_exclusive(&path, Duration::from_millis(120)).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("timed out"), "{message}");
        assert!(message.contains("session_x.lock"), "{message}");
        let LockError::Timeout { holder, .. } = error else {
            panic!("expected timeout, got {error:?}");
        };
        let holder = holder.expect("holder record must be readable while locked");
        assert!(
            holder.contains(&format!("pid={}", std::process::id())),
            "{holder}"
        );
        drop(held);
    }

    #[test]
    fn lock_is_released_when_the_guard_drops() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session_release.lock");
        drop(lock_exclusive(&path, Duration::from_millis(50)).unwrap());
        lock_exclusive(&path, Duration::from_millis(50))
            .expect("released lock must be acquirable again");
    }

    #[test]
    fn waiter_acquires_after_the_holder_releases() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session_wait.lock");
        let barrier = Arc::new(Barrier::new(2));
        let holder_path = path.clone();
        let holder_barrier = barrier.clone();
        let holder = std::thread::spawn(move || {
            let guard = lock_exclusive(&holder_path, Duration::from_millis(50)).unwrap();
            holder_barrier.wait();
            std::thread::sleep(Duration::from_millis(150));
            drop(guard);
        });
        barrier.wait();
        let started = Instant::now();
        let guard = lock_exclusive(&path, Duration::from_secs(5))
            .expect("waiter must acquire once the holder releases");
        assert!(started.elapsed() >= Duration::from_millis(100));
        drop(guard);
        holder.join().unwrap();
    }

    #[test]
    fn timeout_message_reports_unknown_holder_when_record_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session_empty.lock");
        let held = lock_exclusive(&path, Duration::from_millis(50)).unwrap();
        // Remove the record while the lock is held, as a crashed predecessor
        // that never wrote a record would leave it.
        std::fs::remove_file(super::owner_path(&path)).unwrap();
        let error = lock_exclusive(&path, Duration::from_millis(60)).unwrap_err();
        assert!(error.to_string().contains("held by another process"));
        drop(held);
    }
}
