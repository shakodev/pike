use super::*;
use crate::tty::Shell;

fn error(config: Options, expected: LaunchError) {
    let err = Prepared::new(&config).err().expect("configuration must fail");
    assert_eq!(err.get_ref().unwrap().downcast_ref::<LaunchError>(), Some(&expected));
    let diagnostics = format!("{err} {err:?}");
    assert!(!diagnostics.contains("PRIVATE_SENTINEL"));
}

#[test]
fn invalid_explicit_configuration_is_structured_and_private() {
    for program in
        ["", "\"PRIVATE_SENTINEL.exe\"", "PRIVATE_SENTINEL\0.exe", "PRIVATE_SENTINEL.cmd"]
    {
        error(
            Options { shell: Some(Shell::new(program.into(), vec![])), ..Options::default() },
            LaunchError::InvalidProgram,
        );
    }
    error(
        Options {
            shell: Some(Shell::new("pike_PRIVATE_SENTINEL_missing.exe".into(), vec![])),
            ..Options::default()
        },
        LaunchError::ProgramNotFound,
    );
    error(
        Options {
            working_directory: Some(PathBuf::from("PRIVATE_SENTINEL\0")),
            ..Options::default()
        },
        LaunchError::InvalidDirectory,
    );
    error(
        Options { working_directory: Some(std::env::current_exe().unwrap()), ..Options::default() },
        LaunchError::InvalidDirectory,
    );
    error(
        Options {
            shell: Some(Shell::new("cmd.exe".into(), vec!["PRIVATE_SENTINEL\0".into()])),
            ..Options::default()
        },
        LaunchError::InvalidArgument,
    );
    error(
        Options {
            shell: Some(Shell::new("cmd.exe".into(), vec!["a".repeat(32767)])),
            ..Options::default()
        },
        LaunchError::CommandLineTooLong,
    );
    for key in ["", "BAD=PRIVATE_SENTINEL", "PRIVATE_SENTINEL\0"] {
        error(
            Options { env: [(key.into(), "PRIVATE_SENTINEL".into())].into(), ..Options::default() },
            LaunchError::InvalidEnvironment,
        );
    }
    error(
        Options { env: [("A".into(), "PRIVATE_SENTINEL\0".into())].into(), ..Options::default() },
        LaunchError::InvalidEnvironment,
    );
    for (a, b) in [("Path", "PATH"), ("é", "É")] {
        error(
            Options {
                env: [(a.into(), "PRIVATE_SENTINEL".into()), (b.into(), "PRIVATE_SENTINEL".into())]
                    .into(),
                ..Options::default()
            },
            LaunchError::DuplicateEnvironmentKey,
        );
    }
}

#[test]
fn environment_is_sorted_overridden_and_double_terminated() {
    let overrides = [
        ("pAtH".into(), "session".into()),
        ("é".into(), "new".into()),
        ("EMPTY".into(), "".into()),
    ]
    .into();
    let inherited = [("Path", "parent"), ("É", "old"), ("Z", "kept"), ("=C:", "C:\\cwd")]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()));
    let env = environment(&overrides, inherited).unwrap();
    assert_eq!(value(&env, "PATH"), Some("session".into()));
    assert_eq!(value(&env, "É"), Some("new".into()));
    assert_eq!(value(&env, "EMPTY"), Some("".into()));
    assert_eq!(value(&env, "Z"), Some("kept".into()));
    assert_eq!(value(&env, "=C:"), Some("C:\\cwd".into()));
    assert_eq!(env.len(), 5);
    assert!(env.windows(2).all(|p| compare(&p[0].0, &p[1].0).is_lt()));
    assert!(environment_block(&env).ends_with(&[0, 0]));
    assert_eq!(environment_block(&vec![]), [0, 0]);
    // Inherited Windows environment data need not be valid Unicode.
    let env = environment(
        &HashMap::new(),
        [("UNICODE".into(), OsString::from_wide(&[0xD800]))].into_iter(),
    )
    .unwrap();
    assert_eq!(value(&env, "UNICODE").unwrap().encode_wide().collect::<Vec<_>>(), [0xD800]);
}

#[test]
fn crt_encoding_preserves_quotes_spaces_and_trailing_backslashes() {
    for (arg, expected) in [
        ("abc", "abc"),
        ("", "\"\""),
        ("a b", "\"a b\""),
        ("a\tb", "\"a\tb\""),
        ("a\"b", "a\\\"b"),
        ("C:\\Program Files\\", "\"C:\\Program Files\\\\\""),
        ("雪🦀", "雪🦀"),
        ("& %PATH% | >", "\"& %PATH% | >\""),
    ] {
        let mut result = String::new();
        push_escaped_arg(&mut result, arg);
        assert_eq!(result, expected);
    }
}

#[test]
fn automatic_shell_prefers_pwsh_and_wsl_is_never_implicit() {
    let directory = std::env::temp_dir().join(format!("pike-launch-unit-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    std::fs::write(directory.join("pwsh.exe"), []).unwrap();
    std::fs::write(directory.join("wsl.exe"), []).unwrap();
    let env = environment(
        &[("PATH".into(), directory.to_str().unwrap().into())].into(),
        std::iter::empty(),
    )
    .unwrap();
    assert_eq!(default_shell(&directory, &env).unwrap().unwrap().file_name().unwrap(), "pwsh.exe");
    std::fs::remove_file(directory.join("pwsh.exe")).unwrap();
    let default = default_shell(&directory, &env).unwrap().unwrap();
    assert_ne!(default.file_name().unwrap(), "wsl.exe");
    assert!(resolve("wsl.exe", &directory, &env).unwrap().is_some());
}

#[test]
fn preparation_quotes_argv0_and_only_raw_mode_preserves_shell_syntax() {
    let mut config = Options {
        shell: Some(Shell::new("cmd.exe".into(), vec!["/c".into(), "echo a & echo b".into()])),
        escape_args: true,
        ..Options::default()
    };
    let prepared = Prepared::new(&config).unwrap();
    assert_eq!(prepared.command[0], b'"' as u16);
    assert!(!String::from_utf16_lossy(&prepared.program).starts_with(r"\\?\"));
    assert!(prepared.environment.is_none());
    assert!(String::from_utf16_lossy(&prepared.command).ends_with(" /c \"echo a & echo b\"\0"));
    config.escape_args = false;
    assert!(String::from_utf16_lossy(&Prepared::new(&config).unwrap().command)
        .ends_with(" /c echo a & echo b\0"));
}

#[test]
fn drive_relative_paths_cannot_escape_session_resolution() {
    error(
        Options {
            shell: Some(Shell::new(r"C:PRIVATE_SENTINEL.exe".into(), vec![])),
            ..Options::default()
        },
        LaunchError::InvalidProgram,
    );
    error(
        Options {
            shell: Some(Shell::new("PRIVATE_SENTINEL.exe".into(), vec![])),
            env: [("PATH".into(), r"C:PRIVATE_SENTINEL".into())].into(),
            ..Options::default()
        },
        LaunchError::InvalidEnvironment,
    );
    let parent = std::env::current_dir().unwrap();
    let Some(Component::Prefix(prefix)) = parent.components().next() else {
        panic!("Windows current directory must have a prefix");
    };
    if matches!(prefix.kind(), std::path::Prefix::Disk(_)) {
        let mut relative = prefix.as_os_str().to_owned();
        relative.push(".");
        let config = Options {
            shell: Some(Shell::new(
                std::env::current_exe().unwrap().to_str().unwrap().into(),
                vec![],
            )),
            working_directory: Some(relative.into()),
            ..Options::default()
        };
        let prepared = Prepared::new(&config).unwrap();
        let actual =
            PathBuf::from(OsString::from_wide(&prepared.directory[..prepared.directory.len() - 1]));
        assert!(actual.is_absolute());
        assert_eq!(actual, std::path::absolute(&parent).unwrap());
    }
}

#[test]
fn plain_relative_programs_and_path_entries_use_the_session_directory() {
    let directory =
        std::env::temp_dir().join(format!("pike-relative-launch-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let program = directory.join("fixture.exe");
    std::fs::write(&program, []).unwrap();
    let env = environment(&[("PATH".into(), ".".into())].into(), std::iter::empty()).unwrap();
    let expected = std::path::absolute(program).unwrap();
    assert_eq!(resolve(r".\fixture.exe", &directory, &env).unwrap(), Some(expected.clone()));
    assert_eq!(resolve("fixture.exe", &directory, &env).unwrap(), Some(expected));
}
