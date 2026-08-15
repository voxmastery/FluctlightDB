//! Segmented storage helpers for FLCTLTDB v4.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Serialize};

use crate::error::{Error, Result};

pub(crate) fn create_private_dir_all(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub(crate) fn create_private_file(path: &Path) -> Result<File> {
    let mut options = fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        let file = options.open(path)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        Ok(file)
    }
    #[cfg(not(unix))]
    Ok(options.open(path)?)
}

pub(crate) fn sync_parent_dir(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = parent;
    Ok(())
}

pub fn segment_path(base: &Path, name: &str) -> PathBuf {
    base.join(format!("{name}.seg"))
}

pub fn write_segment<T: Serialize>(base: &Path, name: &str, value: &T) -> Result<()> {
    create_private_dir_all(base)?;
    let path = segment_path(base, name);
    let tmp = path.with_extension("seg.tmp");
    let encoded = bincode::serialize(value).map_err(|e| Error::Store(e.to_string()))?;
    let mut file = create_private_file(&tmp)?;
    crate::checkpoint_fault::hit("generation.before_file_write");
    file.write_all(&encoded)?;
    crate::checkpoint_fault::hit("generation.after_file_write");
    file.sync_all()?;
    crate::checkpoint_fault::hit("generation.after_file_fsync");
    drop(file);
    fs::rename(tmp, path)?;
    crate::checkpoint_fault::hit("generation.after_file_rename");
    sync_parent_dir(&segment_path(base, name))?;
    crate::checkpoint_fault::hit("generation.after_file_dir_fsync");
    Ok(())
}

pub fn read_segment<T: DeserializeOwned>(base: &Path, name: &str) -> Result<T> {
    let path = segment_path(base, name);
    let mut file = File::open(&path)?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;
    bincode::deserialize(&raw).map_err(|e| Error::Serde(e.to_string()))
}

pub fn segment_exists(base: &Path, name: &str) -> bool {
    segment_path(base, name).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parent_directory_can_be_synced_after_publication() {
        let dir = tempdir().unwrap();
        let published = dir.path().join("published");
        fs::write(&published, b"durable").unwrap();
        sync_parent_dir(&published).unwrap();
    }
}
