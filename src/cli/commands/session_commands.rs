use super::super::args::SessionCommand;
use super::*;

pub fn run_session_rename_command(
    session_ref: &str,
    name: Option<&str>,
    clear: bool,
    json: bool,
) -> Result<()> {
    let resolved_id = session::find_session_by_name_or_id(session_ref)?;
    let mut session = session::Session::load(&resolved_id)?;

    if clear {
        session.rename_title(None);
    } else {
        let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) else {
            anyhow::bail!("Provide a session name or use --clear");
        };
        session.rename_title(Some(name.to_string()));
    }

    session.save()?;
    crate::tui::session_picker::invalidate_session_list_cache();

    let output = SessionRenameOutput {
        session_id: session.id.clone(),
        display_name: session.display_name().to_string(),
        title: session.display_title().map(ToOwned::to_owned),
        cleared: clear,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if clear {
        println!(
            "Cleared custom name for session {} ({}).",
            output.display_name, output.session_id
        );
    } else if let Some(title) = output.title.as_deref() {
        println!(
            "Renamed session {} ({}) to \"{}\".",
            output.display_name, output.session_id, title
        );
    }

    Ok(())
}

pub(crate) trait SessionImportRequest {
    fn id(&self) -> &str;
    fn preview(&self) -> bool;
}

pub(crate) fn import_jcode_session_command(args: &impl SessionImportRequest) -> Result<()> {
    if args.preview() {
        #[cfg(target_os = "macos")]
        eprintln!("KCode startup may have repaired its own code signature before this preview.");
        let preview = crate::storage::session_import::preview_jcode_session(args.id())?;
        println!(
            "Jcode session {} (source identity and future changes: unknown)",
            args.id()
        );
        println!("Source: local user profile .jcode/sessions; destination: KCode session storage.");
        println!("Point-in-time preview only; source and destination can change before import.");
        println!(
            "Metadata only: contents are not validated. Files read separately may be inconsistent while Jcode is active."
        );
        println!(
            "Import does not coordinate with source writers or protect against concurrent path replacement."
        );
        println!("Import is a one-time copy; later source changes are not synchronized.");
        for file in preview {
            println!(
                "{}: {} bytes; destination {}",
                file.name,
                file.size,
                if file.conflict {
                    "exists (import will refuse)"
                } else {
                    "absent"
                }
            );
        }
        return Ok(());
    }
    let imported = crate::storage::session_import::import_jcode_session(args.id())?;
    println!(
        "Imported {} session files into KCode (jcode source unchanged)",
        imported.len()
    );
    Ok(())
}

pub(crate) fn run_session_command(command: SessionCommand) -> Result<()> {
    match command {
        SessionCommand::ImportJcode { session } => import_jcode_session_command(&session),
        SessionCommand::Rename {
            session,
            name,
            clear,
            json,
        } => run_session_rename_command(&session, name.as_deref(), clear, json),
    }
}
