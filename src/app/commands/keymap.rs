//! Logical-key matching only; the terminal remains the sole native byte encoder.
use super::{Command, FontChange, Resolution, TabMove, TabSelection};
use crate::app::{Direction, ResizeStep};
use crate::settings::{
    Shortcut,
    shortcuts::{self, Command as Id},
};
use iced::{
    keyboard::{self, Key, key::Named},
    widget::pane_grid,
};
use iced_term::HistoryScroll;

pub(super) fn resolve(event: &keyboard::Event, mappings: &[Shortcut]) -> Option<Resolution> {
    let keyboard::Event::KeyPressed {
        key,
        modified_key,
        modifiers,
        repeat,
        ..
    } = event
    else {
        return None;
    };
    // Ctrl+Alt (including shifted AltGr) and Logo never enter this application
    // map. Native text/IME processing retains its established precedence.
    if modifiers.control() && modifiers.alt() {
        return None;
    }
    let mask = (u8::from(modifiers.control()) * shortcuts::CTRL)
        | (u8::from(modifiers.shift()) * shortcuts::SHIFT)
        | (u8::from(modifiers.alt()) * shortcuts::ALT)
        | (u8::from(modifiers.logo()) * shortcuts::LOGO);
    // Prefer the actual shifted symbol if configured. Fall back to the logical
    // base for established digit/letter shortcuts; never use physical positions.
    let binding = [modified_key, key].into_iter().find_map(|key| {
        shortcuts::effective(mappings).find(|b| b.modifiers == mask && matches_key(b.key, key))
    })?;
    let command = command(binding.command);
    Some(if *repeat && command.suppresses_repeat() {
        Resolution::Consume
    } else {
        Resolution::Dispatch(command)
    })
}

fn matches_key(expected: &str, key: &Key) -> bool {
    match key {
        Key::Character(value) => {
            value.eq_ignore_ascii_case(expected)
                || (expected == "+" && value == "=")
                || value
                    .chars()
                    .flat_map(char::to_lowercase)
                    .eq(expected.chars())
        }
        Key::Named(key) => matches!(
            (expected, key),
            ("tab", Named::Tab)
                | ("enter", Named::Enter)
                | ("left", Named::ArrowLeft)
                | ("right", Named::ArrowRight)
                | ("up", Named::ArrowUp)
                | ("down", Named::ArrowDown)
                | ("insert", Named::Insert)
                | ("home", Named::Home)
                | ("end", Named::End)
                | ("pageup", Named::PageUp)
                | ("pagedown", Named::PageDown)
                | ("f1", Named::F1)
                | ("f2", Named::F2)
                | ("f3", Named::F3)
                | ("f4", Named::F4)
                | ("f5", Named::F5)
                | ("f6", Named::F6)
                | ("f7", Named::F7)
                | ("f8", Named::F8)
                | ("f9", Named::F9)
                | ("f10", Named::F10)
                | ("f11", Named::F11)
                | ("f12", Named::F12)
        ),
        Key::Unidentified => false,
    }
}

pub(super) fn command(id: Id) -> Command {
    use pane_grid::Direction as D;
    match id {
        Id::NewTab => Command::NewTab,
        Id::ClosePane => Command::ClosePane,
        Id::CloseTab => Command::CloseTab,
        Id::CloseWindow => Command::CloseWindow,
        Id::NextTab => Command::NextTab,
        Id::PreviousTab => Command::PreviousTab,
        Id::Tab1 => Command::SelectTab(TabSelection::Index(0)),
        Id::Tab2 => Command::SelectTab(TabSelection::Index(1)),
        Id::Tab3 => Command::SelectTab(TabSelection::Index(2)),
        Id::Tab4 => Command::SelectTab(TabSelection::Index(3)),
        Id::Tab5 => Command::SelectTab(TabSelection::Index(4)),
        Id::Tab6 => Command::SelectTab(TabSelection::Index(5)),
        Id::Tab7 => Command::SelectTab(TabSelection::Index(6)),
        Id::Tab8 => Command::SelectTab(TabSelection::Index(7)),
        Id::LastTab => Command::SelectTab(TabSelection::Last),
        Id::SplitRight => Command::Split(Direction::Right),
        Id::SplitDown => Command::Split(Direction::Down),
        Id::FocusLeft => Command::Focus(D::Left),
        Id::FocusRight => Command::Focus(D::Right),
        Id::FocusUp => Command::Focus(D::Up),
        Id::FocusDown => Command::Focus(D::Down),
        Id::ResizeLeft => Command::Resize(ResizeStep::Left),
        Id::ResizeRight => Command::Resize(ResizeStep::Right),
        Id::ResizeUp => Command::Resize(ResizeStep::Up),
        Id::ResizeDown => Command::Resize(ResizeStep::Down),
        Id::MovePaneLeft => Command::MovePane(D::Left),
        Id::MovePaneRight => Command::MovePane(D::Right),
        Id::MovePaneUp => Command::MovePane(D::Up),
        Id::MovePaneDown => Command::MovePane(D::Down),
        Id::ToggleZoom => Command::ToggleZoom,
        Id::FontLarger => Command::Font(FontChange::Larger),
        Id::FontSmaller => Command::Font(FontChange::Smaller),
        Id::FontReset => Command::Font(FontChange::Reset),
        Id::Help => Command::Help,
        Id::RenameTab => Command::RenameTab,
        Id::ResetTabName => Command::ResetTabName,
        Id::MoveTabLeft => Command::MoveTab(TabMove::Left),
        Id::MoveTabRight => Command::MoveTab(TabMove::Right),
        Id::ListTabs => Command::ListTabs,
        Id::ReloadSettings => Command::ReloadSettings,
        Id::Retry => Command::Retry,
        Id::UseDefault => Command::UseDefault,
        Id::Copy => Command::Copy,
        Id::Paste => Command::Paste,
        Id::HistoryPageUp => Command::History(HistoryScroll::PageUp),
        Id::HistoryPageDown => Command::History(HistoryScroll::PageDown),
        Id::HistoryTop => Command::History(HistoryScroll::Top),
        Id::HistoryBottom => Command::History(HistoryScroll::Bottom),
        Id::Profile1 => Command::NewProfile(0),
        Id::Profile2 => Command::NewProfile(1),
        Id::Profile3 => Command::NewProfile(2),
        Id::Profile4 => Command::NewProfile(3),
        Id::Profile5 => Command::NewProfile(4),
        Id::Profile6 => Command::NewProfile(5),
        Id::Profile7 => Command::NewProfile(6),
        Id::Profile8 => Command::NewProfile(7),
        Id::Profile9 => Command::NewProfile(8),
        Id::Profile10 => Command::NewProfile(9),
        Id::Profile11 => Command::NewProfile(10),
        Id::Profile12 => Command::NewProfile(11),
        Id::Profile13 => Command::NewProfile(12),
        Id::Profile14 => Command::NewProfile(13),
        Id::Profile15 => Command::NewProfile(14),
        Id::Profile16 => Command::NewProfile(15),
    }
}

#[cfg(test)]
mod tests;
