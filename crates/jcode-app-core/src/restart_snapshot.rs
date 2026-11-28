use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const AUTO_RESTORE_CRASH_MAX_AGE_HOURS: i64 = 24;
const RESTORE_CLAIM_STALE_MAX_AGE_SECS: i64 = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RestoreClaim {
    pid: u32,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RestoreClaimGuard {
    claim_path: PathBuf,
}

impl RestoreClaimGuard {
    fn release(self) -> Result<()> {
        release_claim_path(&self.claim_path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartSnapshot {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub auto_restore_on_next_start: bool,
    pub sessions: Vec<RestartSnapshotSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartSnapshotSession {
    pub session_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub is_selfdev: bool,
}

#[derive(Debug, Clone)]
pub struct RestoreLaunchOutcome {
    pub session: RestartSnapshotSession,
    pub launched: bool,
    pub command: String,
}

#[derive(Debug, Clone)]
pub struct RestoreSnapshotResult {
    pub snapshot: RestartSnapshot,
    pub outcomes: Vec<RestoreLaunchOutcome>,
}

pub fn snapshot_path() -> Result<PathBuf> {
    Ok(crate::storage::jcode_dir()?.join("restart-snapshot.json"))
}

pub fn restore_claim_path() -> Result<PathBuf> {
    Ok(crate::storage::jcode_dir()?.join("restart-snapshot.restore.claim.json"))
}

pub fn save_current_snapshot() -> Result<RestartSnapshot> {
    let snapshot = capture_current_snapshot()?;
    write_snapshot(&snapshot)?;
    Ok(snapshot)
}

pub fn write_snapshot(snapshot: &RestartSnapshot) -> Result<()> {
    crate::storage::write_json(&snapshot_path()?, snapshot)
}

pub fn load_snapshot() -> Result<RestartSnapshot> {
    crate::storage::read_json(&snapshot_path()?)
}

pub fn clear_snapshot() -> Result<bool> {
    let path = snapshot_path()?;
    if !path.exists() {
        return Ok(false);
    }
    crate::storage::remove_state_file(&path)?;
    Ok(true)
}

pub fn set_auto_restore_on_next_start(enabled: bool) -> Result<bool> {
    let mut snapshot = match load_snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => return Ok(false),
    };
    snapshot.auto_restore_on_next_start = enabled;
    write_snapshot(&snapshot)?;
    Ok(true)
}

pub fn arm_auto_restore_from_recent_crashes() -> Result<Option<RestartSnapshot>> {
    let cutoff = Utc::now() - chrono::Duration::hours(AUTO_RESTORE_CRASH_MAX_AGE_HOURS);
    let mut unique_ids = HashSet::new();
    let mut captured: Vec<(DateTime<Utc>, RestartSnapshotSession)> = Vec::new();

    for (session_id, _) in crate::session::find_recent_crashed_sessions() {
        if !unique_ids.insert(session_id.clone()) {
            continue;
        }

        let Ok(session) = crate::session::Session::load(&session_id) else {
            continue;
        };

        if !matches!(
            session.status,
            crate::session::SessionStatus::Crashed { .. }
        ) {
            continue;
        }

        let sort_key = session.last_active_at.unwrap_or(session.updated_at);
        if sort_key < cutoff {
            continue;
        }

        captured.push((
            sort_key,
            RestartSnapshotSession {
                session_id: session.id.clone(),
                display_name: session.display_name().to_string(),
                working_dir: session.working_dir.clone(),
                is_selfdev: session.is_canary,
            },
        ));
    }

    if captured.is_empty() {
        return Ok(None);
    }

    captured.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.display_name.cmp(&b.1.display_name))
            .then_with(|| a.1.session_id.cmp(&b.1.session_id))
    });

    let snapshot = RestartSnapshot {
        version: 1,
        created_at: Utc::now(),
        auto_restore_on_next_start: true,
        sessions: captured.into_iter().map(|(_, session)| session).collect(),
    };

    write_snapshot(&snapshot)?;
    Ok(Some(snapshot))
}

pub fn capture_current_snapshot() -> Result<RestartSnapshot> {
    let mut unique_ids = HashSet::new();
    let mut captured: Vec<(DateTime<Utc>, RestartSnapshotSession)> = Vec::new();

    for session_id in crate::storage::active_session_ids() {
        if !unique_ids.insert(session_id.clone()) {
            continue;
        }

        let Ok(mut session) = crate::session::Session::load(&session_id) else {
            continue;
        };

        if session.detect_crash() {
            let _ = session.save();
            continue;
        }

        if !matches!(session.status, crate::session::SessionStatus::Active) {
            continue;
        }

        let sort_key = session.last_active_at.unwrap_or(session.updated_at);
        captured.push((
            sort_key,
            RestartSnapshotSession {
                session_id: session.id.clone(),
                display_name: session.display_name().to_string(),
                working_dir: session.working_dir.clone(),
                is_selfdev: session.is_canary,
            },
        ));
    }

    captured.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.display_name.cmp(&b.1.display_name))
            .then_with(|| a.1.session_id.cmp(&b.1.session_id))
    });

    Ok(RestartSnapshot {
        version: 1,
        created_at: Utc::now(),
        auto_restore_on_next_start: false,
        sessions: captured.into_iter().map(|(_, session)| session).collect(),
    })
}

pub fn restore_snapshot(exe: &Path) -> Result<RestoreSnapshotResult> {
    let claim = try_acquire_restore_claim()?
        .ok_or_else(|| anyhow::anyhow!("reboot snapshot restore is already in progress"))?;
    restore_snapshot_with_claim(exe, claim)
}

pub fn try_restore_snapshot(exe: &Path) -> Result<Option<RestoreSnapshotResult>> {
    let Some(claim) = try_acquire_restore_claim()? else {
        return Ok(None);
    };
    restore_snapshot_with_claim(exe, claim).map(Some)
}

pub fn restore_snapshot_with_claim(
    exe: &Path,
    claim: RestoreClaimGuard,
) -> Result<RestoreSnapshotResult> {
    let result = (|| -> Result<RestoreSnapshotResult> {
        let snapshot = load_snapshot()?;
        let mut outcomes = Vec::new();

        for session in &snapshot.sessions {
            let cwd = resolve_session_cwd(session.working_dir.as_deref());
            let context = crate::session_launch::SessionSpawnContext::kind("restart");
            let launched = if session.is_selfdev {
                crate::session_launch::spawn_selfdev_in_new_terminal_with_context(
                    exe,
                    &session.session_id,
                    &cwd,
                    None,
                    &context,
                )?
            } else {
                crate::session_launch::spawn_resume_in_new_terminal_with_context(
                    exe,
                    &session.session_id,
                    &cwd,
                    None,
                    &context,
                )?
            };
            outcomes.push(RestoreLaunchOutcome {
                session: session.clone(),
                launched,
                command: restore_command_display(exe, session),
            });
        }

        Ok(RestoreSnapshotResult { snapshot, outcomes })
    })();

    if let Err(err) = claim.release() {
        let phase = if result.is_ok() {
            "after restore"
        } else {
            "after failed restore"
        };
        crate::logging::warn(&format!(
            "Failed to release reboot snapshot restore claim {}: {}",
            phase, err
        ));
    }

    result
}

fn resolve_session_cwd(configured: Option<&str>) -> PathBuf {
    configured
        .filter(|path| Path::new(path).is_dir())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().filter(|path| path.is_dir()))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn try_acquire_restore_claim() -> Result<Option<RestoreClaimGuard>> {
    let claim_path = restore_claim_path()?;
    if let Some(parent) = claim_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let claim = RestoreClaim {
        pid: std::process::id(),
        created_at: Utc::now(),
    };
    let payload = serde_json::to_vec(&claim)?;
    let mut claim_is_ours = false;

    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&claim_path)
        {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(&payload)?;
                file.sync_all()?;
                claim_is_ours = true;
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if !claim_is_stale(&claim_path)? {
                    return Ok(None);
                }
                if let Err(err) = crate::storage::remove_state_file(&claim_path) {
                    crate::logging::warn(&format!(
                        "Failed to remove stale reboot snapshot restore claim {}: {}",
                        claim_path.display(),
                        err
                    ));
                }
            }
            Err(err) => return Err(err.into()),
        }
    }

    if !claim_is_ours {
        return Ok(None);
    }

    if claim_is_stale(&claim_path)? {
        if let Err(err) = release_claim_path(&claim_path) {
            crate::logging::warn(&format!(
                "Failed to release stale reboot snapshot restore claim {}: {}",
                claim_path.display(),
                err
            ));
        }
        return Ok(None);
    }

    Ok(Some(RestoreClaimGuard { claim_path }))
}

fn claim_is_stale(claim_path: &Path) -> Result<bool> {
    let claim = match std::fs::read(claim_path) {
        Ok(bytes) => match serde_json::from_slice::<RestoreClaim>(&bytes) {
            Ok(claim) => claim,
            Err(_) => return Ok(true),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(err) => return Err(err.into()),
    };

    if claim.pid == std::process::id() {
        return Ok(false);
    }

    let age_secs = Utc::now()
        .signed_duration_since(claim.created_at)
        .num_seconds();
    let age_expired = age_secs >= RESTORE_CLAIM_STALE_MAX_AGE_SECS;
    let pid_dead = !crate::platform::is_process_running(claim.pid);
    Ok(pid_dead || age_expired)
}

fn release_claim_path(claim_path: &Path) -> Result<()> {
    match crate::storage::remove_state_file(claim_path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn shell_escape(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\"'\"'"))
}

pub fn restore_command_display(exe: &Path, session: &RestartSnapshotSession) -> String {
    let exe = shell_escape(exe.to_string_lossy().as_ref());
    if session.is_selfdev {
        format!("{} --resume {} self-dev", exe, session.session_id)
    } else {
        format!("{} --resume {}", exe, session.session_id)
    }
}

#[cfg(test)]
#[path = "restart_snapshot_tests.rs"]
mod restart_snapshot_tests;
