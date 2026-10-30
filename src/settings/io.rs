use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{Error, MAX_FILE_BYTES, Operation, Settings};

/// No portable fallback or process-global changes. Windows: LocalAppData/Pike.
pub fn user_path() -> Result<PathBuf, Error> {
    #[cfg(windows)]
    let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    root.filter(|path| path.is_absolute())
        .map(|root| root.join("Pike/settings.toml"))
        .ok_or(Error::Location)
}

/// A missing file is a valid default configuration and never causes a write.
pub fn load(path: &Path) -> Result<Settings, Error> {
    read(path)?.map_or_else(
        || Ok(Settings::default()),
        |source| Settings::parse(&source),
    )
}

fn read(path: &Path) -> Result<Option<String>, Error> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::io(Operation::Read, error)),
    };
    if !metadata.is_file() {
        return Err(Error::NotFile);
    }
    if metadata.len() > MAX_FILE_BYTES as u64 {
        return Err(Error::TooLarge);
    }
    let mut bytes = Vec::with_capacity((metadata.len() as usize).min(MAX_FILE_BYTES));
    File::open(path)
        .map_err(|e| Error::io(Operation::Read, e))?
        .take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::io(Operation::Read, e))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Error::TooLarge);
    }
    String::from_utf8(bytes).map(Some).map_err(|_| Error::Utf8)
}

/// Last-valid ownership is useful outside the GUI too. Failed reads/validation
/// never modify active state. Saving is an explicit separate operation.
pub struct Store {
    path: PathBuf,
    active: Settings,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").finish_non_exhaustive()
    }
}

impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            active: Settings::default(),
        }
    }
    pub fn active(&self) -> &Settings {
        &self.active
    }
    pub fn reload(&mut self) -> Result<(), Error> {
        let candidate = load(&self.path)?;
        self.active = candidate;
        Ok(())
    }
    /// Refuse a broken target, even for explicit saves. The owner must correct
    /// or move it first. No save call is made by the App or shutdown path.
    pub fn save(&self, settings: &Settings) -> Result<(), Error> {
        save(&self.path, settings, |_, _| Ok(()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Written,
    Flushed,
    BeforeReplace,
}

fn save(
    path: &Path,
    settings: &Settings,
    mut checkpoint: impl FnMut(Stage, &Path) -> std::io::Result<()>,
) -> Result<(), Error> {
    if let Some(source) = read(path)?
        && Settings::parse(&source).is_err()
    {
        return Err(Error::BrokenTarget);
    }
    let text = settings.to_toml()?;
    if text.len() > MAX_FILE_BYTES {
        return Err(Error::TooLarge);
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| Error::io(Operation::CreateDirectory, e))?;
    let (temporary, mut file) = temporary(parent)?;
    let cleanup = Temporary(temporary);
    file.write_all(text.as_bytes())
        .map_err(|e| Error::io(Operation::Write, e))?;
    checkpoint(Stage::Written, &cleanup.0).map_err(|e| Error::io(Operation::Write, e))?;
    file.sync_all()
        .map_err(|e| Error::io(Operation::Flush, e))?;
    checkpoint(Stage::Flushed, &cleanup.0).map_err(|e| Error::io(Operation::Flush, e))?;
    drop(file); // Windows replacement cannot assume an open handle permits delete.
    // Check again immediately before commit, so a newly broken file survives.
    if let Some(source) = read(path)?
        && Settings::parse(&source).is_err()
    {
        return Err(Error::BrokenTarget);
    }
    checkpoint(Stage::BeforeReplace, &cleanup.0).map_err(|e| Error::io(Operation::Replace, e))?;
    // Rust's Windows implementation uses MoveFileExW(REPLACE_EXISTING), with
    // FileRenameInfoEx fallback. Same-directory, no remove-target/copy fallback.
    // ACL/sharing/read-only failures propagate with the prior target intact.
    fs::rename(&cleanup.0, path).map_err(|e| Error::io(Operation::Replace, e))?;
    Ok(())
}

fn temporary(parent: &Path) -> Result<(PathBuf, File), Error> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for _ in 0..32 {
        let path = parent.join(format!(
            ".pike-settings-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(Error::io(Operation::CreateTemporary, error)),
        }
    }
    Err(Error::Io {
        operation: Operation::CreateTemporary,
        kind: std::io::ErrorKind::AlreadyExists,
        code: None,
    })
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests;
