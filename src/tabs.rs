//! Bounded tab presentation and ordering; no terminal/process ownership.

use std::fmt;

/// Presentation limit, independent of the upstream OSC transport byte limit.
pub const TITLE_LIMIT: usize = 256;

/// Keep editable spaces while bounding draft storage; trimming is a save-time
/// policy, so typing a space between words does not silently remove it.
pub fn title_draft(value: &str) -> String {
    value
        .chars()
        .filter(|ch| {
            !ch.is_control()
                && !matches!(ch, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .take(TITLE_LIMIT)
        .collect()
}

/// Sanitized plain text. Debug output never includes a user or OSC title.
#[derive(Clone, PartialEq, Eq)]
pub struct Title(String);

impl Title {
    /// Empty/whitespace-only names mean "use the automatic title". Controls,
    /// line separators and bidi overrides cannot alter the surrounding chrome.
    /// Bound Unicode scalars, preserving UTF-8 and ordinary combining/emoji text.
    pub fn new(value: &str) -> Option<Self> {
        let clean = title_draft(value.trim());
        let clean = clean.trim();
        (!clean.is_empty()).then(|| Self(clean.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Title {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Title([redacted])")
    }
}

/// Manual names win until explicitly reset. An OSC reset/empty title removes
/// only that session's automatic title; launch profile then generic fallback
/// still work for starting, failed, exited and title-less sessions.
pub fn display_title(manual: Option<&Title>, osc: Option<&Title>, profile: &str) -> Title {
    manual
        .or(osc)
        .cloned()
        .or_else(|| Title::new(profile))
        .unwrap_or_else(|| Title("Terminal".into()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Before,
    After,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    /// Zero-based position in the current order, resolved only when consumed.
    Index(usize),
    Next,
    Previous,
}

/// Coalesced indicators since this tab was last selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    pub unread: bool,
    pub bell: bool,
    pub exited: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum ActivityEvent {
    Output,
    Bell,
    Exit,
}

impl Activity {
    pub(crate) fn record(&mut self, event: ActivityEvent) -> bool {
        let before = *self;
        match event {
            ActivityEvent::Output => self.unread = true,
            ActivityEvent::Bell => self.bell = true,
            ActivityEvent::Exit => self.exited = true,
        }
        *self != before
    }

    pub(crate) fn seen(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests;
