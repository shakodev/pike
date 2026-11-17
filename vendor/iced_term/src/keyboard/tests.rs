use super::*;
use iced_core::keyboard::{key, Location};

fn character(key: &str, modifiers: Modifiers, text: Option<&str>) -> Event {
    Event::KeyPressed {
        key: Key::Character(key.into()),
        modified_key: Key::Character(key.into()),
        physical_key: key::Physical::Unidentified(
            key::NativeCode::Unidentified,
        ),
        location: Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    }
}

fn named(key: Named, modifiers: Modifiers) -> Event {
    Event::KeyPressed {
        key: Key::Named(key),
        modified_key: Key::Named(key),
        physical_key: key::Physical::Unidentified(
            key::NativeCode::Unidentified,
        ),
        location: Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    }
}

fn bytes(event: &Event, mode: TermMode) -> Vec<u8> {
    match action(event, &BindingsLayout::new(), mode) {
        KeyAction::Write { bytes, alt_prefix } => {
            let mut result = Vec::new();
            if alt_prefix {
                result.push(0x1b);
            }
            result.extend_from_slice(&bytes);
            result
        },
        _ => panic!("expected encoded key"),
    }
}

#[test]
fn all_control_letters_include_ctrl_u_and_ctrl_c() {
    for (letter, expected) in (b'a'..=b'z').zip(1_u8..=26) {
        let key = char::from(letter).to_string();
        assert_eq!(
            bytes(&character(&key, Modifiers::CTRL, None), TermMode::empty()),
            [expected]
        );
    }
    assert_eq!(
        bytes(
            &character("U", Modifiers::CTRL | Modifiers::SHIFT, Some("U")),
            TermMode::empty()
        ),
        b"\x15"
    );
    assert_eq!(
        bytes(
            &character("c", Modifiers::CTRL, Some("\x03")),
            TermMode::empty()
        ),
        b"\x03"
    );
}

#[test]
fn cursor_mode_and_modified_navigation_use_xterm_sequences() {
    for (key, normal, app, modified) in [
        (Named::ArrowUp, "\x1b[A", "\x1bOA", "\x1b[1;5A"),
        (Named::ArrowDown, "\x1b[B", "\x1bOB", "\x1b[1;5B"),
        (Named::ArrowRight, "\x1b[C", "\x1bOC", "\x1b[1;5C"),
        (Named::ArrowLeft, "\x1b[D", "\x1bOD", "\x1b[1;5D"),
        (Named::Home, "\x1b[H", "\x1bOH", "\x1b[1;5H"),
        (Named::End, "\x1b[F", "\x1bOF", "\x1b[1;5F"),
    ] {
        assert_eq!(
            bytes(&named(key, Modifiers::empty()), TermMode::empty()),
            normal.as_bytes()
        );
        assert_eq!(
            bytes(&named(key, Modifiers::empty()), TermMode::APP_CURSOR),
            app.as_bytes()
        );
        assert_eq!(
            bytes(&named(key, Modifiers::CTRL), TermMode::APP_CURSOR),
            modified.as_bytes()
        );
    }
}

#[test]
fn modified_function_and_editing_keys_are_not_malformed_ss3() {
    for (key, modifiers, expected) in [
        (Named::F1, Modifiers::CTRL, "\x1b[1;5P"),
        (Named::F2, Modifiers::SHIFT, "\x1b[1;2Q"),
        (Named::F3, Modifiers::ALT, "\x1b[1;3R"),
        (
            Named::F4,
            Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT,
            "\x1b[1;8S",
        ),
        (Named::F5, Modifiers::empty(), "\x1b[15~"),
        (Named::F12, Modifiers::CTRL | Modifiers::ALT, "\x1b[24;7~"),
        (Named::F20, Modifiers::SHIFT, "\x1b[34;2~"),
        (Named::Insert, Modifiers::ALT, "\x1b[2;3~"),
        (
            Named::Delete,
            Modifiers::CTRL | Modifiers::SHIFT,
            "\x1b[3;6~",
        ),
        (
            Named::PageUp,
            Modifiers::ALT | Modifiers::SHIFT,
            "\x1b[5;4~",
        ),
        (Named::PageDown, Modifiers::CTRL, "\x1b[6;5~"),
        (Named::Home, Modifiers::SHIFT, "\x1b[1;2H"),
    ] {
        assert_eq!(
            bytes(&named(key, modifiers), TermMode::ALT_SCREEN),
            expected.as_bytes()
        );
    }
}

#[test]
fn altgr_and_composed_text_are_preserved_without_escape_prefixes() {
    for (key, text) in [("q", "@"), ("e", "€"), ("2", "²"), ("a", "á")] {
        assert_eq!(
            bytes(
                &character(key, Modifiers::CTRL | Modifiers::ALT, Some(text)),
                TermMode::empty()
            ),
            text.as_bytes()
        );
    }
    assert_eq!(
        bytes(
            &character("e", Modifiers::ALT, Some("é")),
            TermMode::empty()
        ),
        "é".as_bytes()
    );
    assert_eq!(
        bytes(
            &character("e", Modifiers::empty(), Some("e\u{301}")),
            TermMode::empty()
        ),
        "e\u{301}".as_bytes()
    );
    let mut dead = character("e", Modifiers::ALT, None);
    if let Event::KeyPressed { modified_key, .. } = &mut dead {
        *modified_key = Key::Unidentified;
    }
    assert!(matches!(
        action(&dead, &BindingsLayout::new(), TermMode::empty()),
        KeyAction::Consume
    ));
    if let Event::KeyPressed { modifiers, .. } = &mut dead {
        *modifiers = Modifiers::CTRL | Modifiers::ALT;
    }
    assert!(matches!(
        action(&dead, &BindingsLayout::new(), TermMode::empty()),
        KeyAction::Consume
    ));
    assert_eq!(
        bytes(
            &character("с", Modifiers::CTRL, Some("\x03")),
            TermMode::empty()
        ),
        b"\x03"
    );
}

#[test]
fn alt_control_punctuation_and_repeat_have_exactly_one_output() {
    assert_eq!(
        bytes(
            &character("b", Modifiers::ALT, Some("b")),
            TermMode::empty()
        ),
        b"\x1bb"
    );
    assert_eq!(
        bytes(
            &character("u", Modifiers::CTRL | Modifiers::ALT, None),
            TermMode::empty()
        ),
        b"\x1b\x15"
    );
    for (key, expected) in [
        ("[", 0x1b),
        ("\\", 0x1c),
        ("]", 0x1d),
        ("^", 0x1e),
        ("_", 0x1f),
        ("@", 0),
    ] {
        assert_eq!(
            bytes(&character(key, Modifiers::CTRL, None), TermMode::empty()),
            [expected]
        );
    }
    let mut repeat = character("x", Modifiers::empty(), Some("x"));
    if let Event::KeyPressed { repeat, .. } = &mut repeat {
        *repeat = true;
    }
    assert_eq!(bytes(&repeat, TermMode::empty()), b"x");
}

#[test]
fn ignore_is_consumed_and_custom_mode_bindings_take_precedence() {
    use crate::bindings::{Binding, InputKind};
    let mut bindings = BindingsLayout::new();
    let broad = Binding {
        target: InputKind::Char("x".into()),
        modifiers: Modifiers::empty(),
        terminal_mode_include: TermMode::empty(),
        terminal_mode_exclude: TermMode::empty(),
    };
    bindings.add_bindings(vec![(broad.clone(), BindingAction::Ignore)]);
    let key = character("X", Modifiers::empty(), Some("X"));
    assert!(matches!(
        action(&key, &bindings, TermMode::empty()),
        KeyAction::Consume
    ));
    bindings.add_bindings(vec![(
        Binding {
            terminal_mode_include: TermMode::APP_CURSOR,
            ..broad.clone()
        },
        BindingAction::Char('!'),
    )]);
    assert!(matches!(
        action(&key, &bindings, TermMode::empty()),
        KeyAction::Consume
    ));
    assert!(matches!(
        action(&key, &bindings, TermMode::APP_CURSOR),
        KeyAction::Write { .. }
    ));
    bindings.add_bindings(vec![(broad, BindingAction::Ignore)]);
    assert!(matches!(
        action(&key, &bindings, TermMode::APP_CURSOR),
        KeyAction::Consume
    ));
}

#[test]
fn control_digit_aliases_and_unmapped_committed_text_follow_legacy_policy() {
    for (key, expected) in [
        ("2", 0),
        ("3", 0x1b),
        ("4", 0x1c),
        ("5", 0x1d),
        ("6", 0x1e),
        ("7", 0x1f),
        ("8", 0x7f),
    ] {
        assert_eq!(
            bytes(&character(key, Modifiers::CTRL, None), TermMode::empty()),
            [expected]
        );
    }
    assert_eq!(
        bytes(
            &character("1", Modifiers::CTRL, Some("1")),
            TermMode::empty()
        ),
        b"1"
    );
}

#[test]
fn clipboard_shortcuts_do_not_steal_ctrl_c_and_logo_fallback_is_silent() {
    let bindings = BindingsLayout::new();
    let shortcut = if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL | Modifiers::SHIFT
    };
    assert!(matches!(
        action(
            &character("c", shortcut, Some("c")),
            &bindings,
            TermMode::empty()
        ),
        KeyAction::Copy
    ));
    assert!(matches!(
        action(
            &character("v", shortcut, Some("v")),
            &bindings,
            TermMode::empty()
        ),
        KeyAction::Paste
    ));
    assert_eq!(
        bytes(
            &character("c", Modifiers::CTRL, Some("\x03")),
            TermMode::empty()
        ),
        b"\x03"
    );
    assert!(matches!(
        action(
            &character("x", Modifiers::LOGO, Some("x")),
            &bindings,
            TermMode::empty()
        ),
        KeyAction::Unhandled
    ));
}

#[test]
fn command_h_remains_available_to_the_host_without_terminal_bytes() {
    for text in [None, Some("h")] {
        assert!(matches!(
            action(
                &character("h", Modifiers::LOGO, text),
                &BindingsLayout::new(),
                TermMode::empty()
            ),
            KeyAction::Unhandled
        ));
    }
}

#[test]
fn return_obeys_newline_mode_but_ctrl_m_remains_carriage_return() {
    for cursor in [TermMode::empty(), TermMode::APP_CURSOR] {
        for (newline, expected) in [
            (TermMode::empty(), b"\r".as_slice()),
            (TermMode::LINE_FEED_NEW_LINE, b"\r\n".as_slice()),
        ] {
            let mode = cursor | newline;
            for modifiers in
                [Modifiers::empty(), Modifiers::SHIFT, Modifiers::CTRL]
            {
                assert_eq!(
                    bytes(&named(Named::Enter, modifiers), mode),
                    expected
                );
                let mut alt_expected = vec![0x1b];
                alt_expected.extend_from_slice(expected);
                assert_eq!(
                    bytes(
                        &named(Named::Enter, modifiers | Modifiers::ALT),
                        mode
                    ),
                    alt_expected
                );
            }
            assert_eq!(
                bytes(&character("m", Modifiers::CTRL, None), mode),
                b"\r"
            );
        }
    }
}

#[test]
fn host_delegation_keeps_native_bytes_and_restores_defaults_without_losing_overrides(
) {
    use crate::bindings::{Binding, InputKind};
    let mut bindings = BindingsLayout::new();
    let cs = Modifiers::CTRL | Modifiers::SHIFT;
    let custom = Binding {
        target: InputKind::Char("x".into()),
        modifiers: Modifiers::empty(),
        terminal_mode_include: TermMode::empty(),
        terminal_mode_exclude: TermMode::empty(),
    };
    bindings.add_bindings(vec![(custom.clone(), BindingAction::Ignore)]);
    // A later broad binding must remain newer than the earlier mode override.
    bindings.add_bindings(vec![(
        Binding {
            terminal_mode_include: TermMode::APP_CURSOR,
            ..custom.clone()
        },
        BindingAction::Char('!'),
    )]);
    bindings.add_bindings(vec![(custom, BindingAction::Ignore)]);
    for enabled in [true, true, false, false, true] {
        bindings.set_host_shortcuts(enabled);
        for mode in [
            TermMode::empty(),
            TermMode::ALT_SCREEN,
            TermMode::APP_CURSOR,
        ] {
            assert!(matches!(
                action(
                    &character("x", Modifiers::empty(), Some("x")),
                    &bindings,
                    mode
                ),
                KeyAction::Consume
            ));
        }
        for (key, code) in [("c", 3), ("v", 22)] {
            let event = character(key, cs, None);
            if enabled {
                assert!(
                    matches!(action(&event, &bindings, TermMode::empty()), KeyAction::Write { bytes, alt_prefix: false } if bytes.as_ref() == [code])
                );
            } else if key == "c" {
                assert!(matches!(
                    action(&event, &bindings, TermMode::empty()),
                    KeyAction::Copy
                ));
            } else {
                assert!(matches!(
                    action(&event, &bindings, TermMode::empty()),
                    KeyAction::Paste
                ));
            }
        }
        for (key, scroll, encoded) in [
            (
                Named::PageUp,
                crate::HistoryScroll::PageUp,
                b"\x1b[5;2~".as_slice(),
            ),
            (
                Named::PageDown,
                crate::HistoryScroll::PageDown,
                b"\x1b[6;2~".as_slice(),
            ),
            (
                Named::Home,
                crate::HistoryScroll::Top,
                b"\x1b[1;2H".as_slice(),
            ),
            (
                Named::End,
                crate::HistoryScroll::Bottom,
                b"\x1b[1;2F".as_slice(),
            ),
        ] {
            for mode in [TermMode::empty(), TermMode::ALT_SCREEN] {
                let event = named(key, Modifiers::SHIFT);
                if !enabled && mode.is_empty() {
                    assert!(
                        matches!(action(&event, &bindings, mode), KeyAction::ScrollHistory(actual) if actual == scroll)
                    );
                } else {
                    assert!(
                        matches!(action(&event, &bindings, mode), KeyAction::Write { bytes, alt_prefix: false } if bytes.as_ref() == encoded)
                    );
                }
            }
        }
        assert_eq!(
            matches!(
                action(
                    &named(Named::Insert, Modifiers::SHIFT),
                    &bindings,
                    TermMode::empty()
                ),
                KeyAction::Paste
            ),
            !enabled
        );
    }
}

#[test]
fn named_space_and_unidentified_commits_preserve_dead_key_and_altgr_text() {
    for (key, modifiers, text) in [
        (Key::Named(Named::Space), Modifiers::empty(), "´"),
        (
            Key::Named(Named::Space),
            Modifiers::CTRL | Modifiers::ALT,
            "\u{a0}",
        ),
        (Key::Unidentified, Modifiers::empty(), "é"),
    ] {
        let mut event = named(Named::Space, modifiers);
        if let Event::KeyPressed {
            key: logical,
            modified_key,
            text: committed,
            ..
        } = &mut event
        {
            *logical = key.clone();
            *modified_key = key;
            *committed = Some(text.into());
        }
        assert_eq!(bytes(&event, TermMode::empty()), text.as_bytes());
    }
    let mut event = named(Named::ArrowLeft, Modifiers::CTRL | Modifiers::ALT);
    if let Event::KeyPressed { text, .. } = &mut event {
        *text = Some("x".into());
    }
    assert_eq!(bytes(&event, TermMode::empty()), b"\x1b[1;7D");
}
