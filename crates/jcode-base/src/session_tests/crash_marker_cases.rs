use super::*;
use anyhow::{Result, anyhow};

/// Post-reboot PID reuse: the ownership marker names a PID that is alive, but in
/// a *different* boot. Before boot identity was recorded this session stayed
/// `Active` forever: never marked crashed, never offered as a resume hint.
#[test]
fn test_detect_crash_when_recorded_pid_is_alive_in_a_later_boot() -> Result<()> {
    let _env_lock = lock_env();
    let temp_home = crate::test_temp::tempdir_with_prefix("kcode-crash-reboot-reuse-")
        .map_err(|e| anyhow!(e))?;
    let _home = EnvVarGuard::set("KCODE_HOME", temp_home.path().as_os_str());
    let _test_flag = EnvVarGuard::set("KCODE_TEST_SESSION", "0");

    let mut session = Session::create_with_id(
        "session_reboot_reuse".to_string(),
        None,
        Some("boot".to_string()),
    );
    // Ownership marker written by this very process, in the current boot.
    session.mark_active();
    session.save()?;
    assert!(
        !session.detect_crash(),
        "a live owner in this boot is not a crash"
    );

    let dir = crate::storage::active_pids_dir().ok_or_else(|| anyhow!("no home"))?;
    let recorded_boot = crate::storage::boot_epoch_secs();
    let stale_boot = recorded_boot.map(|boot| boot - 86_400);
    std::fs::write(
        dir.join(&session.id),
        crate::storage::ActivePidMarker {
            pid: std::process::id(),
            boot_epoch_secs: stale_boot,
        }
        .render(),
    )?;

    if stale_boot.is_some() {
        assert!(
            session.detect_crash(),
            "an owner recorded in an earlier boot is a crash even while its PID is live"
        );
        assert!(matches!(session.status, SessionStatus::Crashed { .. }));
        assert!(
            !dir.join(&session.id).exists(),
            "marking crashed must clear the stale ownership marker"
        );
    }
    Ok(())
}

/// Backwards compatibility: a marker from a build that wrote a bare PID has no
/// boot to compare, so it keeps the old PID-liveness semantics and a live PID
/// still means "not crashed".
#[test]
fn test_detect_crash_legacy_pid_only_marker_stays_live() -> Result<()> {
    let _env_lock = lock_env();
    let temp_home = crate::test_temp::tempdir_with_prefix("kcode-crash-legacy-marker-")
        .map_err(|e| anyhow!(e))?;
    let _home = EnvVarGuard::set("KCODE_HOME", temp_home.path().as_os_str());
    let _test_flag = EnvVarGuard::set("KCODE_TEST_SESSION", "0");

    let mut session = Session::create_with_id(
        "session_legacy_marker".to_string(),
        None,
        Some("legacy".into()),
    );
    session.mark_active();
    session.save()?;

    let dir = crate::storage::active_pids_dir().ok_or_else(|| anyhow!("no home"))?;
    std::fs::write(dir.join(&session.id), std::process::id().to_string())?;

    assert!(
        !session.detect_crash(),
        "legacy marker must keep PID-only semantics"
    );
    Ok(())
}
