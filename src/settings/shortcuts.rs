//! One bounded effective binding catalog for validation, routing and reference.
use super::Error;
use serde::{Deserialize, Serialize};

macro_rules! commands {
    ($( $name:ident => ($label:literal, [$($key:literal : $mask:expr),*]) ),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum Command { $($name),* }
        impl Command {
            pub const ALL: &'static [Self] = &[$(Self::$name),*];
            pub fn label(self) -> &'static str { match self { $(Self::$name => $label),* } }
            fn defaults(self) -> &'static [(&'static str, u8)] {
                match self { $(Self::$name => &[$(($key, $mask)),*]),* }
            }
        }
    }
}
pub const CTRL: u8 = 1;
pub const SHIFT: u8 = 2;
pub const ALT: u8 = 4;
pub const LOGO: u8 = 8;
const CS: u8 = CTRL | SHIFT;
const AS: u8 = ALT | SHIFT;
commands! {
    NewTab => ("New terminal / tab", ["t": CS]),
    ClosePane => ("Close pane", ["w": CS]),
    CloseTab => ("Close tab", ["q": CS]),
    CloseWindow => ("Close window", []),
    NextTab => ("Next tab", ["tab": CTRL]),
    PreviousTab => ("Previous tab", ["tab": CS]),
    Tab1 => ("Select tab 1", ["1": CS]),
    Tab2 => ("Select tab 2", ["2": CS]),
    Tab3 => ("Select tab 3", ["3": CS]),
    Tab4 => ("Select tab 4", ["4": CS]),
    Tab5 => ("Select tab 5", ["5": CS]),
    Tab6 => ("Select tab 6", ["6": CS]),
    Tab7 => ("Select tab 7", ["7": CS]),
    Tab8 => ("Select tab 8", ["8": CS]),
    LastTab => ("Select last tab", ["9": CS]),
    SplitRight => ("Split right", ["o": CS]),
    SplitDown => ("Split down", ["e": CS]),
    FocusLeft => ("Focus pane left", ["left": ALT]),
    FocusRight => ("Focus pane right", ["right": ALT]),
    FocusUp => ("Focus pane above", ["up": ALT]),
    FocusDown => ("Focus pane below", ["down": ALT]),
    ResizeLeft => ("Resize divider left", ["left": AS]),
    ResizeRight => ("Resize divider right", ["right": AS]),
    ResizeUp => ("Resize divider up", ["up": AS]),
    ResizeDown => ("Resize divider down", ["down": AS]),
    MovePaneLeft => ("Move pane left", []),
    MovePaneRight => ("Move pane right", []),
    MovePaneUp => ("Move pane up", []),
    MovePaneDown => ("Move pane down", []),
    ToggleZoom => ("Maximize / restore pane", ["enter": CS]),
    FontLarger => ("Larger terminal font", ["+": CS]),
    FontSmaller => ("Smaller terminal font", ["-": CS]),
    FontReset => ("Reset terminal font", ["0": CS]),
    Help => ("Commands and shortcuts", ["f1": 0]),
    RenameTab => ("Rename tab", ["r": CS]),
    ResetTabName => ("Restore automatic tab name", []),
    MoveTabLeft => ("Move tab left", []),
    MoveTabRight => ("Move tab right", []),
    ListTabs => ("All tabs", ["l": CS]),
    ReloadSettings => ("Reload settings", ["f5": CS]),
    Retry => ("Restart / retry terminal", []),
    UseDefault => ("Retry with default shell", []),
    Copy => ("Copy selection", ["c": CS, "c": LOGO]),
    Paste => ("Paste clipboard", ["v": CS, "insert": SHIFT, "v": LOGO]),
    HistoryPageUp => ("History page up", ["pageup": SHIFT]),
    HistoryPageDown => ("History page down", ["pagedown": SHIFT]),
    HistoryTop => ("History beginning", ["home": SHIFT]),
    HistoryBottom => ("History end", ["end": SHIFT]),
    Profile1 => ("New terminal with profile 1", []),
    Profile2 => ("New terminal with profile 2", []),
    Profile3 => ("New terminal with profile 3", []),
    Profile4 => ("New terminal with profile 4", []),
    Profile5 => ("New terminal with profile 5", []),
    Profile6 => ("New terminal with profile 6", []),
    Profile7 => ("New terminal with profile 7", []),
    Profile8 => ("New terminal with profile 8", []),
    Profile9 => ("New terminal with profile 9", []),
    Profile10 => ("New terminal with profile 10", []),
    Profile11 => ("New terminal with profile 11", []),
    Profile12 => ("New terminal with profile 12", []),
    Profile13 => ("New terminal with profile 13", []),
    Profile14 => ("New terminal with profile 14", []),
    Profile15 => ("New terminal with profile 15", []),
    Profile16 => ("New terminal with profile 16", []),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
}
impl Modifier {
    fn mask(self) -> u8 {
        match self {
            Self::Ctrl => CTRL,
            Self::Shift => SHIFT,
            Self::Alt => ALT,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Key(String);
impl TryFrom<String> for Key {
    type Error = &'static str;
    fn try_from(key: String) -> Result<Self, Self::Error> {
        let key = key.to_lowercase();
        let function = key
            .strip_prefix('f')
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| (1..=12).contains(n));
        // Validation follows deserialization, yielding a useful indexed error.
        Ok(Self(if let Some(number) = function {
            format!("f{number}")
        } else if key == "=" {
            "+".into()
        } else {
            key
        }))
    }
}
impl From<Key> for String {
    fn from(key: Key) -> Self {
        key.0
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shortcut {
    pub command: Command,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<Key>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<Modifier>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unbind: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    Limit,
    Unsupported,
    Modifiers,
    Duplicate,
    Unbind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding<'a> {
    pub command: Command,
    pub key: &'a str,
    pub modifiers: u8,
}
impl std::fmt::Display for Binding<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (mask, name) in [
            (CTRL, "Ctrl+"),
            (ALT, "Alt+"),
            (SHIFT, "Shift+"),
            (LOGO, "Cmd+"),
        ] {
            if self.modifiers & mask != 0 {
                f.write_str(name)?;
            }
        }
        f.write_str(&self.key.to_uppercase())
    }
}
/// Configured entries replace all defaults for their command. Multiple entries
/// add alternatives; an explicit unbind leaves the command available in the menu.
pub fn effective(shortcuts: &[Shortcut]) -> impl Iterator<Item = Binding<'_>> {
    let defaults = Command::ALL
        .iter()
        .copied()
        .filter(move |command| !shortcuts.iter().any(|s| s.command == *command))
        .flat_map(|command| {
            command
                .defaults()
                .iter()
                .map(move |(key, modifiers)| Binding {
                    command,
                    key,
                    modifiers: *modifiers,
                })
        });
    let overrides = shortcuts.iter().filter_map(|s| {
        s.key.as_ref().filter(|_| !s.unbind).map(|key| Binding {
            command: s.command,
            key: &key.0,
            modifiers: s.modifiers.iter().fold(0, |bits, m| bits | m.mask()),
        })
    });
    defaults
        .filter(|b| b.modifiers != LOGO || cfg!(target_os = "macos"))
        .chain(overrides)
}
fn supported(key: &str) -> bool {
    (key.chars().count() == 1 && key.chars().all(|c| !c.is_control() && !c.is_whitespace()))
        || matches!(
            key,
            "tab"
                | "enter"
                | "left"
                | "right"
                | "up"
                | "down"
                | "insert"
                | "home"
                | "end"
                | "pageup"
                | "pagedown"
        )
        || key
            .strip_prefix('f')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=12).contains(&n))
}
pub(super) fn validate(shortcuts: &[Shortcut]) -> Result<(), Error> {
    let error = |index: usize, problem| Error::Shortcut {
        entry: (index + 1).min(255) as u8,
        problem,
    };
    if shortcuts.len() > 64 {
        return Err(error(64, Problem::Limit));
    }
    for (index, shortcut) in shortcuts.iter().enumerate() {
        if shortcut.unbind {
            if shortcut.key.is_some()
                || !shortcut.modifiers.is_empty()
                || shortcuts
                    .iter()
                    .filter(|s| s.command == shortcut.command)
                    .count()
                    != 1
            {
                return Err(error(index, Problem::Unbind));
            }
            continue;
        }
        let Some(key) = shortcut.key.as_ref().filter(|key| supported(&key.0)) else {
            return Err(error(index, Problem::Unsupported));
        };
        let mask = shortcut.modifiers.iter().fold(0, |bits, m| bits | m.mask());
        let arrow = matches!(key.0.as_str(), "left" | "right" | "up" | "down");
        let safe = mask == CS
            || (arrow && (mask == ALT || mask == AS))
            || (key.0 == "tab" && mask == CTRL)
            || (key.0 == "f1" && mask == 0)
            || (matches!(
                key.0.as_str(),
                "insert" | "pageup" | "pagedown" | "home" | "end"
            ) && mask == SHIFT);
        if !safe || mask.count_ones() as usize != shortcut.modifiers.len() {
            return Err(error(index, Problem::Modifiers));
        }
    }
    let mut chords = std::collections::BTreeSet::new();
    for binding in effective(shortcuts) {
        if !chords.insert((binding.modifiers, binding.key)) {
            let index = shortcuts
                .iter()
                .position(|s| s.command == binding.command)
                .unwrap_or(0);
            return Err(error(index, Problem::Duplicate));
        }
    }
    Ok(())
}
