use super::*;

#[test]
fn missing_fields_have_documented_defaults_and_serialization_roundtrips() {
    let settings = Settings::parse("version = 1").unwrap();
    assert_eq!(settings, Settings::default());
    assert_eq!(settings.font().size, 14.0);
    assert_eq!(settings.history().lines(), 2000);
    assert_eq!(settings.close_policy(), ClosePolicy::ConfirmRunning);
    assert_eq!(
        Settings::parse(&settings.to_toml().unwrap()).unwrap(),
        settings
    );
}

#[test]
fn malformed_oversized_unknown_version_and_unknown_fields_are_rejected_without_source_text() {
    for source in [
        "version = 1\nprofiles = PRIVATE_SECRET",
        "version = 1\nPRIVATE_SECRET = 2",
        "version = 1\n[font]\nsize = 'PRIVATE_SECRET'",
        "version = 1\n[font]\nfamily = 'PRIVATE_SECRET'",
        "version = 1\n[profiles.default]\nPRIVATE_SECRET = 2",
        "version = 1\n[profiles.default.env]\nsecret = 'unterminated-PRIVATE_SECRET",
        "version = 1\nversion = 1",
        "[font]\nsize = 14",
    ] {
        let error = Settings::parse(source).unwrap_err();
        assert!(matches!(
            error,
            Error::Parse {
                line: 1..,
                column: 1..
            }
        ));
        assert!(!format!("{error} {error:?}").contains("PRIVATE_SECRET"));
    }
    assert_eq!(
        Settings::parse("version = 2\nunknown = 'future'"),
        Err(Error::Version)
    );
    assert_eq!(
        Settings::parse(&" ".repeat(MAX_FILE_BYTES + 1)),
        Err(Error::TooLarge)
    );
}

#[test]
fn ranges_reject_instead_of_clamping_and_accept_exact_boundaries() {
    for size in ["9.99", "28.01", "nan", "inf", "-inf"] {
        assert_eq!(
            Settings::parse(&format!("version = 1\n[font]\nsize = {size}")),
            Err(Error::Invalid(Field::FontSize))
        );
    }
    for size in [10, 28] {
        assert_eq!(
            Settings::parse(&format!("version = 1\n[font]\nsize = {size}"))
                .unwrap()
                .font()
                .size,
            size as f32
        );
    }
    for history in [0, 10_000] {
        assert_eq!(
            Settings::parse(&format!("version = 1\nhistory = {history}"))
                .unwrap()
                .history()
                .lines(),
            history
        );
    }
    assert_eq!(
        Settings::parse("version = 1\nhistory = 10001"),
        Err(Error::Invalid(Field::History))
    );
    assert!(Settings::parse("version = 1\nhistory = -1").is_err());
    assert!(Settings::parse("version = 1\ntheme = 'unknown'").is_err());
    assert!(Settings::parse("version = 1\nclose_policy = 'guess-jobs'").is_err());
}

#[test]
fn launch_schema_is_bounded_and_keeps_literals_and_debug_redacted() {
    for (suffix, field) in [
        ("shell = ''", Field::Shell),
        ("shell = 'C:relative.exe'", Field::Shell),
        ("shell = '\"quoted\"'", Field::Shell),
        ("args = ['no-explicit-shell']", Field::Args),
        ("cwd = 'relative'", Field::Cwd),
        ("env = { A = 'one', a = 'two' }", Field::Environment),
        ("env = { 'BAD=NAME' = 'two' }", Field::Environment),
    ] {
        assert_eq!(
            Settings::parse(&format!("version = 1\n[profiles.default]\n{suffix}")),
            Err(Error::Invalid(field))
        );
    }
    let mut document = Document::default();
    let profile = document.profiles.get_mut(&ProfileId::default()).unwrap();
    profile.shell = Some("PRIVATE_SECRET".into());
    profile.args = vec![
        "two words".into(),
        "\"literal\"".into(),
        "$no-expansion".into(),
        "".into(),
    ];
    profile
        .env
        .insert("PIKE_FIXTURE".into(), "PRIVATE_SECRET".into());
    validate::document(&document).unwrap();
    let settings = Settings(document.clone());
    let launch = settings.launch(settings.default_profile()).unwrap();
    assert_eq!(
        launch.args,
        document.profiles[settings.default_profile()].args
    );
    assert_eq!(launch.env["PIKE_FIXTURE"], "PRIVATE_SECRET");
    assert!(!format!("{settings:?} {launch:?}").contains("PRIVATE_SECRET"));
    for (kind, field) in [(0, Field::Shell), (1, Field::Args), (2, Field::Environment)] {
        let mut candidate = document.clone();
        let profile = candidate.profiles.get_mut(&ProfileId::default()).unwrap();
        match kind {
            0 => profile.shell = Some("a".repeat(4097)),
            1 => profile.args = vec!["a".into(); 65],
            _ => profile.env = (0..65).map(|i| (format!("K{i}"), "v".into())).collect(),
        }
        assert_eq!(validate::document(&candidate), Err(Error::Invalid(field)));
    }
}

#[test]
fn profile_references_and_counts_are_validated() {
    assert_eq!(
        Settings::parse("version = 1\ndefault_profile = 'missing'"),
        Err(Error::Invalid(Field::DefaultProfile))
    );
    assert_eq!(
        Settings::parse("version = 1\nprofiles = {}"),
        Err(Error::Invalid(Field::Profiles))
    );
    let doc = Document {
        profiles: (0..17)
            .map(|i| (ProfileId(format!("profile{i}")), Profile::default()))
            .collect(),
        ..Document::default()
    };
    assert_eq!(
        validate::document(&doc),
        Err(Error::Invalid(Field::Profiles))
    );
    assert!(Settings::parse("version = 1\n[profiles.'bad profile']").is_err());
}

#[test]
fn shortcuts_have_a_typed_bounded_schema_with_canonical_duplicate_and_reserved_chord_checks() {
    let one =
        "[[shortcuts]]\ncommand = 'reload-settings'\nkey = 'F5'\nmodifiers = ['ctrl', 'shift']\n";
    let source = format!("version = 1\n{one}");
    let parsed = Settings::parse(&source).unwrap();
    assert_eq!(parsed.shortcuts().len(), 1);
    assert_eq!(Settings::parse(&parsed.to_toml().unwrap()).unwrap(), parsed);
    for bad in [
        source.replace("'F5'", "'v'"),
        source.replace("'F5'", "'c'"),
        source.replace("['ctrl', 'shift']", "['ctrl']"),
        source.replace("['ctrl', 'shift']", "['ctrl', 'ctrl', 'shift']"),
        format!("version = 1\n{one}{one}"),
        format!(
            "version = 1\n{one}{}",
            one.replace("reload-settings", "new-tab")
                .replace("F5", "f5")
        ),
        format!("version = 1\n{}", one.repeat(65)),
    ] {
        assert_eq!(Settings::parse(&bad), Err(Error::Invalid(Field::Shortcuts)));
    }
    assert!(Settings::parse(&source.replace("reload-settings", "arbitrary-script")).is_err());
    assert!(Settings::parse(&source.replace("'F5'", "'unknown'")).is_err());
}

#[test]
fn function_key_numeric_aliases_cannot_bypass_canonical_chord_uniqueness() {
    for alias in ["f05", "f+5", "f0005", "F05"] {
        let source = format!(
            "version = 1\n[[shortcuts]]\ncommand = 'reload-settings'\nkey = 'f5'\nmodifiers = ['ctrl', 'shift']\n[[shortcuts]]\ncommand = 'new-tab'\nkey = '{alias}'\nmodifiers = ['shift', 'ctrl']"
        );
        assert_eq!(
            Settings::parse(&source),
            Err(Error::Invalid(Field::Shortcuts)),
            "alias {alias} bypassed duplicate detection"
        );
    }
    let one = Settings::parse("version = 1\n[[shortcuts]]\ncommand = 'reload-settings'\nkey = 'f0005'\nmodifiers = ['ctrl', 'shift']").unwrap();
    assert!(one.to_toml().unwrap().contains("key = \"f5\""));
}
