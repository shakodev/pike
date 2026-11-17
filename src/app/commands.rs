//! Pure application shortcut resolution. Native terminal encoding stays in the
//! terminal widget, and the host's single capture wrapper owns IME/modal priority.
use iced::keyboard;
#[cfg(test)]
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::pane_grid;

use super::Direction;

mod host;
mod keymap;
mod reference;
pub use host::PasteRequest;
pub(super) use host::{State, on_key};
pub use reference::{CommandReference, HelpAction};

/// Expected refusals keep the current workspace usable and expose recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NoActivePane,
    TerminalNotReady,
    MissingTab,
    NewTabRefused,
    SplitRefused,
    Workspace(crate::workspace::Error),
}

impl Error {
    pub fn message(self) -> &'static str {
        match self {
            Self::NoActivePane => "Open a terminal from the Commands menu, then try again.",
            Self::TerminalNotReady => "Wait for this terminal to start, or retry its error pane.",
            Self::MissingTab => {
                "That tab is not open. Choose an existing tab or create one from the Commands menu."
            }
            Self::NewTabRefused => {
                "A tab could not be created. Close a pane, wait for cleanup, and try again."
            }
            Self::SplitRefused => {
                "This terminal could not be split. Enlarge the window or wait for startup/cleanup, then try again."
            }
            Self::Workspace(_) => {
                "This pane is no longer available. Select an existing terminal and try again."
            }
        }
    }
}

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
pub enum TabMove {
    Left,
    Right,
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
    RenameTab,
    ListTabs,
    ReloadSettings,
    CloseWindow,
    Resize(super::ResizeStep),
    MovePane(pane_grid::Direction),
    MoveTab(TabMove),
    ResetTabName,
    Retry,
    UseDefault,
    Copy,
    Paste,
    History(iced_term::HistoryScroll),
    NewProfile(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    Dispatch(Command),
    /// A recognized repeat must still be captured, without another UI action.
    Consume,
}

/// Compatibility default resolver; live hosts pass the active validated mappings
/// to resolve_with at the same pre-child capture boundary.
pub fn resolve(event: &keyboard::Event) -> Option<Resolution> {
    resolve_with(event, &[])
}

pub fn resolve_with(
    event: &keyboard::Event,
    shortcuts: &[crate::settings::Shortcut],
) -> Option<Resolution> {
    keymap::resolve(event, shortcuts)
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
                | Self::RenameTab
                | Self::ListTabs
                | Self::ReloadSettings
                | Self::CloseWindow
                | Self::MovePane(_)
                | Self::MoveTab(_)
                | Self::ResetTabName
                | Self::Retry
                | Self::UseDefault
                | Self::Copy
                | Self::Paste
                | Self::NewProfile(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::{Location, key};

    #[test]
    fn reload_is_exact_and_nonrepeating_while_bare_f5_and_terminal_chords_pass_through() {
        let key = Key::Named(Named::F5);
        let modifiers = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(
            resolve(&press(key.clone(), modifiers, false)),
            Some(Resolution::Dispatch(Command::ReloadSettings))
        );
        assert_eq!(
            resolve(&press(key.clone(), modifiers, true)),
            Some(Resolution::Consume)
        );
        assert_eq!(
            resolve(&press(key.clone(), Modifiers::empty(), false)),
            None
        );
        assert_eq!(
            resolve(&press(key, Modifiers::CTRL | Modifiers::ALT, false)),
            None
        );
    }

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
            (Key::Character("r".into()), Command::RenameTab),
            (Key::Character("l".into()), Command::ListTabs),
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
            (Key::Named(Named::F2), Modifiers::empty()),
            (Key::Named(Named::Space), Modifiers::CTRL | Modifiers::ALT),
        ] {
            assert_eq!(resolve(&press(key, modifiers, false)), None);
        }
        for modifiers in [Modifiers::empty(), Modifiers::CTRL | Modifiers::SHIFT] {
            assert_eq!(resolve(&keyboard::Event::ModifiersChanged(modifiers)), None);
        }
    }

    #[test]
    fn directions_and_resize_share_the_typed_resolver_and_repeat() {
        for (key, direction, resize) in [
            (
                Named::ArrowLeft,
                pane_grid::Direction::Left,
                super::super::ResizeStep::Left,
            ),
            (
                Named::ArrowRight,
                pane_grid::Direction::Right,
                super::super::ResizeStep::Right,
            ),
            (
                Named::ArrowUp,
                pane_grid::Direction::Up,
                super::super::ResizeStep::Up,
            ),
            (
                Named::ArrowDown,
                pane_grid::Direction::Down,
                super::super::ResizeStep::Down,
            ),
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
                Some(Resolution::Dispatch(Command::Resize(resize)))
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
