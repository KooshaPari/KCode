//! Descriptor-anchored, one-way import of one selected legacy session.
//!
//! Both source and destination directory components are opened without following
//! symlinks. Source file descriptors are opened once with `O_NOFOLLOW`, then
//! copied into private staging files. `linkat` publishes without replacement,
//! so a path swap cannot redirect reads or overwrite an existing destination.

use anyhow::{Context, Result, bail};
use std::ffi::{CString, OsStr};
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

use super::DevNamespacePaths;

const FILE_SUFFIXES: [&str; 3] = ["json", "bak", "journal.jsonl"];
static STAGE_NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(super) fn import(paths: &DevNamespacePaths, session_id: &str) -> Result<Vec<PathBuf>> {
    validate_session_id(session_id)?;

    let user_home = paths.home.parent().context("invalid dev home path")?;
    let home_dir = open_directory_tree(user_home, false)?;
    let production_dir = open_child_directory(&home_dir, ".kcode", false)?;
    let source_dir = open_child_directory(&production_dir, "sessions", false)
        .context("legacy sessions directory does not exist or is unsafe")?;

    let dev_root = open_child_directory(&home_dir, ".kcode-dev", true)?;
    let destination_dir = open_child_directory(&dev_root, "sessions", true)?;

    let mut sources = Vec::new();
    for suffix in FILE_SUFFIXES {
        let name = format!("{session_id}.{suffix}");
        match open_regular_file_at(&source_dir, &name) {
            Ok(file) => sources.push((name, file)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("opening legacy session file {name}"));
            }
        }
    }
    if !sources.iter().any(|(name, _)| name.ends_with(".json")) {
        bail!("legacy session {session_id} has no JSON session record");
    }

    let (stage_name, stage_dir) = create_stage_directory(&destination_dir, session_id)?;

    let destination_path = paths.home.join("sessions");
    let result = stage_and_publish(&sources, &stage_dir, &destination_dir, &destination_path);
    cleanup_stage(&stage_dir, &destination_dir, &stage_name, &sources);
    result
}

fn create_stage_directory(destination_dir: &File, session_id: &str) -> Result<(String, File)> {
    let base = format!(".import-{session_id}-{}", std::process::id());
    for attempt in 0..128 {
        let stage_name = if attempt == 0 {
            base.clone()
        } else {
            let nonce = STAGE_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            format!("{base}-{nonce}")
        };
        let stage_name_c = c_string(OsStr::new(&stage_name))?;
        let created =
            unsafe { libc::mkdirat(destination_dir.as_raw_fd(), stage_name_c.as_ptr(), 0o700) };
        if created != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                continue;
            }
            return Err(error).context("creating isolated import staging directory");
        }
        match open_child_directory(destination_dir, &stage_name, false) {
            Ok(dir) => return Ok((stage_name, dir)),
            Err(error) => {
                unlink_at(destination_dir, &stage_name, libc::AT_REMOVEDIR);
                return Err(error);
            }
        }
    }
    bail!("could not allocate a unique isolated import staging directory")
}

fn validate_session_id(session_id: &str) -> Result<()> {
    if session_id.is_empty()
        || session_id.len() > 160
        || !session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!("invalid session ID for legacy import");
    }
    Ok(())
}

fn stage_and_publish(
    sources: &[(String, File)],
    stage_dir: &File,
    destination_dir: &File,
    destination_path: &Path,
) -> Result<Vec<PathBuf>> {
    for (name, input) in sources {
        let mut output = create_private_file_at(stage_dir, name)?;
        let mut input = input;
        std::io::copy(&mut input, &mut output)?;
        output.sync_all()?;
    }

    let mut published: Vec<String> = Vec::new();
    for (name, _) in sources {
        if let Err(error) = link_at(stage_dir, name, destination_dir, name) {
            for created in &published {
                unlink_at(destination_dir, created, 0);
            }
            return Err(error).context("publishing imported session files without overwrite");
        }
        published.push(name.clone());
    }

    Ok(sources
        .iter()
        .map(|(name, _)| destination_path.join(name))
        .collect())
}

fn cleanup_stage(
    stage_dir: &File,
    destination_dir: &File,
    stage_name: &str,
    sources: &[(String, File)],
) {
    for (name, _) in sources {
        unlink_at(stage_dir, name, 0);
    }
    unlink_at(destination_dir, stage_name, libc::AT_REMOVEDIR);
}

fn open_directory_tree(path: &Path, create_missing: bool) -> Result<File> {
    if !path.is_absolute() {
        bail!("import directory must be absolute: {}", path.display());
    }
    let root = unsafe { libc::open(c"/".as_ptr(), directory_flags()) };
    if root < 0 {
        return Err(std::io::Error::last_os_error()).context("opening filesystem root");
    }
    let mut current = unsafe { File::from_raw_fd(root) };
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => continue,
            Component::Normal(name) => {
                current = open_child_directory(&current, name, create_missing)?;
            }
            Component::Prefix(_) | Component::ParentDir => {
                bail!("unsafe import directory path: {}", path.display());
            }
        }
    }
    Ok(current)
}

fn open_child_directory(parent: &File, name: impl AsRef<OsStr>, create: bool) -> Result<File> {
    let name = c_string(name.as_ref())?;
    let mut fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), directory_flags()) };
    if fd < 0 && create && std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound {
        let made = unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) };
        if made != 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
        {
            return Err(std::io::Error::last_os_error().into());
        }
        fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), directory_flags()) };
    }
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn directory_flags() -> libc::c_int {
    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC
}

fn open_regular_file_at(parent: &File, name: &str) -> std::io::Result<File> {
    let name = CString::new(name).map_err(std::io::Error::other)?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let file = unsafe { File::from_raw_fd(fd) };
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "legacy session entry is not a regular file",
        ));
    }
    Ok(file)
}

fn create_private_file_at(parent: &File, name: &str) -> Result<File> {
    let name = c_string(OsStr::new(name))?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error()).context("creating private staged file");
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn link_at(
    source: &File,
    source_name: &str,
    destination: &File,
    destination_name: &str,
) -> Result<()> {
    let source_name = c_string(OsStr::new(source_name))?;
    let destination_name = c_string(OsStr::new(destination_name))?;
    let result = unsafe {
        libc::linkat(
            source.as_raw_fd(),
            source_name.as_ptr(),
            destination.as_raw_fd(),
            destination_name.as_ptr(),
            0,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error()).context("linking imported file");
    }
    Ok(())
}

fn unlink_at(parent: &File, name: &str, flags: libc::c_int) {
    if let Ok(name) = CString::new(name) {
        unsafe {
            libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), flags);
        }
    }
}

fn c_string(name: &OsStr) -> Result<CString> {
    CString::new(name.as_bytes()).context("import path contains a NUL byte")
}
