use std::io::{self, Write};
use std::path::Path;

pub(super) fn save_recording_to_path(path: &Path, contents: &str) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "recording path has no parent")
    })?;
    crate::storage::reject_dev_home_symlink_path(parent)
        .and_then(|()| crate::storage::reject_dev_home_symlink_path(path))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    std::fs::create_dir_all(parent)?;
    crate::storage::reject_dev_home_symlink_path(path)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)?.write_all(contents.as_bytes())
}
