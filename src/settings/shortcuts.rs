//! Typed future-remapping seam. R31 validates and retains this data; the one
//! existing application resolver continues to own dispatch and IME priority.
use super::{Error, Field};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Command {
    NewTab,
    ClosePane,
    CloseTab,
    NextTab,
    PreviousTab,
    Tab1,
    Tab2,
    Tab3,
    Tab4,
    Tab5,
    Tab6,
    Tab7,
    Tab8,
    LastTab,
    SplitRight,
    SplitDown,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    ResizeLeft,
    ResizeRight,
    ResizeUp,
    ResizeDown,
    ToggleZoom,
    FontLarger,
    FontSmaller,
    FontReset,
    Help,
    RenameTab,
    ListTabs,
    ReloadSettings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Key(String);

impl TryFrom<String> for Key {
    type Error = &'static str;
    fn try_from(key: String) -> Result<Self, Self::Error> {
        let key = key.to_ascii_lowercase();
        let single = key.len() == 1
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+-=".contains(&b));
        let function = key
            .strip_prefix('f')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=12).contains(&n));
        if single
            || function
            || matches!(
                key.as_str(),
                "tab" | "enter" | "left" | "right" | "up" | "down"
            )
        {
            Ok(Self(if key == "=" { "+".into() } else { key }))
        } else {
            Err("unsupported shortcut key")
        }
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
    pub key: Key,
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
}

pub(super) fn validate(shortcuts: &[Shortcut]) -> Result<(), Error> {
    use Modifier::{Alt, Ctrl, Shift};
    let error = Error::Invalid(Field::Shortcuts);
    if shortcuts.len() > 64 {
        return Err(error);
    }
    let mut commands = std::collections::BTreeSet::new();
    let mut chords = std::collections::BTreeSet::new();
    for shortcut in shortcuts {
        let modifiers: std::collections::BTreeSet<_> = shortcut.modifiers.iter().copied().collect();
        let arrow = matches!(shortcut.key.0.as_str(), "left" | "right" | "up" | "down");
        let safe = (modifiers == [Ctrl, Shift].into()
            && !matches!(shortcut.key.0.as_str(), "c" | "v"))
            || (arrow && (modifiers == [Alt].into() || modifiers == [Alt, Shift].into()))
            || (shortcut.key.0 == "tab" && modifiers == [Ctrl].into())
            || (shortcut.key.0 == "f1" && modifiers.is_empty());
        if modifiers.len() != shortcut.modifiers.len()
            || !safe
            || !commands.insert(shortcut.command)
            || !chords.insert((modifiers, shortcut.key.clone()))
        {
            return Err(error);
        }
    }
    Ok(())
}
