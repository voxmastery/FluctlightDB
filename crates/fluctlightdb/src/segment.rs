//! Segmented storage helpers for FLCTLTDB v4.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Serialize};

use crate::error::{Error, Result};

pub fn segment_path(base: &Path, name: &str) -> PathBuf {
    base.join(format!("{name}.seg"))
}

pub fn write_segment<T: Serialize>(base: &Path, name: &str, value: &T) -> Result<()> {
    fs::create_dir_all(base)?;
    let path = segment_path(base, name);
    let tmp = path.with_extension("seg.tmp");
    let encoded = bincode::serialize(value).map_err(|e| Error::Store(e.to_string()))?;
    let mut file = File::create(&tmp)?;
    file.write_all(&encoded)?;
    file.sync_all()?;
    fs::rename(tmp, path)?;
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
