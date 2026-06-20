//! Cross-process lock for `.flct` read/write (CLI + serve).

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub fn lock_path(brain_path: &Path) -> PathBuf {
    crate::storage::lock_path(brain_path)
}

pub struct StoreLock {
    _file: File,
}

/// Shared (read) lock — multiple readers; writers (`StoreLock`) block until readers release.
pub struct SharedStoreLock {
    _file: File,
}

impl StoreLock {
    pub fn acquire(brain_path: &Path) -> io::Result<Self> {
        Self::acquire_with_timeout(brain_path, Duration::from_secs(120))
    }

    pub fn acquire_with_timeout(brain_path: &Path, timeout: Duration) -> io::Result<Self> {
        let deadline = Instant::now() + timeout;
        loop {
            match Self::try_acquire(brain_path) {
                Ok(lock) => return Ok(lock),
                Err(e) if lock_contended(&e) => {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::WouldBlock,
                            format!(
                                "brain lock busy for {}s — stop fluctlight-serve or wait",
                                timeout.as_secs()
                            ),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub fn try_acquire(brain_path: &Path) -> io::Result<Self> {
        let path = lock_path(brain_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = file.as_raw_fd();
            let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Self { _file: file })
    }
}

impl SharedStoreLock {
    pub fn acquire(brain_path: &Path) -> io::Result<Self> {
        Self::acquire_with_timeout(brain_path, Duration::from_secs(120))
    }

    pub fn acquire_with_timeout(brain_path: &Path, timeout: Duration) -> io::Result<Self> {
        let deadline = Instant::now() + timeout;
        loop {
            match Self::try_acquire(brain_path) {
                Ok(lock) => return Ok(lock),
                Err(e) if lock_contended(&e) => {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::WouldBlock,
                            format!(
                                "brain read lock busy for {}s — retry shortly",
                                timeout.as_secs()
                            ),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub fn try_acquire(brain_path: &Path) -> io::Result<Self> {
        let path = lock_path(brain_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = file.as_raw_fd();
            let ret = unsafe { libc::flock(fd, libc::LOCK_SH | libc::LOCK_NB) };
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Self { _file: file })
    }
}

#[cfg(unix)]
fn lock_contended(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
        || err.raw_os_error() == Some(libc::EWOULDBLOCK)
        || err.raw_os_error() == Some(libc::EAGAIN)
}

#[cfg(not(unix))]
fn lock_contended(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        unlock_file(&self._file);
    }
}

impl Drop for SharedStoreLock {
    fn drop(&mut self) {
        unlock_file(&self._file);
    }
}

#[cfg(unix)]
fn unlock_file(file: &File) {
    use std::os::unix::io::AsRawFd;
    let fd = file.as_raw_fd();
    unsafe {
        libc::flock(fd, libc::LOCK_UN);
    }
}

#[cfg(not(unix))]
fn unlock_file(_file: &File) {}
