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

    // Normal user-configured homes may intentionally be symlinks. Resolve
    // that root before opening it with O_NOFOLLOW; dev homes are validated
    // above and must retain their no-follow boundary.
    let root =
        if std::env::var_os("KCODE_DEV_NAMESPACE").as_deref() == Some(std::ffi::OsStr::new("1")) {
            root.to_path_buf()
        } else {
            root.canonicalize()?
        };

    #[cfg(unix)]
    {
        remove_unix(&root, relative)
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

    #[cfg(unix)]
    #[test]
    fn removes_a_state_file_when_normal_home_is_a_symlink() {
        let _lock = crate::lock_test_env();
        struct Restore(Option<std::ffi::OsString>, Option<std::ffi::OsString>);
        impl Drop for Restore {
            fn drop(&mut self) {
                // SAFETY: The storage test environment lock serializes tests
                // that read or modify process environment variables.
                unsafe {
                    match self.0.take() {
                        Some(value) => std::env::set_var("KCODE_HOME", value),
                        None => std::env::remove_var("KCODE_HOME"),
                    }
                    match self.1.take() {
                        Some(value) => std::env::set_var("KCODE_DEV_NAMESPACE", value),
                        None => std::env::remove_var("KCODE_DEV_NAMESPACE"),
                    }
                }
            }
        }
        let target = tempfile::tempdir().unwrap();
        let parent = target.path().join("active_pids");
        std::fs::create_dir(&parent).unwrap();
        let state_file = parent.join("session-1");
        std::fs::write(&state_file, b"123").unwrap();
        let link_parent = tempfile::tempdir().unwrap();
        let home_link = link_parent.path().join("home");
        std::os::unix::fs::symlink(target.path(), &home_link).unwrap();
        let path_through_link = home_link.join("active_pids/session-1");

        let _restore = Restore(
            std::env::var_os("KCODE_HOME"),
            std::env::var_os("KCODE_DEV_NAMESPACE"),
        );
        // SAFETY: Test environment access is serialized by `lock_test_env`.
        unsafe {
            std::env::set_var("KCODE_HOME", &home_link);
            std::env::remove_var("KCODE_DEV_NAMESPACE");
        }
        remove_state_file(&path_through_link).unwrap();

        assert!(!state_file.exists());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symlink_home_in_dev_namespace() {
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
        let user_home = tempfile::tempdir().unwrap();
        let dev_home = user_home.path().join(".kcode-dev");
        std::fs::create_dir(&dev_home).unwrap();
        let home_link = user_home.path().join("home-link");
        std::os::unix::fs::symlink(&dev_home, &home_link).unwrap();
        let _restore = Restore(
            ["HOME", "KCODE_HOME", "KCODE_DEV_NAMESPACE"]
                .map(|name| (name, std::env::var_os(name)))
                .to_vec(),
        );
        // SAFETY: Test environment access is serialized by `lock_test_env`.
        unsafe {
            std::env::set_var("HOME", user_home.path());
            std::env::set_var("KCODE_HOME", &home_link);
            std::env::set_var("KCODE_DEV_NAMESPACE", "1");
        }

        let error = remove_state_file(&home_link.join("session")).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        assert!(error.to_string().contains("symlink"));
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
