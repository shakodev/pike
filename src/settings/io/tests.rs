use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn non_ascii_override_names_are_rejected_before_commit_while_unicode_values_remain_literal() {
    let directory = Directory::new();
    let mut store = Store::new(directory.file());
    fs::write(
        directory.file(),
        "version = 1\nhistory = 41\n[profiles.default.env]\nPIKE31 = '日本語 🦀'",
    )
    .unwrap();
    store.reload().unwrap();
    let valid = store.active().clone();
    assert_eq!(
        valid.launch(valid.default_profile()).unwrap().env["PIKE31"],
        "日本語 🦀"
    );
    let invalid = "version = 1\n[profiles.default.env]\n'É' = 'one'\n'é' = 'two'";
    fs::write(directory.file(), invalid).unwrap();
    assert_eq!(
        store.reload(),
        Err(Error::Invalid(super::super::Field::Environment))
    );
    assert_eq!(store.active(), &valid);
    assert_eq!(fs::read_to_string(directory.file()).unwrap(), invalid);
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "pike31-tests-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self) -> PathBuf {
        self.0.join("settings.toml")
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn missing_does_not_create_files_and_failed_reloads_keep_last_valid_without_touching_broken_bytes()
{
    let directory = Directory::new();
    let mut store = Store::new(directory.file());
    store.reload().unwrap();
    assert!(!directory.file().exists());
    fs::write(
        directory.file(),
        "version = 1\nhistory = 7\ntheme = 'light'",
    )
    .unwrap();
    store.reload().unwrap();
    let valid = store.active().clone();
    for bytes in [
        b"not-toml-PRIVATE_SECRET".to_vec(),
        vec![b'x'; MAX_FILE_BYTES + 1],
        vec![0xff],
        b"version = 9".to_vec(),
    ] {
        fs::write(directory.file(), &bytes).unwrap();
        assert!(store.reload().is_err());
        assert_eq!(store.active(), &valid);
        assert!(store.save(&Settings::default()).is_err());
        assert_eq!(fs::read(directory.file()).unwrap(), bytes);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }
    fs::remove_file(directory.file()).unwrap();
    store.reload().unwrap();
    assert_eq!(store.active(), &Settings::default());
}

#[test]
fn explicit_save_replaces_a_complete_existing_file_and_creates_missing_parent_only_when_requested()
{
    let directory = Directory::new();
    let path = directory.0.join("nested/settings.toml");
    let store = Store::new(path.clone());
    let candidate = Settings::parse("version = 1\nhistory = 12").unwrap();
    store.save(&candidate).unwrap();
    assert_eq!(load(&path).unwrap(), candidate);
    store.save(&Settings::default()).unwrap();
    assert_eq!(load(&path).unwrap(), Settings::default());
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn interrupted_write_flush_and_replace_keep_previous_target_and_clean_only_our_temporary() {
    let directory = Directory::new();
    let previous = b"# preserve formatting\nversion = 1\nhistory = 13\n";
    let candidate = Settings::parse("version = 1\nhistory = 27").unwrap();
    for interrupted in [Stage::Written, Stage::Flushed, Stage::BeforeReplace] {
        fs::write(directory.file(), previous).unwrap();
        let result = save(&directory.file(), &candidate, |stage, temporary| {
            assert_eq!(temporary.parent(), Some(directory.0.as_path()));
            if stage == interrupted {
                Err(std::io::ErrorKind::Interrupted.into())
            } else {
                Ok(())
            }
        });
        assert!(matches!(
            result,
            Err(Error::Io {
                kind: std::io::ErrorKind::Interrupted,
                ..
            })
        ));
        assert_eq!(fs::read(directory.file()).unwrap(), previous);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }
    // A process interrupted before rename can leave a temp; loading ignores it.
    fs::write(directory.0.join(".pike-settings-orphan.tmp"), b"partial").unwrap();
    assert_eq!(load(&directory.file()).unwrap().history().lines(), 13);
    Store::new(directory.file()).save(&candidate).unwrap();
    assert_eq!(
        fs::read(directory.0.join(".pike-settings-orphan.tmp")).unwrap(),
        b"partial"
    );
}

#[test]
fn a_target_broken_during_save_is_retained_and_replace_failure_never_removes_it() {
    let directory = Directory::new();
    fs::write(directory.file(), "version = 1").unwrap();
    let broken = b"PRIVATE_SECRET broken";
    assert_eq!(
        save(&directory.file(), &Settings::default(), |stage, _| {
            if stage == Stage::Flushed {
                fs::write(directory.file(), broken)?;
            }
            Ok(())
        }),
        Err(Error::BrokenTarget)
    );
    assert_eq!(fs::read(directory.file()).unwrap(), broken);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn directories_and_symlinks_are_not_settings_and_io_errors_expose_no_paths() {
    let directory = Directory::new();
    fs::create_dir(directory.file()).unwrap();
    assert_eq!(load(&directory.file()), Err(Error::NotFile));
    assert_eq!(
        Store::new(directory.file()).save(&Settings::default()),
        Err(Error::NotFile)
    );
    let error = Error::io(
        Operation::Read,
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, "PRIVATE_SECRET"),
    );
    assert!(!format!("{error} {error:?}").contains("PRIVATE_SECRET"));
}

#[cfg(unix)]
#[test]
fn actual_permission_errors_preserve_active_and_previous_file() {
    use std::os::unix::fs::PermissionsExt;
    let directory = Directory::new();
    let mut store = Store::new(directory.file());
    fs::write(directory.file(), "version = 1\nhistory = 29").unwrap();
    store.reload().unwrap();
    fs::set_permissions(directory.file(), fs::Permissions::from_mode(0o000)).unwrap();
    assert!(matches!(
        store.reload(),
        Err(Error::Io {
            kind: std::io::ErrorKind::PermissionDenied,
            ..
        })
    ));
    assert_eq!(store.active().history().lines(), 29);
    fs::set_permissions(directory.file(), fs::Permissions::from_mode(0o600)).unwrap();
    let before = fs::read(directory.file()).unwrap();
    fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o500)).unwrap();
    let result = store.save(&Settings::default());
    fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        result,
        Err(Error::Io {
            kind: std::io::ErrorKind::PermissionDenied,
            ..
        })
    ));
    assert_eq!(fs::read(directory.file()).unwrap(), before);
}

#[cfg(windows)]
#[test]
fn windows_sharing_violation_keeps_target_and_cleans_temporary() {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = Directory::new();
    let before = "version = 1\nhistory = 31";
    fs::write(directory.file(), before).unwrap();
    // FILE_SHARE_READ only: permit validation, deny replacement/delete.
    let held = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(directory.file())
        .unwrap();
    assert!(matches!(
        Store::new(directory.file()).save(&Settings::default()),
        Err(Error::Io {
            operation: Operation::Replace,
            ..
        })
    ));
    assert_eq!(fs::read_to_string(directory.file()).unwrap(), before);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    drop(held);
}
