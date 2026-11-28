use anyhow::Result;
use chrono::Utc;
use serde::Deserialize;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::cli::args::RestartCommand;

/// Dispatch a `jcode restart` subcommand. Kept next to the commands it selects so
/// the CLI entry point stays a single line per top-level command.
pub(crate) async fn run_restart_command(action: RestartCommand) -> Result<()> {
    match action {
        RestartCommand::Save {
            auto_restore,
            replace_armed,
        } => run_restart_save_command(auto_restore, replace_armed).await?,
        RestartCommand::Restore => run_restart_restore_command()?,
        RestartCommand::Status => run_restart_status_command()?,
        RestartCommand::Clear => run_restart_clear_command()?,
    }
    Ok(())
}

pub async fn run_restart_save_command(auto_restore: bool, replace_armed: bool) -> Result<()> {
    run_restart_save_command_with_writer(auto_restore, replace_armed, &mut std::io::stdout()).await
}

async fn run_restart_save_command_with_writer<W: std::io::Write>(
    auto_restore: bool,
    replace_armed: bool,
    output: &mut W,
) -> Result<()> {
    let path = crate::restart_snapshot::snapshot_path()?;
    let existing = existing_snapshot_for_save(&path)?;
    if !replace_armed {
        // The restore-side claim that serializes competing restores belongs to #389.
        if let Some(snapshot) = existing
            .as_ref()
            .filter(|snapshot| snapshot.auto_restore_on_next_start)
        {
            anyhow::bail!(
                "Refusing to overwrite armed reboot snapshot at {}: created {}, {} session(s), auto-restore armed. Run `jcode restart restore` to consume it, or use `jcode restart save --replace-armed` to replace it.",
                path.display(),
                snapshot.created_at,
                snapshot.sessions.len()
            );
        }
    }

    let mut snapshot = if let Some(snapshot) = capture_connected_restart_snapshot().await? {
        snapshot
    } else {
        crate::restart_snapshot::capture_current_snapshot()?
    };
    snapshot.auto_restore_on_next_start = auto_restore && !snapshot.sessions.is_empty();
    crate::restart_snapshot::write_snapshot(&snapshot)?;

    if snapshot.sessions.is_empty() {
        writeln!(output, "Saved empty reboot snapshot to {}", path.display())?;
        if auto_restore {
            writeln!(
                output,
                "Automatic restore was not armed because no sessions were captured."
            )?;
        }
        writeln!(output, "\nNo active jcode windows were detected.")?;
        return Ok(());
    }

    writeln!(
        output,
        "Saved reboot snapshot with {} session(s) to {}{}\n",
        snapshot.sessions.len(),
        path.display(),
        if let Some(existing) = &existing {
            format!(
                " (replaced unarmed snapshot created {} with {} session(s))",
                existing.created_at,
                existing.sessions.len()
            )
        } else {
            String::new()
        }
    )?;
    for session in &snapshot.sessions {
        let suffix = if session.is_selfdev {
            " [self-dev]"
        } else {
            ""
        };
        writeln!(
            output,
            "- {} ({}){}",
            session.display_name, session.session_id, suffix
        )?;
    }
    if auto_restore {
        writeln!(
            output,
            "\nAutomatic restore is armed for the next plain `jcode` launch."
        )?;
    }
    writeln!(
        output,
        "\nAfter reboot, restore them with:\n  jcode restart restore"
    )?;

    Ok(())
}

fn existing_snapshot_for_save(
    path: &Path,
) -> Result<Option<crate::restart_snapshot::RestartSnapshot>> {
    match crate::restart_snapshot::load_snapshot() {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(error) => {
            let missing =
                path.try_exists().map_err(|error| {
                    anyhow::anyhow!(
                        "Failed to inspect reboot snapshot at {}: {}",
                        path.display(),
                        error
                    )
                })? || matches!(nested_io_error(&error), Some(std::io::ErrorKind::NotFound));
            if missing {
                Ok(None)
            } else {
                Err(anyhow::anyhow!(
                    "Failed to read existing reboot snapshot at {}: {}",
                    path.display(),
                    error
                ))
            }
        }
    }
}

fn nested_io_error(error: &anyhow::Error) -> Option<std::io::ErrorKind> {
    error.chain().find_map(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .map(|error| error.kind())
    })
}

pub fn run_restart_status_command() -> Result<()> {
    let path = crate::restart_snapshot::snapshot_path()?;
    let snapshot = match crate::restart_snapshot::load_snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => {
            println!("No reboot snapshot saved.\n\nCreate one with:\n  jcode restart save");
            return Ok(());
        }
    };

    println!(
        "Reboot snapshot: {}\nCreated: {}\nSessions: {}\nAuto-restore on next plain startup: {}\n",
        path.display(),
        snapshot.created_at,
        snapshot.sessions.len(),
        if snapshot.auto_restore_on_next_start {
            "armed"
        } else {
            "off"
        }
    );
    for session in &snapshot.sessions {
        let suffix = if session.is_selfdev {
            " [self-dev]"
        } else {
            ""
        };
        println!(
            "- {} ({}){}",
            session.display_name, session.session_id, suffix
        );
    }

    Ok(())
}

pub async fn maybe_run_pending_restart_restore_on_startup() -> Result<bool> {
    let snapshot = match crate::restart_snapshot::load_snapshot() {
        Ok(snapshot) => snapshot,
        // Do not synthesize an auto-restore snapshot from crashed sessions here.
        // A crashed session should remain crashed until the user explicitly resumes
        // or restores it, rather than being respawned by the next default startup.
        Err(_) => return Ok(false),
    };

    if snapshot.auto_restore_on_next_start {
        let _ = crate::restart_snapshot::set_auto_restore_on_next_start(false);
        println!(
            "Found a reboot snapshot with auto-restore enabled. Restoring {} jcode window(s)...\n",
            snapshot.sessions.len()
        );
        run_restart_restore_command()?;
        return Ok(true);
    }

    if std::io::stdin().is_terminal() || std::io::stderr().is_terminal() {
        println!("Saved reboot snapshot detected. Restore it with:\n  jcode restart restore\n");
    }

    Ok(false)
}

pub fn run_restart_clear_command() -> Result<()> {
    if crate::restart_snapshot::clear_snapshot()? {
        println!("Cleared reboot snapshot.");
    } else {
        println!("No reboot snapshot was saved.");
    }
    Ok(())
}

pub fn run_restart_restore_command() -> Result<()> {
    let exe = current_restart_restore_exe()?;
    let result = match crate::restart_snapshot::restore_snapshot(&exe) {
        Ok(result) => result,
        Err(error) => {
            let path = crate::restart_snapshot::snapshot_path()?;
            return Err(anyhow::anyhow!(
                "Failed to restore reboot snapshot at {}: {}",
                path.display(),
                error
            ));
        }
    };

    if result.snapshot.sessions.is_empty() {
        println!("Saved reboot snapshot is empty. Nothing to restore.");
        let _ = crate::restart_snapshot::clear_snapshot();
        return Ok(());
    }

    let launched = result
        .outcomes
        .iter()
        .filter(|outcome| outcome.launched)
        .count();
    let fallback = result.outcomes.len().saturating_sub(launched);

    if launched > 0 {
        println!("Restored {} jcode window(s).", launched);
    }

    if fallback > 0 {
        println!(
            "\n{} session(s) could not be opened automatically. Run these commands manually:\n",
            fallback
        );
        for outcome in result.outcomes.iter().filter(|outcome| !outcome.launched) {
            println!("# {}", outcome.session.display_name);
            println!("{}", outcome.command);
        }
        println!(
            "\nThe reboot snapshot was kept so you can try `jcode restart restore` again later."
        );
        return Ok(());
    }

    let _ = crate::restart_snapshot::clear_snapshot();
    println!("Cleared reboot snapshot after successful restore.");
    Ok(())
}

fn current_restart_restore_exe() -> Result<PathBuf> {
    crate::build::client_update_candidate(false)
        .map(|(path, _)| path)
        .or_else(|| std::env::current_exe().ok())
        .ok_or_else(|| anyhow::anyhow!("Could not determine jcode executable for restore"))
}

#[derive(Debug, Deserialize)]
struct ConnectedRestartSessionRow {
    session_id: String,
    #[serde(default)]
    working_dir: Option<String>,
}

async fn capture_connected_restart_snapshot()
-> Result<Option<crate::restart_snapshot::RestartSnapshot>> {
    let mut client = match crate::server::Client::connect_debug().await {
        Ok(client) => client,
        Err(_) => return Ok(None),
    };

    let request_id = client.debug_command("sessions", None).await?;
    let response = loop {
        match client.read_event().await? {
            crate::protocol::ServerEvent::DebugResponse { id, ok, output } if id == request_id => {
                if !ok {
                    anyhow::bail!(output);
                }
                break output;
            }
            crate::protocol::ServerEvent::Ack { id } if id == request_id => {}
            crate::protocol::ServerEvent::Done { id } if id == request_id => {}
            crate::protocol::ServerEvent::Error { id, message, .. } if id == request_id => {
                anyhow::bail!(message);
            }
            _ => {}
        }
    };

    let rows: Vec<ConnectedRestartSessionRow> = serde_json::from_str(&response)?;
    if rows.is_empty() {
        return Ok(Some(crate::restart_snapshot::RestartSnapshot {
            version: 1,
            created_at: Utc::now(),
            auto_restore_on_next_start: false,
            sessions: Vec::new(),
        }));
    }

    let mut seen = std::collections::HashSet::new();
    let mut sessions = Vec::new();
    for row in rows {
        if !seen.insert(row.session_id.clone()) {
            continue;
        }
        let Ok(mut session) = crate::session::Session::load(&row.session_id) else {
            continue;
        };
        if session.detect_crash() {
            let _ = session.save();
            continue;
        }
        sessions.push(crate::restart_snapshot::RestartSnapshotSession {
            session_id: session.id.clone(),
            display_name: session.display_name().to_string(),
            working_dir: session.working_dir.clone().or(row.working_dir),
            is_selfdev: session.is_canary,
        });
    }

    sessions.sort_by(|a, b| {
        a.display_name
            .cmp(&b.display_name)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });

    Ok(Some(crate::restart_snapshot::RestartSnapshot {
        version: 1,
        created_at: Utc::now(),
        auto_restore_on_next_start: false,
        sessions,
    }))
}

#[cfg(test)]
#[path = "restart_tests.rs"]
mod restart_tests;
