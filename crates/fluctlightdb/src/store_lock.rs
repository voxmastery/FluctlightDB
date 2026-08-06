//! Cross-process lock for `.flct` read/write (CLI + serve).

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fs2::FileExt;

pub fn lock_path(brain_path: &Path) -> PathBuf {
    let mut name = brain_path.as_os_str().to_os_string();
    name.push(".lock");
    PathBuf::from(name)
}

pub struct StoreLock {
    _file: File,
}

/// Shared (read) lock — multiple readers; writers (`StoreLock`) block until readers release.
pub struct SharedStoreLock {
    _file: File,
}

fn open_lock_file(brain_path: &Path) -> io::Result<File> {
    let path = lock_path(brain_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        let file = options.open(&path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        Ok(file)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        let file = options.open(&path)?;
        if file.metadata()?.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "brain lock path is a reparse point",
            ));
        }
        return Ok(file);
    }
    #[cfg(not(any(unix, windows)))]
    options.open(&path)
}

fn lock_contended(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
        || err.kind() == io::ErrorKind::AlreadyExists
        || err.to_string().to_lowercase().contains("would block")
        || err
            .to_string()
            .to_lowercase()
            .contains("resource temporarily unavailable")
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
        let file = open_lock_file(brain_path)?;
        file.try_lock_exclusive()?;
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
        let file = open_lock_file(brain_path)?;
        file.try_lock_shared()?;
        Ok(Self { _file: file })
    }
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

impl Drop for SharedStoreLock {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;
    use tempfile::tempdir;

    #[test]
    fn exclusive_lock_blocks_second_writer() {
        let dir = tempdir().unwrap();
        let brain = dir.path().join("brain");
        std::fs::create_dir_all(&brain).unwrap();

        let _first = StoreLock::try_acquire(&brain).unwrap();
        let second = StoreLock::try_acquire(&brain);
        assert!(second.is_err(), "second exclusive lock should fail");
    }

    #[test]
    fn lock_path_is_stable_when_brain_directory_appears() {
        let dir = tempdir().unwrap();
        let brain = dir.path().join("brain");
        let before = lock_path(&brain);
        std::fs::create_dir_all(&brain).unwrap();
        let after = lock_path(&brain);
        assert_eq!(before, after);
    }

    #[cfg(unix)]
    #[test]
    fn lock_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let brain = dir.path().join("brain");
        let _lock = StoreLock::try_acquire(&brain).unwrap();
        let mode = std::fs::metadata(lock_path(&brain))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn lock_released_after_drop() {
        let dir = tempdir().unwrap();
        let brain = dir.path().join("brain");
        std::fs::create_dir_all(&brain).unwrap();

        {
            let _lock = StoreLock::try_acquire(&brain).unwrap();
        }
        let again = StoreLock::try_acquire(&brain);
        assert!(again.is_ok(), "lock should be available after drop");
    }

    #[test]
    fn lock_contention_from_threads() {
        let dir = tempdir().unwrap();
        let brain = dir.path().join("brain");
        std::fs::create_dir_all(&brain).unwrap();
        let brain_path = brain.clone();
        let (tx, rx) = mpsc::channel();

        let holder = thread::spawn(move || {
            let lock = StoreLock::try_acquire(&brain_path).unwrap();
            tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(200));
            drop(lock);
        });

        rx.recv().unwrap();
        thread::sleep(Duration::from_millis(20));
        assert!(StoreLock::try_acquire(&brain).is_err());
        holder.join().unwrap();
        assert!(StoreLock::try_acquire(&brain).is_ok());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_symlink_race_cannot_redirect_lock_open_to_victim() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let dir = tempdir().unwrap();
        let brain = dir.path().join("brain");
        let victim = dir.path().join("victim");
        std::fs::write(&victim, b"do-not-touch").unwrap();
        std::fs::set_permissions(&victim, std::fs::Permissions::from_mode(0o644)).unwrap();
        symlink(&victim, lock_path(&brain)).unwrap();

        assert!(StoreLock::try_acquire(&brain).is_err());
        assert_eq!(std::fs::read(&victim).unwrap(), b"do-not-touch");
        assert_eq!(
            std::fs::metadata(&victim).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_reparse_point_cannot_redirect_lock_open_to_victim() {
        use std::os::windows::fs::symlink_file;

        let dir = tempdir().unwrap();
        let brain = dir.path().join("brain");
        let victim = dir.path().join("victim");
        std::fs::write(&victim, b"do-not-touch").unwrap();
        symlink_file(&victim, lock_path(&brain)).unwrap();

        assert!(StoreLock::try_acquire(&brain).is_err());
        assert_eq!(std::fs::read(&victim).unwrap(), b"do-not-touch");
    }
}
