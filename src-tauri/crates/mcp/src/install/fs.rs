use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Writes through a temp file and a rename so the settings file never holds partial data.
/// The folder is 0700 and the file is 0600 because the file can hold API tokens.
pub(super) fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let written = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(data)?;
        drop(file);
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
    })();
    if let Err(err) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }
    if let Err(err) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

/// The file's bytes, or nothing when it does not exist yet.
pub(super) fn read_or_empty(path: &Path) -> std::io::Result<Vec<u8>> {
    match std::fs::read(path) {
        Ok(data) => Ok(data),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}
