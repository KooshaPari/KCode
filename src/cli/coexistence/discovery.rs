use std::ffi::OsStr;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Discovery {
    FoundOnPath,
    NotFoundOnPath,
    Unknown,
}

impl Discovery {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::FoundOnPath => "found_on_path",
            Self::NotFoundOnPath => "not_found_on_path",
            Self::Unknown => "unknown",
        }
    }
}

pub(super) fn discover(path: Option<&OsStr>, names: &[&str]) -> Discovery {
    discover_with(path, names, |file| {
        std::fs::metadata(file).map(|m| m.is_file())
    })
}

fn discover_with(
    path: Option<&OsStr>,
    names: &[&str],
    mut is_file: impl FnMut(&Path) -> io::Result<bool>,
) -> Discovery {
    let Some(path) = path else {
        return Discovery::Unknown;
    };
    let mut uncertain = false;
    for directory in std::env::split_paths(path) {
        if directory.as_os_str().is_empty() || !directory.is_absolute() || is_unc(&directory) {
            uncertain = true;
            continue;
        }
        for name in names {
            match is_file(&directory.join(name)) {
                Ok(true) => return Discovery::FoundOnPath,
                Ok(false) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => uncertain = true,
            }
        }
    }
    if uncertain {
        Discovery::Unknown
    } else {
        Discovery::NotFoundOnPath
    }
}

#[cfg(windows)]
fn is_unc(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    matches!(path.components().next(), Some(Component::Prefix(prefix))
        if matches!(prefix.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..)))
}

#[cfg(not(windows))]
fn is_unc(_: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absence_and_inaccessible_are_distinct() {
        let dir = std::env::current_dir().unwrap();
        let path = std::env::join_paths([dir]).unwrap();
        assert_eq!(
            discover_with(Some(&path), &["jcode"], |_| Err(
                io::ErrorKind::NotFound.into()
            )),
            Discovery::NotFoundOnPath
        );
        assert_eq!(
            discover_with(Some(&path), &["jcode"], |_| Err(
                io::ErrorKind::PermissionDenied.into()
            )),
            Discovery::Unknown
        );
        assert_eq!(
            discover_with(Some(&path), &["jcode"], |_| Ok(true)),
            Discovery::FoundOnPath
        );
        assert_eq!(
            discover_with(None, &["jcode"], |_| Ok(true)),
            Discovery::Unknown
        );
        assert_eq!(
            discover_with(Some(OsStr::new("relative")), &["jcode"], |_| Ok(true)),
            Discovery::Unknown
        );
    }

    #[cfg(windows)]
    #[test]
    fn unc_entries_are_not_probed() {
        assert_eq!(
            discover_with(Some(OsStr::new(r"\\server\share")), &["jcode.exe"], |_| {
                panic!("remote path must not be probed")
            }),
            Discovery::Unknown
        );
    }
}
