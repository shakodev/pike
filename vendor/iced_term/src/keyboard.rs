//! Legacy VT/xterm key encoding. No negotiated extended keyboard protocol.
use crate::bindings::{BindingAction, BindingsLayout};
use alacritty_terminal::term::TermMode;
use iced_core::keyboard::{key::Named, Event, Key, Modifiers};
use std::borrow::Cow;

pub(crate) enum KeyAction<'a> {
    Write {
        bytes: Cow<'a, [u8]>,
        alt_prefix: bool,
    },
    Copy,
    Paste,
    Consume,
    Unhandled,
}

impl<'a> KeyAction<'a> {
    fn text(text: &'a str, alt_prefix: bool) -> Self {
        Self::Write {
            bytes: Cow::Borrowed(text.as_bytes()),
            alt_prefix,
        }
    }
    fn bytes(bytes: &'static [u8], alt_prefix: bool) -> Self {
        Self::Write {
            bytes: Cow::Borrowed(bytes),
            alt_prefix,
        }
    }
    fn sequence(sequence: String) -> Self {
        Self::Write {
            bytes: Cow::Owned(sequence.into_bytes()),
            alt_prefix: false,
        }
    }
}

pub(crate) fn action<'a>(
    event: &'a Event,
    bindings: &'a BindingsLayout,
    mode: TermMode,
) -> KeyAction<'a> {
    let Event::KeyPressed {
        key,
        modified_key,
        modifiers,
        text,
        ..
    } = event
    else {
        return KeyAction::Unhandled;
    };
    if let Some(binding) = bindings.key_action(key, *modifiers, mode) {
        return match binding {
            BindingAction::Ignore | BindingAction::LinkOpen => {
                KeyAction::Consume
            },
            BindingAction::Copy => KeyAction::Copy,
            BindingAction::Paste => KeyAction::Paste,
            BindingAction::Esc(sequence) => KeyAction::text(sequence, false),
            BindingAction::Char(c) => {
                let mut utf8 = [0; 4];
                KeyAction::sequence(c.encode_utf8(&mut utf8).to_owned())
            },
        };
    }
    // Unbound OS/application shortcuts must never become terminal text.
    if modifiers.logo() {
        return KeyAction::Unhandled;
    }
    let text = text
        .as_ref()
        .map(|text| text.as_str())
        .filter(|text| !text.is_empty());
    // Iced exposes no side-specific AltGr modifier. Trust committed text for
    // Ctrl+Alt (and non-ASCII Option text) rather than inventing a Meta prefix.
    if modifiers.alt() {
        if let Some(text) =
            text.filter(|text| text.chars().all(|c| !c.is_control()))
        {
            if modifiers.control()
                || !text.is_ascii()
                || text.chars().nth(1).is_some()
            {
                return KeyAction::text(text, false);
            }
        }
    }
    if let Key::Named(named) = key {
        return named_action(*named, *modifiers, mode);
    }
    // Dead keys have no committed text. Do not manufacture input from their
    // unmodified base key; the later committed text is sent by its own event.
    if matches!(modified_key, Key::Unidentified) && text.is_none() {
        return KeyAction::Consume;
    }
    if modifiers.control() {
        if let Some(code) = control_code(modified_key)
            .or_else(|| control_code(key))
            .or_else(|| {
                // Preserve an OS-provided C0 code on a non-Latin layout.
                let [byte] = text?.as_bytes() else {
                    return None;
                };
                (*byte <= 0x1f || *byte == 0x7f).then_some(*byte)
            })
        {
            let bytes = if modifiers.alt() {
                vec![0x1b, code]
            } else {
                vec![code]
            };
            return KeyAction::Write {
                bytes: Cow::Owned(bytes),
                alt_prefix: false,
            };
        }
        return text
            .map(|text| KeyAction::text(text, modifiers.alt()))
            .unwrap_or(KeyAction::Unhandled);
    }
    if let Some(text) = text {
        return KeyAction::text(text, modifiers.alt());
    }
    // Alt+letter may have no text on the platform. It still has a logical key.
    if modifiers.alt() {
        if let Key::Character(text) = modified_key {
            return KeyAction::text(text.as_str(), true);
        }
    }
    KeyAction::Unhandled
}

fn control_code(key: &Key) -> Option<u8> {
    let Key::Character(text) = key else {
        return None;
    };
    let [byte] = text.as_bytes() else {
        return None;
    };
    Some(match byte.to_ascii_lowercase() {
        b'a'..=b'z' => byte.to_ascii_uppercase() - b'@',
        b' ' | b'@' | b'2' => 0,
        b'[' | b'3' => 0x1b,
        b'\\' | b'4' => 0x1c,
        b']' | b'5' => 0x1d,
        b'^' | b'6' => 0x1e,
        b'_' | b'-' | b'/' | b'7' => 0x1f,
        b'?' | b'8' => 0x7f,
        _ => return None,
    })
}

fn named_action(
    named: Named,
    modifiers: Modifiers,
    mode: TermMode,
) -> KeyAction<'static> {
    use Named::*;
    let alt = modifiers.alt();
    match named {
        Enter => return KeyAction::bytes(b"\r", alt),
        Escape => return KeyAction::bytes(b"\x1b", alt),
        Backspace => {
            return KeyAction::bytes(
                if modifiers.control() {
                    b"\x08"
                } else {
                    b"\x7f"
                },
                alt,
            )
        },
        Tab => {
            return KeyAction::bytes(
                if modifiers.shift() { b"\x1b[Z" } else { b"\t" },
                alt,
            )
        },
        Space => {
            return KeyAction::bytes(
                if modifiers.control() { b"\0" } else { b" " },
                alt,
            )
        },
        _ => {},
    }
    let parameter = 1
        + u8::from(modifiers.shift())
        + 2 * u8::from(alt)
        + 4 * u8::from(modifiers.control());
    let cursor = match named {
        ArrowUp => Some('A'),
        ArrowDown => Some('B'),
        ArrowRight => Some('C'),
        ArrowLeft => Some('D'),
        Home => Some('H'),
        End => Some('F'),
        _ => None,
    };
    if let Some(final_byte) = cursor {
        return if parameter == 1 {
            let prefix = if mode.contains(TermMode::APP_CURSOR) {
                'O'
            } else {
                '['
            };
            KeyAction::sequence(format!("\x1b{prefix}{final_byte}"))
        } else {
            KeyAction::sequence(format!("\x1b[1;{parameter}{final_byte}"))
        };
    }
    let function = match named {
        F1 => Some('P'),
        F2 => Some('Q'),
        F3 => Some('R'),
        F4 => Some('S'),
        _ => None,
    };
    if let Some(final_byte) = function {
        return if parameter == 1 {
            KeyAction::sequence(format!("\x1bO{final_byte}"))
        } else {
            KeyAction::sequence(format!("\x1b[1;{parameter}{final_byte}"))
        };
    }
    let number = match named {
        Insert => 2,
        Delete => 3,
        PageUp => 5,
        PageDown => 6,
        F5 => 15,
        F6 => 17,
        F7 => 18,
        F8 => 19,
        F9 => 20,
        F10 => 21,
        F11 => 23,
        F12 => 24,
        F13 => 25,
        F14 => 26,
        F15 => 28,
        F16 => 29,
        F17 => 31,
        F18 => 32,
        F19 => 33,
        F20 => 34,
        _ => return KeyAction::Unhandled,
    };
    if parameter == 1 {
        KeyAction::sequence(format!("\x1b[{number}~"))
    } else {
        KeyAction::sequence(format!("\x1b[{number};{parameter}~"))
    }
}

#[cfg(test)]
mod tests;
