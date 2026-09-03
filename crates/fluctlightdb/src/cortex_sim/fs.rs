//! Logical filesystem for simulation traces (path → bytes).

use std::collections::BTreeMap;

#[derive(Debug, Default, Clone)]
pub struct CortexFs {
    files: BTreeMap<String, Vec<u8>>,
}

impl CortexFs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write(&mut self, path: impl Into<String>, bytes: impl AsRef<[u8]>) {
        self.files.insert(path.into(), bytes.as_ref().to_vec());
    }

    pub fn read(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(|v| v.as_slice())
    }

    pub fn exists(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    pub fn remove(&mut self, path: &str) -> bool {
        self.files.remove(path).is_some()
    }
}
