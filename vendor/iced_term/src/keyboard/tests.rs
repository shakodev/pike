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
            bytes(&named(key, modifiers), TermMode::empty()),
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
