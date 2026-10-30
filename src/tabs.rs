//! Constant-size background activity; no terminal contents or event backlog.

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
