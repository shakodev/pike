//! Bounded transient composition, never a copy of terminal input/history.
use crate::{InputError, INPUT_BYTE_LIMIT};
use iced_core::input_method::{Event, Preedit};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

pub const PREEDIT_BYTE_LIMIT: usize = 4096;
const PENDING_COMMIT_LIMIT: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImeError {
    PreeditTooLarge { bytes: usize },
    CommitQueueFull { bytes: usize, available: usize },
    WrongSession,
    Input(InputError),
}

impl fmt::Display for ImeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PreeditTooLarge { bytes } => write!(f, "Composition preview was cleared: {bytes} bytes exceed the {PREEDIT_BYTE_LIMIT}-byte limit."),
            Self::CommitQueueFull { bytes, available } => write!(f, "IME text was not sent: {bytes} bytes requested, {available} pending bytes available. Try again after input is processed."),
            Self::WrongSession => f.write_str("IME text was not sent: the target session changed."),
            Self::Input(error) => write!(f, "IME text was not sent: {error}"),
        }
    }
}
impl std::error::Error for ImeError {}

struct Commit {
    owner: Weak<Session>,
    epoch: u64,
    text: Mutex<Option<Box<str>>>,
}

/// Redacted, owner/generation-bound deferred commit. Clones consume it once;
/// dropping the last clone releases its payload without sending any bytes.
#[derive(Clone)]
pub struct ImeCommit(Arc<Commit>);

impl fmt::Debug for ImeCommit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImeCommit")
            .field(
                "pending_bytes",
                &self
                    .0
                    .text
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .map_or(0, |s| s.len()),
            )
            .finish()
    }
}

impl ImeCommit {
    pub(crate) fn take(
        &self,
        owner: &Arc<Session>,
    ) -> Result<Option<Box<str>>, ImeError> {
        if !self.0.owner.ptr_eq(&Arc::downgrade(owner)) {
            self.0.text.lock().unwrap_or_else(|e| e.into_inner()).take();
            return Err(ImeError::WrongSession);
        }
        let state = owner.lock();
        let text = self.0.text.lock().unwrap_or_else(|e| e.into_inner()).take();
        Ok(if state.epoch == Some(self.0.epoch) && state.enabled {
            text
        } else {
            None
        })
    }
}

struct State {
    enabled: bool,
    requested: bool,
    opened: bool,
    epoch: Option<u64>,
    preedit: Option<Preedit>,
    pending: Vec<Weak<Commit>>,
}

impl State {
    fn cancel(&mut self) {
        self.epoch = self.epoch.and_then(|n| n.checked_add(1));
        self.requested = false;
        self.opened = false;
        self.preedit = None;
        for commit in self.pending.drain(..).filter_map(|p| p.upgrade()) {
            commit.text.lock().unwrap_or_else(|e| e.into_inner()).take();
        }
    }
}

pub(crate) struct Session(Mutex<State>);

impl Default for Session {
    fn default() -> Self {
        Self(Mutex::new(State {
            enabled: true,
            requested: false,
            opened: false,
            epoch: Some(0),
            preedit: None,
            pending: Vec::new(),
        }))
    }
}

impl Session {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub(crate) fn cancel(&self) {
        self.lock().cancel();
    }
    pub(crate) fn enabled(&self) -> bool {
        self.lock().enabled
    }
    pub(crate) fn set_enabled(&self, enabled: bool) {
        let mut state = self.lock();
        if state.enabled != enabled {
            state.cancel();
            state.enabled = enabled;
        }
    }
    pub(crate) fn composing(&self) -> bool {
        self.lock().opened
    }
    pub(crate) fn requested(&self, enabled: bool) {
        self.lock().requested = enabled;
    }
    pub(crate) fn preedit(&self) -> Option<Preedit> {
        self.lock().preedit.clone()
    }

    pub(crate) fn event(
        self: &Arc<Self>,
        event: &Event,
    ) -> Result<Option<ImeCommit>, ImeError> {
        let mut state = self.lock();
        if !state.enabled || !state.requested || state.epoch.is_none() {
            return Ok(None);
        }
        match event {
            Event::Opened => {
                state.opened = true;
                state.preedit = None;
            },
            Event::Closed => {
                state.opened = false;
                state.preedit = None;
            },
            Event::Preedit(text, selection) if state.opened => {
                state.preedit = None;
                if text.len() > PREEDIT_BYTE_LIMIT {
                    return Err(ImeError::PreeditTooLarge {
                        bytes: text.len(),
                    });
                }
                if !text.is_empty() {
                    state.preedit = Some(Preedit {
                        content: text.clone(),
                        selection: selection.clone().filter(|s| {
                            s.start <= s.end
                                && s.end <= text.len()
                                && text.is_char_boundary(s.start)
                                && text.is_char_boundary(s.end)
                        }),
                        text_size: None,
                    });
                }
            },
            Event::Commit(text) if state.opened => {
                state.preedit = None;
                if text.is_empty() {
                    return Ok(None);
                }
                let mut pending_bytes = 0;
                state.pending.retain(|weak| {
                    weak.upgrade().is_some_and(|commit| {
                        let text = commit
                            .text
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        if let Some(text) = &*text {
                            pending_bytes += text.len();
                            true
                        } else {
                            false
                        }
                    })
                });
                let available = INPUT_BYTE_LIMIT.saturating_sub(pending_bytes);
                if text.len() > available
                    || state.pending.len() == PENDING_COMMIT_LIMIT
                {
                    return Err(ImeError::CommitQueueFull {
                        bytes: text.len(),
                        available: if state.pending.len()
                            == PENDING_COMMIT_LIMIT
                        {
                            0
                        } else {
                            available
                        },
                    });
                }
                let commit = Arc::new(Commit {
                    owner: Arc::downgrade(self),
                    epoch: state.epoch.unwrap_or(0),
                    text: Mutex::new(Some(text.clone().into_boxed_str())),
                });
                state.pending.push(Arc::downgrade(&commit));
                return Ok(Some(ImeCommit(commit)));
            },
            _ => {}, // untagged late events require a fresh Opened after reset
        }
        Ok(None)
    }
}

/// One native IME owner per real window. Weak references never retain a PTY.
#[derive(Default)]
pub(crate) struct Window {
    owner: Option<Weak<Session>>,
    reset: bool,
}

impl Window {
    pub(crate) fn claim(&mut self, session: &Arc<Session>) {
        let owner = Arc::downgrade(session);
        if !self.owner.as_ref().is_some_and(|old| old.ptr_eq(&owner)) {
            if let Some(old) = self.owner.take() {
                if let Some(old) = old.upgrade() {
                    old.cancel();
                }
                // Even an expired owner may have left a native composition.
                self.reset = true;
            }
            self.owner = Some(owner);
        }
    }
    pub(crate) fn release(&mut self, session: &Arc<Session>) {
        if self
            .owner
            .as_ref()
            .is_some_and(|old| old.ptr_eq(&Arc::downgrade(session)))
        {
            self.clear();
        }
        session.cancel();
    }
    pub(crate) fn clear(&mut self) {
        if let Some(old) = self.owner.take().and_then(|old| old.upgrade()) {
            old.cancel();
        }
        self.reset = true;
    }
    pub(crate) fn disable_once(&mut self) -> bool {
        std::mem::take(&mut self.reset)
    }
}

#[cfg(test)]
mod tests;
