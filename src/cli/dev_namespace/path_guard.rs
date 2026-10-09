use anyhow::{Context, Result, bail};
use std::path::{Component, Path, PathBuf};

pub(super) fn paths_overlap(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

#[cfg(unix)]
pub(super) fn validate_existing_dev_home(path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() {
        bail!("kcode-dev root is not a directory: {}", path.display());
    }
    use std::os::unix::fs::MetadataExt;
    let expected_owner = unsafe { libc::geteuid() };
    if metadata.uid() != expected_owner {
        bail!(
            "kcode-dev root is not owned by the current user: {}",
            path.display()
        );
    }
    if metadata.mode() & 0o077 != 0 {
        bail!(
            "kcode-dev root permissions expose it to other users: {}",
            path.display()
        );
    }
    Ok(())
}

pub(super) fn reject_symlink_components(path: &Path) -> Result<()> {
    let mut cursor = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => cursor.push(prefix.as_os_str()),
            Component::RootDir => cursor.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => bail!("kcode-dev path contains '..': {}", path.display()),
            Component::Normal(part) => {
                cursor.push(part);
                if std::fs::symlink_metadata(&cursor)
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
                {
                    bail!("kcode-dev path traverses symlink {}", cursor.display());
                }
            }
        }
    }
    Ok(())
}

pub(super) fn reject_dynamic_symlinks(home: &Path) -> Result<()> {
    let entries = match std::fs::read_dir(home) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let dynamic_file = (name.starts_with("reload-context-") && name.ends_with(".json"))
            || name.starts_with("telemetry_milestone_")
            || name.starts_with("telemetry_active_days_")
            || name.starts_with("telemetry_session_starts_");
        if dynamic_file && entry.file_type()?.is_symlink() {
            bail!(
                "kcode-dev path traverses dynamic symlink {}",
                entry.path().display()
            );
        }
    }
    for relative in [
        "browser",
        "notifications",
        "Applications/KcodeNotificationBroker.app",
        "telemetry_active_sessions",
        "active_pids",
        "streaming_pids",
        "internal_pids",
        "reload-recovery",
        "run/durable-state",
        "run/kcode-swarm-state",
    ] {
        reject_symlink_tree(&home.join(relative))?;
    }
    Ok(())
}

fn reject_symlink_tree(path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        bail!("kcode-dev path traverses symlink {}", path.display());
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path)? {
            reject_symlink_tree(&entry?.path())?;
        }
    }
    Ok(())
}

/// Canonicalize existing ancestors while preserving a not-yet-created suffix.
pub(super) fn canonicalize_existing_prefix(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("kcode-dev path must be absolute: {}", path.display());
    }
    let mut ancestor = path;
    let mut suffix = Vec::new();
    while let Err(error) = std::fs::symlink_metadata(ancestor) {
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(error).with_context(|| {
                format!("inspect kcode-dev path component {}", ancestor.display())
            });
        }
        let name = ancestor
            .file_name()
            .context("kcode-dev path has no existing ancestor")?;
        suffix.push(name.to_os_string());
        ancestor = ancestor
            .parent()
            .context("kcode-dev path has no existing ancestor")?;
    }
    let mut resolved = std::fs::canonicalize(ancestor)?;
    for part in suffix.into_iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}

#[cfg(all(test, unix))]
mod tests {
    use super::canonicalize_existing_prefix;

    #[test]
    fn canonicalize_existing_prefix_rejects_dangling_symlink() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let dangling = temp.path().join("dangling");
        symlink(temp.path().join("missing-target"), &dangling).unwrap();

        assert!(canonicalize_existing_prefix(&dangling).is_err());
    }
}

pub(super) fn validate_optional_path_under(name: &str, root: &Path) -> Result<()> {
    let Some(value) = std::env::var_os(name).map(PathBuf::from) else {
        return Ok(());
    };
    if !value.is_absolute() || !canonicalize_existing_prefix(&value)?.starts_with(root) {
        bail!("unsafe kcode-dev {name} path: {}", value.display());
    }
    Ok(())
}

pub(super) fn validate_socket_path(socket: &Path, runtime: &Path) -> Result<()> {
    let socket = canonicalize_existing_prefix(socket)?;
    let runtime = canonicalize_existing_prefix(runtime)?;
    if !socket.starts_with(&runtime) {
        bail!(
            "kcode-dev socket {} is outside {}",
            socket.display(),
            runtime.display()
        );
    }
    Ok(())
}
