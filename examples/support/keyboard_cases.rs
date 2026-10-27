//! Literal byte oracles for the widget/PTY probe, independent of its encoder.
use iced::keyboard::{
    Event, Key, Location, Modifiers as M,
    key::{self, Named},
};

pub struct Case {
    pub event: Event,
    pub expected: Vec<u8>,
    pub captured: bool,
}

pub fn character(key: &str, modifiers: M, text: Option<&str>) -> Event {
    Event::KeyPressed {
        key: Key::Character(key.into()),
        modified_key: Key::Character(key.into()),
        physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
        location: Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    }
}

pub fn named(key: Named, modifiers: M) -> Event {
    Event::KeyPressed {
        key: Key::Named(key),
        modified_key: Key::Named(key),
        physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
        location: Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    }
}

pub fn cases(application: bool, newline: bool) -> Vec<Case> {
    let mut cases = Vec::new();
    for (key, expected) in (b'a'..=b'z').zip(1_u8..=26) {
        cases.push(Case {
            event: character(&char::from(key).to_string(), M::CTRL, None),
            expected: vec![expected],
            captured: true,
        });
    }
    for (key, normal, app) in [
        (Named::ArrowUp, "\x1b[A", "\x1bOA"),
        (Named::ArrowDown, "\x1b[B", "\x1bOB"),
        (Named::ArrowRight, "\x1b[C", "\x1bOC"),
        (Named::ArrowLeft, "\x1b[D", "\x1bOD"),
        (Named::Home, "\x1b[H", "\x1bOH"),
        (Named::End, "\x1b[F", "\x1bOF"),
        (Named::Insert, "\x1b[2~", "\x1b[2~"),
        (Named::Delete, "\x1b[3~", "\x1b[3~"),
        (Named::PageUp, "\x1b[5~", "\x1b[5~"),
        (Named::PageDown, "\x1b[6~", "\x1b[6~"),
        (Named::F1, "\x1bOP", "\x1bOP"),
        (Named::F2, "\x1bOQ", "\x1bOQ"),
        (Named::F3, "\x1bOR", "\x1bOR"),
        (Named::F4, "\x1bOS", "\x1bOS"),
        (Named::F5, "\x1b[15~", "\x1b[15~"),
        (Named::F6, "\x1b[17~", "\x1b[17~"),
        (Named::F7, "\x1b[18~", "\x1b[18~"),
        (Named::F8, "\x1b[19~", "\x1b[19~"),
        (Named::F9, "\x1b[20~", "\x1b[20~"),
        (Named::F10, "\x1b[21~", "\x1b[21~"),
        (Named::F11, "\x1b[23~", "\x1b[23~"),
        (Named::F12, "\x1b[24~", "\x1b[24~"),
        (Named::F13, "\x1b[25~", "\x1b[25~"),
        (Named::F14, "\x1b[26~", "\x1b[26~"),
        (Named::F15, "\x1b[28~", "\x1b[28~"),
        (Named::F16, "\x1b[29~", "\x1b[29~"),
        (Named::F17, "\x1b[31~", "\x1b[31~"),
        (Named::F18, "\x1b[32~", "\x1b[32~"),
        (Named::F19, "\x1b[33~", "\x1b[33~"),
        (Named::F20, "\x1b[34~", "\x1b[34~"),
    ] {
        cases.push(Case {
            event: named(key, M::empty()),
            expected: if application { app } else { normal }.as_bytes().to_vec(),
            captured: true,
        });
    }
    for (modifiers, expected) in [
        (M::SHIFT, "\x1b[1;2A"),
        (M::ALT, "\x1b[1;3A"),
        (M::SHIFT | M::ALT, "\x1b[1;4A"),
        (M::CTRL, "\x1b[1;5A"),
        (M::SHIFT | M::CTRL, "\x1b[1;6A"),
        (M::CTRL | M::ALT, "\x1b[1;7A"),
        (M::SHIFT | M::CTRL | M::ALT, "\x1b[1;8A"),
    ] {
        cases.push(Case {
            event: named(Named::ArrowUp, modifiers),
            expected: expected.as_bytes().to_vec(),
            captured: true,
        });
    }
    for (key, modifiers, expected) in [
        (Named::F1, M::CTRL, "\x1b[1;5P"),
        (Named::F2, M::ALT, "\x1b[1;3Q"),
        (Named::F3, M::SHIFT, "\x1b[1;2R"),
        (Named::F4, M::CTRL | M::SHIFT, "\x1b[1;6S"),
        (Named::Insert, M::ALT, "\x1b[2;3~"),
        (Named::Delete, M::CTRL, "\x1b[3;5~"),
        (Named::PageDown, M::CTRL | M::ALT, "\x1b[6;7~"),
        (
            Named::Enter,
            M::empty(),
            if newline { "\r\n" } else { "\r" },
        ),
        (
            Named::Enter,
            M::ALT,
            if newline { "\x1b\r\n" } else { "\x1b\r" },
        ),
        (Named::Backspace, M::empty(), "\x7f"),
        (Named::Backspace, M::CTRL, "\x08"),
        (Named::Tab, M::SHIFT, "\x1b[Z"),
        (Named::Escape, M::empty(), "\x1b"),
    ] {
        cases.push(Case {
            event: named(key, modifiers),
            expected: expected.as_bytes().to_vec(),
            captured: true,
        });
    }
    for (key, modifiers, text, expected) in [
        ("u", M::CTRL | M::SHIFT, None, "\x15"),
        ("b", M::ALT, Some("b"), "\x1bb"),
        ("u", M::CTRL | M::ALT, None, "\x1b\x15"),
        ("q", M::CTRL | M::ALT, Some("@"), "@"),
        ("e", M::CTRL | M::ALT, Some("€"), "€"),
        ("e", M::ALT, Some("é"), "é"),
        ("e", M::empty(), Some("e\u{301}"), "e\u{301}"),
        ("[", M::CTRL, None, "\x1b"),
        ("\\", M::CTRL, None, "\x1c"),
        ("]", M::CTRL, None, "\x1d"),
        ("^", M::CTRL, None, "\x1e"),
        ("_", M::CTRL, None, "\x1f"),
        ("@", M::CTRL, None, "\0"),
        ("2", M::CTRL, None, "\0"),
        ("3", M::CTRL, None, "\x1b"),
        ("4", M::CTRL, None, "\x1c"),
        ("5", M::CTRL, None, "\x1d"),
        ("6", M::CTRL, None, "\x1e"),
        ("7", M::CTRL, None, "\x1f"),
        ("8", M::CTRL, None, "\x7f"),
        ("1", M::CTRL, Some("1"), "1"),
    ] {
        cases.push(Case {
            event: character(key, modifiers, text),
            expected: expected.as_bytes().to_vec(),
            captured: true,
        });
    }
    for _ in 0..64 {
        let mut event = character("r", M::empty(), Some("r"));
        if let Event::KeyPressed { repeat, .. } = &mut event {
            *repeat = true;
        }
        cases.push(Case {
            event,
            expected: b"r".to_vec(),
            captured: true,
        });
    }
    // Unbound host shortcuts must produce neither terminal bytes nor capture.
    for text in [None, Some("h")] {
        cases.push(Case {
            event: character("h", M::LOGO, text),
            expected: vec![],
            captured: false,
        });
    }
    // Installed by the probe as an app interception, Copy and empty Paste.
    cases.push(Case {
        event: character("x", M::empty(), Some("x")),
        expected: vec![],
        captured: true,
    });
    for key in [Named::F21, Named::F22] {
        cases.push(Case {
            event: named(key, M::empty()),
            expected: vec![],
            captured: true,
        });
    }
    let mut dead = character("e", M::CTRL | M::ALT, None);
    if let Event::KeyPressed { modified_key, .. } = &mut dead {
        *modified_key = Key::Unidentified;
    }
    cases.push(Case {
        event: dead,
        expected: vec![],
        captured: true,
    });
    cases.push(Case {
        event: Event::KeyReleased {
            key: Key::Character("r".into()),
            modified_key: Key::Character("r".into()),
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: M::empty(),
        },
        expected: vec![],
        captured: false,
    });
    cases
}
