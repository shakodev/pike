//! Pure application shortcut resolution. Terminal editing bindings stay in the
//! terminal widget, and the host's single capture wrapper owns IME/modal priority.
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::widget::pane_grid;

use super::Direction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabSelection {
    /// Zero-based index into the existing workspace order.
    Index(u8),
    Last,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontChange {
    Larger,
    Smaller,
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    NewTab,
    ClosePane,
    CloseTab,
    NextTab,
    PreviousTab,
    SelectTab(TabSelection),
    Split(Direction),
    Focus(pane_grid::Direction),
    ToggleZoom,
    Font(FontChange),
    Help,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    Dispatch(Command),
    /// A recognized repeat must still be captured, without another UI action.
    Consume,
}

/// Resolve against each event's modifiers, never a cached modifier snapshot.
/// Exact modifiers leave Ctrl+C, printable AltGr, dead keys and terminal-owned
/// copy/paste/history bindings on their established encoder paths. Resize keys
/// are resolved by the host's existing resize mapper after this returns None.
pub fn resolve(event: &keyboard::Event) -> Option<Resolution> {
    let keyboard::Event::KeyPressed {
        key,
        modifiers,
        repeat,
        ..
    } = event
    else {
        return None;
    };
    let command = if *modifiers == Modifiers::CTRL | Modifiers::SHIFT {
        match key {
            Key::Character(character) => {
                let [character] = character.as_bytes() else {
                    return None;
                };
                match character.to_ascii_lowercase() {
                    b't' => Command::NewTab,
                    b'w' => Command::ClosePane,
                    b'q' => Command::CloseTab,
                    b'o' => Command::Split(Direction::Right),
                    b'e' => Command::Split(Direction::Down),
                    b'1'..=b'8' => Command::SelectTab(TabSelection::Index(character - b'1')),
                    b'9' => Command::SelectTab(TabSelection::Last),
                    b'+' | b'=' => Command::Font(FontChange::Larger),
                    b'-' => Command::Font(FontChange::Smaller),
                    b'0' => Command::Font(FontChange::Reset),
                    _ => return None,
                }
            }
            Key::Named(Named::Tab) => Command::PreviousTab,
            Key::Named(Named::Enter) => Command::ToggleZoom,
            _ => return None,
        }
    } else if *modifiers == Modifiers::CTRL && *key == Key::Named(Named::Tab) {
        Command::NextTab
    } else if *modifiers == Modifiers::ALT {
        Command::Focus(match key {
            Key::Named(Named::ArrowLeft) => pane_grid::Direction::Left,
            Key::Named(Named::ArrowRight) => pane_grid::Direction::Right,
            Key::Named(Named::ArrowUp) => pane_grid::Direction::Up,
            Key::Named(Named::ArrowDown) => pane_grid::Direction::Down,
            _ => return None,
        })
    } else if modifiers.is_empty() && *key == Key::Named(Named::F1) {
        Command::Help
    } else {
        return None;
    };
    Some(if *repeat && command.suppresses_repeat() {
        Resolution::Consume
    } else {
        Resolution::Dispatch(command)
    })
}

impl Command {
    fn suppresses_repeat(self) -> bool {
        matches!(
            self,
            Self::NewTab
                | Self::ClosePane
                | Self::CloseTab
                | Self::Split(_)
                | Self::ToggleZoom
                | Self::Help
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::{Location, key};

    fn press(key: Key, modifiers: Modifiers, repeat: bool) -> keyboard::Event {
        keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: Location::Standard,
            modifiers,
            text: None,
            repeat,
        }
    }

    #[test]
    fn structural_defaults_dispatch_once_and_capture_repeats() {
        let modifiers = Modifiers::CTRL | Modifiers::SHIFT;
        for (key, expected) in [
            (Key::Character("t".into()), Command::NewTab),
            (Key::Character("W".into()), Command::ClosePane),
            (Key::Character("q".into()), Command::CloseTab),
            (Key::Character("o".into()), Command::Split(Direction::Right)),
            (Key::Character("E".into()), Command::Split(Direction::Down)),
            (Key::Named(Named::Enter), Command::ToggleZoom),
        ] {
            assert_eq!(
                resolve(&press(key.clone(), modifiers, false)),
                Some(Resolution::Dispatch(expected))
            );
            assert_eq!(
                resolve(&press(key, modifiers, true)),
                Some(Resolution::Consume)
            );
        }
        assert_eq!(
            resolve(&press(Key::Named(Named::F1), Modifiers::empty(), true)),
            Some(Resolution::Consume)
        );
    }

    #[test]
    fn ordered_tab_selection_and_navigation_are_typed() {
        for number in 1..=9 {
            let expected = if number == 9 {
                TabSelection::Last
            } else {
                TabSelection::Index(number - 1)
            };
            assert_eq!(
                resolve(&press(
                    Key::Character(number.to_string().into()),
                    Modifiers::CTRL | Modifiers::SHIFT,
                    false
                )),
                Some(Resolution::Dispatch(Command::SelectTab(expected)))
            );
        }
        for (modifiers, expected) in [
            (Modifiers::CTRL, Command::NextTab),
            (Modifiers::CTRL | Modifiers::SHIFT, Command::PreviousTab),
        ] {
            assert_eq!(
                resolve(&press(Key::Named(Named::Tab), modifiers, true)),
                Some(Resolution::Dispatch(expected))
            );
        }
    }

    #[test]
    fn exact_modifiers_leave_altgr_interrupt_and_terminal_defaults_to_child() {
        for (key, modifiers) in [
            (Key::Character("c".into()), Modifiers::CTRL),
            (Key::Character("o".into()), Modifiers::CTRL | Modifiers::ALT),
            (
                Key::Character("o".into()),
                Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT,
            ),
            (
                Key::Character("o".into()),
                Modifiers::CTRL | Modifiers::SHIFT | Modifiers::LOGO,
            ),
            (Key::Character("é".into()), Modifiers::empty()),
            (Key::Unidentified, Modifiers::empty()),
            (Key::Named(Named::Space), Modifiers::CTRL | Modifiers::ALT),
            (
                Key::Character("c".into()),
                Modifiers::CTRL | Modifiers::SHIFT,
            ),
            (
                Key::Character("v".into()),
                Modifiers::CTRL | Modifiers::SHIFT,
            ),
            (Key::Named(Named::Insert), Modifiers::SHIFT),
            (Key::Named(Named::PageUp), Modifiers::SHIFT),
            (Key::Named(Named::PageDown), Modifiers::SHIFT),
            (Key::Named(Named::Home), Modifiers::SHIFT),
            (Key::Named(Named::End), Modifiers::SHIFT),
        ] {
            assert_eq!(resolve(&press(key, modifiers, false)), None);
        }
        for modifiers in [Modifiers::empty(), Modifiers::CTRL | Modifiers::SHIFT] {
            assert_eq!(resolve(&keyboard::Event::ModifiersChanged(modifiers)), None);
        }
    }

    #[test]
    fn directions_repeat_but_leave_resize_chords_to_existing_mapper() {
        for (key, direction) in [
            (Named::ArrowLeft, pane_grid::Direction::Left),
            (Named::ArrowRight, pane_grid::Direction::Right),
            (Named::ArrowUp, pane_grid::Direction::Up),
            (Named::ArrowDown, pane_grid::Direction::Down),
        ] {
            assert_eq!(
                resolve(&press(Key::Named(key), Modifiers::ALT, true)),
                Some(Resolution::Dispatch(Command::Focus(direction)))
            );
            assert_eq!(
                resolve(&press(
                    Key::Named(key),
                    Modifiers::ALT | Modifiers::SHIFT,
                    false
                )),
                None
            );
        }
    }

    #[test]
    fn font_and_help_defaults_and_release_preserve_input_transitions() {
        for (key, change) in [
            ("+", FontChange::Larger),
            ("=", FontChange::Larger),
            ("-", FontChange::Smaller),
            ("0", FontChange::Reset),
        ] {
            assert_eq!(
                resolve(&press(
                    Key::Character(key.into()),
                    Modifiers::CTRL | Modifiers::SHIFT,
                    true
                )),
                Some(Resolution::Dispatch(Command::Font(change)))
            );
        }
        assert_eq!(
            resolve(&press(Key::Named(Named::F1), Modifiers::empty(), false)),
            Some(Resolution::Dispatch(Command::Help))
        );
        let event = keyboard::Event::KeyReleased {
            key: Key::Character("o".into()),
            modified_key: Key::Character("O".into()),
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: Modifiers::CTRL | Modifiers::SHIFT,
        };
        assert_eq!(resolve(&event), None);
        assert_eq!(
            resolve(&press(
                Key::Character("oe".into()),
                Modifiers::CTRL | Modifiers::SHIFT,
                false
            )),
            None
        );
    }
}
