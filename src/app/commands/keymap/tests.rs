use super::*;
use crate::settings::{Settings, shortcuts::Problem};
use iced::keyboard::{Location, Modifiers, key};

fn event(key: Key, modified_key: Key, modifiers: Modifiers, repeat: bool) -> keyboard::Event {
    keyboard::Event::KeyPressed {
        key,
        modified_key,
        modifiers,
        repeat,
        text: None,
        physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
        location: Location::Standard,
    }
}
fn character(base: &str, modified: &str, modifiers: Modifiers, repeat: bool) -> keyboard::Event {
    event(
        Key::Character(base.into()),
        Key::Character(modified.into()),
        modifiers,
        repeat,
    )
}
fn mappings(body: &str) -> Settings {
    Settings::parse(&format!("version = 1\n{body}")).unwrap()
}

#[test]
fn replacements_remove_old_chords_unbind_and_reuse_explicitly_released_chords() {
    let settings = mappings(
        "[[shortcuts]]\ncommand='split-right'\nkey='k'\nmodifiers=['ctrl','shift']\n[[shortcuts]]\ncommand='font-larger'\nkey='o'\nmodifiers=['shift','ctrl']\n[[shortcuts]]\ncommand='close-pane'\nunbind=true",
    );
    let cs = Modifiers::CTRL | Modifiers::SHIFT;
    for (key, expected) in [
        ("k", Some(Command::Split(Direction::Right))),
        ("o", Some(Command::Font(FontChange::Larger))),
        ("+", None),
        ("w", None),
    ] {
        assert_eq!(
            resolve(&character(key, key, cs, false), settings.shortcuts()),
            expected.map(Resolution::Dispatch)
        );
    }
    assert_eq!(
        resolve(&character("k", "K", cs, true), settings.shortcuts()),
        Some(Resolution::Consume)
    );
}

#[test]
fn every_catalog_action_accepts_an_explicit_chord_through_the_same_resolver() {
    // Unbind other commands so each trial is independent of the default map.
    // This includes all stable host controls, profile entries and menu actions.
    for target in Id::ALL {
        let mut source = String::from("version=1\n");
        for id in Id::ALL {
            let name = toml::Value::try_from(id).unwrap();
            source.push_str(&format!("[[shortcuts]]\ncommand={name}\n"));
            source.push_str(if id == target {
                "key='f2'\nmodifiers=['ctrl','shift']\n"
            } else {
                "unbind=true\n"
            });
        }
        let settings = Settings::parse(&source).unwrap();
        assert_eq!(
            resolve(
                &event(
                    Key::Named(Named::F2),
                    Key::Named(Named::F2),
                    Modifiers::CTRL | Modifiers::SHIFT,
                    false
                ),
                settings.shortcuts()
            ),
            Some(Resolution::Dispatch(command(*target)))
        );
    }
}

#[test]
fn shifted_symbols_and_non_us_logical_letters_precede_base_keys_and_altgr_is_untouched() {
    let settings = mappings(
        "[[shortcuts]]\ncommand='new-tab'\nkey='!'\nmodifiers=['ctrl','shift']\n[[shortcuts]]\ncommand='help'\nkey='ö'\nmodifiers=['ctrl','shift']\n[[shortcuts]]\ncommand='font-larger'\nkey='?'\nmodifiers=['ctrl','shift']",
    );
    let cs = Modifiers::CTRL | Modifiers::SHIFT;
    assert_eq!(
        resolve(&character("1", "!", cs, false), settings.shortcuts()),
        Some(Resolution::Dispatch(Command::NewTab))
    );
    assert_eq!(
        resolve(&character("ö", "Ö", cs, false), settings.shortcuts()),
        Some(Resolution::Dispatch(Command::Help))
    );
    assert_eq!(
        resolve(&character("/", "?", cs, false), settings.shortcuts()),
        Some(Resolution::Dispatch(Command::Font(FontChange::Larger)))
    );
    assert_eq!(
        resolve(
            &character("ö", "Ö", cs | Modifiers::ALT, false),
            settings.shortcuts()
        ),
        None
    );
    assert_eq!(
        resolve(
            &character("c", "c", Modifiers::CTRL, false),
            settings.shortcuts()
        ),
        None
    );
    assert_eq!(
        resolve(
            &event(Key::Unidentified, Key::Unidentified, cs, false),
            settings.shortcuts()
        ),
        None
    );
}

#[test]
fn duplicate_effective_defaults_aliases_and_unsafe_forms_have_indexed_errors() {
    for (body, problem) in [
        (
            "command='new-tab'\nkey='o'\nmodifiers=['ctrl','shift']",
            Problem::Duplicate,
        ),
        (
            "command='help'\nkey='f13'\nmodifiers=['ctrl','shift']",
            Problem::Unsupported,
        ),
        (
            "command='help'\nkey='c'\nmodifiers=['ctrl']",
            Problem::Modifiers,
        ),
        (
            "command='help'\nkey='q'\nmodifiers=['ctrl','alt']",
            Problem::Modifiers,
        ),
        (
            "command='help'\nkey='f2'\nmodifiers=['ctrl','ctrl','shift']",
            Problem::Modifiers,
        ),
        ("command='help'\nkey='f1'\nunbind=true", Problem::Unbind),
    ] {
        let error = Settings::parse(&format!("version=1\n[[shortcuts]]\n{body}")).unwrap_err();
        assert_eq!(
            error,
            crate::settings::Error::Shortcut { entry: 1, problem }
        );
        assert!(error.to_string().contains("Shortcut entry 1"));
        assert!(error.to_string().contains("last valid"));
    }
    let repeated = "[[shortcuts]]\ncommand='help'\nkey='f02'\nmodifiers=['ctrl','shift']\n[[shortcuts]]\ncommand='help'\nkey='f2'\nmodifiers=['shift','ctrl']";
    assert!(matches!(
        Settings::parse(&format!("version=1\n{repeated}")),
        Err(crate::settings::Error::Shortcut {
            problem: Problem::Duplicate,
            ..
        })
    ));
}

#[test]
fn alternatives_replace_the_entire_default_set_and_roundtrip() {
    let settings = mappings(
        "[[shortcuts]]\ncommand='paste'\nkey='f2'\nmodifiers=['ctrl','shift']\n[[shortcuts]]\ncommand='paste'\nkey='f3'\nmodifiers=['ctrl','shift']",
    );
    let paste: Vec<_> = shortcuts::effective(settings.shortcuts())
        .filter(|binding| binding.command == Id::Paste)
        .collect();
    assert_eq!(
        paste.iter().map(|binding| binding.key).collect::<Vec<_>>(),
        ["f2", "f3"]
    );
    assert_eq!(
        Settings::parse(&settings.to_toml().unwrap()).unwrap(),
        settings
    );
    assert_eq!(
        resolve(
            &character("v", "V", Modifiers::CTRL | Modifiers::SHIFT, false),
            settings.shortcuts()
        ),
        None
    );
    assert_eq!(
        resolve(
            &event(
                Key::Named(Named::Insert),
                Key::Named(Named::Insert),
                Modifiers::SHIFT,
                false
            ),
            settings.shortcuts()
        ),
        None
    );
}
