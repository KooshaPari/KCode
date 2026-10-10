//! Race-resistant removal of files beneath the active KCode state directory.

use std::path::Path;

/// Remove a private state file without following a directory swapped in after
/// the caller's path validation. Unix traversal stays anchored to a directory
/// descriptor and uses `unlinkat` for the final component.
pub fn remove_state_file(path: &Path) -> std::io::Result<()> {
    let home = crate::kcode_dir().map_err(std::io::Error::other)?;
    remove_file_beneath(&home, path)
}

/// Remove a durable state file without following a directory swapped in after
/// the caller's path validation. Durable state may live under the configured
/// runtime root rather than `KCODE_HOME`.
pub fn remove_durable_state_file(path: &Path) -> std::io::Result<()> {
    remove_file_beneath(&crate::durable_state_dir(), path)
}

fn remove_file_beneath(root: &Path, path: &Path) -> std::io::Result<()> {
    crate::reject_dev_home_symlink_path(path).map_err(std::io::Error::other)?;
    let relative = path.strip_prefix(root).map_err(std::io::Error::other)?;
    if relative.as_os_str().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "state path must name a file beneath its root",
        ));
    }

    #[cfg(unix)]
    {
        remove_unix(root, relative)
    }

    #[cfg(not(unix))]
    {
        std::fs::remove_file(path)
    }
}

#[cfg(unix)]
fn remove_unix(home: &Path, relative: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;

    let mut components = relative.components().peekable();
    let mut directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(home)?;

    while let Some(component) = components.next() {
        let std::path::Component::Normal(name) = component else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "state path must be relative and contain no dot components",
            ));
        };
        let name = CString::new(name.as_bytes())
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
        if components.peek().is_some() {
            let fd = unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            directory = unsafe { std::fs::File::from_raw_fd(fd) };
        } else {
            let result = unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) };
            if result < 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::remove_state_file;

    #[test]
    fn removes_a_state_file_beneath_the_configured_home() {
        let _lock = crate::lock_test_env();
        struct Restore(Vec<(&'static str, Option<std::ffi::OsString>)>);
        impl Drop for Restore {
            fn drop(&mut self) {
                // SAFETY: The storage test environment lock serializes tests
                // that read or modify process environment variables.
                unsafe {
                    for (name, value) in self.0.drain(..) {
                        match value {
                            Some(value) => std::env::set_var(name, value),
                            None => std::env::remove_var(name),
                        }
                    }
                }
            }
        }

        let home = tempfile::tempdir().unwrap();
        let _restore = Restore(
            ["KCODE_HOME", "KCODE_DEV_NAMESPACE"]
                .map(|name| (name, std::env::var_os(name)))
                .to_vec(),
        );
        // SAFETY: Test environment access is serialized by `lock_test_env`.
        unsafe {
            std::env::set_var("KCODE_HOME", home.path());
            std::env::remove_var("KCODE_DEV_NAMESPACE");
        }
        let parent = home.path().join("active_pids");
        std::fs::create_dir(&parent).unwrap();
        let state_file = parent.join("session-1");
        std::fs::write(&state_file, b"123").unwrap();

        remove_state_file(&state_file).unwrap();

        assert!(!state_file.exists());
    }

    #[test]
    fn removes_a_durable_state_file_beneath_the_runtime_root() {
        let _lock = crate::lock_test_env();
        struct Restore(Vec<(&'static str, Option<std::ffi::OsString>)>);
        impl Drop for Restore {
            fn drop(&mut self) {
                // SAFETY: The storage test environment lock serializes tests
                // that read or modify process environment variables.
                unsafe {
                    for (name, value) in self.0.drain(..) {
                        match value {
                            Some(value) => std::env::set_var(name, value),
                            None => std::env::remove_var(name),
                        }
                    }
                }
            }
        }

        let runtime = tempfile::tempdir().unwrap();
        let _restore = Restore(
            ["KCODE_RUNTIME_DIR", "KCODE_DEV_NAMESPACE"]
                .map(|name| (name, std::env::var_os(name)))
                .to_vec(),
        );
        // SAFETY: Test environment access is serialized by `lock_test_env`.
        unsafe {
            std::env::set_var("KCODE_RUNTIME_DIR", runtime.path());
            std::env::remove_var("KCODE_DEV_NAMESPACE");
        }
        let root = crate::durable_state_dir();
        let parent = root.join("swarm");
        std::fs::create_dir_all(&parent).unwrap();
        let state_file = parent.join("swarm-1.json");
        std::fs::write(&state_file, b"snapshot").unwrap();

        crate::remove_durable_state_file(&state_file).unwrap();

        assert!(!state_file.exists());
    }
}
