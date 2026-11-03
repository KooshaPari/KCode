//! Durable, idempotent acceptance of externally requested session wakes.
//!
//! A queued receipt is deliberately not an assertion that a turn consumed it.
//! Callers must keep it pending until a separate turn-entry proof is recorded.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum WakeState {
    Queued,
    Awakened,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct WakeReceipt {
    pub receipt_id: String,
    pub session_id: String,
    pub message: String,
    pub state: WakeState,
}

fn receipt_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn path() -> Result<PathBuf> {
    Ok(crate::storage::jcode_dir()?.join("native-wake-receipts.json"))
}

fn load() -> Result<BTreeMap<String, WakeReceipt>> {
    let path = path()?;
    if path.exists() {
        crate::storage::read_json(&path)
    } else {
        // A Windows write can be interrupted after moving the primary to
        // .bak and before installing the new file. Absence is only an empty
        // store when neither generation exists.
        let backup = path.with_extension("bak");
        if backup.exists() {
            crate::storage::read_json(&backup)
        } else {
            Ok(BTreeMap::new())
        }
    }
}

fn save(receipts: &BTreeMap<String, WakeReceipt>) -> Result<()> {
    crate::storage::write_json(&path()?, receipts)
}

fn validate(receipt_id: &str, session_id: &str, message: &str) -> Result<()> {
    if receipt_id.is_empty()
        || receipt_id.len() > 128
        || !receipt_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
    {
        bail!("receipt_id must be 1-128 ASCII letters, digits, dash, underscore, dot, or colon");
    }
    if session_id.is_empty()
        || session_id.len() > 128
        || !session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
    {
        bail!("invalid target session_id");
    }
    if message.trim().is_empty() || message.len() > 64 * 1024 {
        bail!("wake message must be nonempty and at most 64 KiB");
    }
    Ok(())
}

/// Native bridge writes are opt-in and scoped to exact existing session IDs.
/// The daemon reads this from its own environment, never from request data.
pub(super) fn target_is_allowed(session_id: &str) -> bool {
    std::env::var("JCODE_NATIVE_WAKE_SESSION_ALLOWLIST")
        .ok()
        .is_some_and(|list| list.split(',').map(str::trim).any(|id| id == session_id))
}

/// Returns the existing record on retry; a reused key with changed payload fails closed.
pub(super) fn submit(receipt_id: &str, session_id: &str, message: &str) -> Result<WakeReceipt> {
    validate(receipt_id, session_id, message)?;
    let _guard = receipt_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut receipts = load()?;
    if let Some(existing) = receipts.get(receipt_id) {
        if existing.session_id != session_id || existing.message != message {
            bail!("receipt_id already belongs to another wake payload");
        }
        return Ok(existing.clone());
    }
    let receipt = WakeReceipt {
        receipt_id: receipt_id.to_string(),
        session_id: session_id.to_string(),
        message: message.to_string(),
        state: WakeState::Queued,
    };
    receipts.insert(receipt_id.to_string(), receipt.clone());
    save(&receipts)?;
    Ok(receipt)
}

pub(super) fn get(receipt_id: &str) -> Result<Option<WakeReceipt>> {
    let _guard = receipt_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Ok(load()?.get(receipt_id).cloned())
}

/// Called only after the exact receipt's content has been saved in the target transcript.
pub(crate) fn mark_awakened(receipt_id: &str, session_id: &str) -> Result<()> {
    let _guard = receipt_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut receipts = load()?;
    let Some(receipt) = receipts.get_mut(receipt_id) else {
        bail!("unknown receipt_id");
    };
    if receipt.session_id != session_id {
        bail!("receipt target mismatch");
    }
    receipt.state = WakeState::Awakened;
    save(&receipts)
}

pub(super) fn status_event(id: u64, receipt: WakeReceipt) -> crate::protocol::ServerEvent {
    crate::protocol::ServerEvent::WakeReceipt {
        id,
        receipt_id: receipt.receipt_id,
        session_id: receipt.session_id,
        status: match receipt.state {
            WakeState::Queued => "queued",
            WakeState::Awakened => "awakened",
        }
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_receipt_survives_reload_and_retries_without_duplicate() {
        let _guard = crate::storage::lock_test_env();
        let temp = tempfile::TempDir::new().expect("temp dir");
        let old_home = std::env::var_os("JCODE_HOME");
        let old_allowlist = std::env::var_os("JCODE_NATIVE_WAKE_SESSION_ALLOWLIST");
        crate::env::set_var("JCODE_HOME", temp.path());
        crate::env::set_var(
            "JCODE_NATIVE_WAKE_SESSION_ALLOWLIST",
            "ses_target,ses_second",
        );
        assert!(target_is_allowed("ses_target"));
        assert!(!target_is_allowed("ses_target_extra"));

        let first = submit("discord:event:1", "ses_target", "wake now").expect("submit");
        assert_eq!(first.state, WakeState::Queued);
        assert_eq!(get("discord:event:1").expect("reload"), Some(first.clone()));
        assert_eq!(
            submit("discord:event:1", "ses_target", "wake now")
                .expect("retry")
                .state,
            WakeState::Queued
        );
        assert!(submit("discord:event:1", "ses_other", "wake now").is_err());
        assert!(submit("discord:event:1", "ses_target", "changed").is_err());
        assert!(submit("../escape", "ses_target", "wake now").is_err());

        mark_awakened("discord:event:1", "ses_target").expect("transcript committed");
        assert_eq!(
            submit("discord:event:1", "ses_target", "wake now")
                .expect("retry after wake")
                .state,
            WakeState::Awakened
        );

        if let Some(old_home) = old_home {
            crate::env::set_var("JCODE_HOME", old_home);
        } else {
            crate::env::remove_var("JCODE_HOME");
        }
        if let Some(old_allowlist) = old_allowlist {
            crate::env::set_var("JCODE_NATIVE_WAKE_SESSION_ALLOWLIST", old_allowlist);
        } else {
            crate::env::remove_var("JCODE_NATIVE_WAKE_SESSION_ALLOWLIST");
        }
    }

    #[test]
    fn interrupted_replace_recovers_backup_without_erasing_history() {
        let _guard = crate::storage::lock_test_env();
        let temp = tempfile::TempDir::new().expect("temp dir");
        let old_home = std::env::var_os("JCODE_HOME");
        crate::env::set_var("JCODE_HOME", temp.path());

        let first = submit("bridge:event:old", "ses_target", "first").expect("submit");
        let primary = path().expect("receipt path");
        let backup = primary.with_extension("bak");
        std::fs::rename(&primary, &backup).expect("simulate interrupted Windows replace");
        assert_eq!(
            get(&first.receipt_id).expect("recover absent primary"),
            Some(first)
        );
        submit("bridge:event:new", "ses_target", "second").expect("preserve recovered history");
        assert!(get("bridge:event:old").expect("reload").is_some());
        assert!(get("bridge:event:new").expect("reload").is_some());

        // A corrupt primary must recover the prior generation as well.
        std::fs::write(&primary, b"{").expect("simulate torn primary");
        assert!(
            get("bridge:event:old")
                .expect("recover corrupt primary")
                .is_some()
        );
        std::fs::write(&backup, b"{").expect("simulate corrupt backup");
        std::fs::write(&primary, b"{").expect("simulate corrupt primary again");
        assert!(get("bridge:event:old").is_err());
        assert!(submit("bridge:event:third", "ses_target", "third").is_err());

        if let Some(old_home) = old_home {
            crate::env::set_var("JCODE_HOME", old_home);
        } else {
            crate::env::remove_var("JCODE_HOME");
        }
    }
}
