//! Recoverable failures contain typed metadata only: no event, path or user text.
use std::collections::VecDeque;
use std::io::ErrorKind;

use crate::{identity::SessionId, launch::LaunchError, settings};
#[path = "errors/writes.rs"]
mod writes;
pub use writes::WriteReceipt;
pub(crate) use writes::Writes;

pub const NOTICE_LIMIT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Application,
    Session(SessionId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Launch(LaunchError),
    Io(ErrorKind),
    Overload(iced_term::EventBridgeError),
    Input(iced_term::InputError),
    Clipboard(iced_term::PasteError),
    ClipboardWrite(iced_runtime::clipboard::WriteError),
    Ime(iced_term::ImeError),
    Config(settings::Error),
    Renderer(iced_runtime::system::RendererFailure),
    Cleanup,
    Worker,
    Resize(iced_term::ResizeError),
    LinkOpen,
    Operation(crate::app::commands::Error),
    CleanupPrompt(Option<i32>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    Restart,
    PasteAgain,
    CopyAgain,
    ReloadSettings,
    Wait,
    /// Rejected input is never saved or replayed. The user can type it again.
    Reenter,
    Relaunch,
    Resize,
}

impl Error {
    pub fn recovery(self) -> Recovery {
        match self {
            Self::Launch(_) | Self::Io(_) | Self::Overload(_) | Self::Worker => Recovery::Restart,
            Self::Clipboard(_) => Recovery::PasteAgain,
            Self::ClipboardWrite(_) => Recovery::CopyAgain,
            Self::Input(_) | Self::Ime(_) => Recovery::Reenter,
            Self::Config(_) => Recovery::ReloadSettings,
            Self::Renderer(_) => Recovery::Relaunch,
            Self::Cleanup => Recovery::Wait,
            Self::Resize(_) => Recovery::Resize,
            Self::LinkOpen => Recovery::Reenter,
            Self::Operation(_) => Recovery::Reenter,
            Self::CleanupPrompt(_) => Recovery::Wait,
        }
    }

    pub fn message(self) -> String {
        match self {
            Self::Launch(error) => error.to_string(),
            Self::Io(_) | Self::Worker => "The terminal stopped unexpectedly. Final output is retained; restart after cleanup completes.".into(),
            Self::Overload(_) => "Terminal output stopped after its event limit was reached. Restart after cleanup completes.".into(),
            Self::Input(error) => format!("Input was not sent: {error}. Type it again after the program reads input."),
            Self::Clipboard(error) => error.to_string(),
            Self::ClipboardWrite(_) => "Could not write the clipboard. Check clipboard access and try again.".into(),
            Self::Ime(error) => error.to_string(),
            Self::Config(error) => format!("{error} Correct settings.toml, then reload; the last valid settings remain active."),
            Self::Renderer(_) => renderer_report().into(),
            Self::Cleanup => "Terminal cleanup is still pending. Wait for owned resources to close; healthy terminals remain usable.".into(),
            Self::Resize(_) => "The terminal could not apply its new size. Typing remains available; retry the current size or dismiss this message.".into(),
            Self::LinkOpen => "Could not open the link. Check the default application, then open the link again.".into(),
            Self::Operation(error) => error.message().into(),
            Self::CleanupPrompt(_) => "Pike could not display its cleanup choice. Cleanup remains pending; no automatic force close was attempted.".into(),
        }
    }
}

/// This is also the safe startup report when no renderer exists to show a banner.
pub fn renderer_report() -> &'static str {
    "Pike could not start its window or renderer. Check display/graphics availability and relaunch Pike. No automatic renderer fallback was attempted."
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrorId(u64);
impl ErrorId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Retry,
    Dismiss,
    UseDefault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Notice {
    pub id: ErrorId,
    pub owner: Owner,
    pub error: Error,
    pub count: u32,
    pub resolved: bool,
    pub retrying: bool,
}

/// Bounded UI state, independent of optional diagnostics. A dismissed error can
/// recur, but queued controls for an evicted/dismissed error have no authority.
#[derive(Default)]
pub struct Store {
    next: u64,
    notices: VecDeque<Notice>,
}

impl Store {
    pub fn admit(&mut self, owner: Owner, error: Error) -> Option<ErrorId> {
        if let Some(position) = self
            .notices
            .iter()
            .position(|n| n.owner == owner && n.error == error)
        {
            let mut notice = self.notices.remove(position).expect("known notice");
            if notice.retrying {
                self.next = self.next.checked_add(1)?;
                notice.id = ErrorId(self.next);
            }
            notice.count = notice.count.saturating_add(1);
            notice.resolved = false;
            notice.retrying = false;
            let id = notice.id;
            self.notices.push_back(notice);
            return Some(id);
        }
        self.next = self.next.checked_add(1)?;
        let id = ErrorId(self.next);
        if self.notices.len() == NOTICE_LIMIT {
            self.notices.pop_front();
        }
        self.notices.push_back(Notice {
            id,
            owner,
            error,
            count: 1,
            resolved: false,
            retrying: false,
        });
        Some(id)
    }

    pub fn get(&self, id: ErrorId) -> Option<Notice> {
        self.notices.iter().find(|notice| notice.id == id).copied()
    }

    pub fn begin_retry(&mut self, id: ErrorId) -> bool {
        let Some(notice) = self.notices.iter_mut().find(|n| n.id == id) else {
            return false;
        };
        if notice.retrying || notice.resolved {
            return false;
        }
        notice.retrying = true;
        true
    }

    /// A revoked asynchronous reply ends only its originating attempt. Rotate
    /// authority so that its held controls/replies cannot affect a fresh retry.
    pub fn cancel_retry(&mut self, owner: Owner, id: ErrorId) -> bool {
        let Some(notice) = self
            .notices
            .iter_mut()
            .find(|n| n.id == id && n.owner == owner && n.retrying)
        else {
            return false;
        };
        let Some(next) = self.next.checked_add(1) else {
            return false;
        };
        self.next = next;
        notice.id = ErrorId(next);
        notice.retrying = false;
        true
    }

    pub fn latest(&self, owner: Owner) -> Option<Notice> {
        self.notices
            .iter()
            .rev()
            .find(|n| n.owner == owner)
            .copied()
    }

    pub fn notices(&self) -> impl DoubleEndedIterator<Item = Notice> + '_ {
        self.notices.iter().copied()
    }

    pub fn dismiss(&mut self, id: ErrorId) -> Option<Notice> {
        let position = self.notices.iter().position(|n| n.id == id)?;
        self.notices.remove(position)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(Owner) -> bool) {
        self.notices.retain(|n| keep(n.owner));
    }

    /// A successful operation resolves only its own category, leaving other
    /// source errors intact. Old controls for resolved notices become inert.
    pub fn resolve(&mut self, owner: Owner, mut resolved: impl FnMut(Error) -> bool) {
        self.notices
            .retain(|n| n.owner != owner || !resolved(n.error));
    }

    /// Retain a recovered resize banner's space until explicit dismissal. This
    /// prevents its own viewport change from causing an error/success loop.
    pub fn resized(&mut self, owner: Owner) {
        for notice in &mut self.notices {
            if notice.owner == owner && matches!(notice.error, Error::Resize(_)) {
                notice.resolved = true;
                notice.retrying = false;
            }
        }
    }

    pub fn clear(&mut self) {
        self.notices = VecDeque::new();
    }

    pub fn len(&self) -> usize {
        self.notices.len()
    }
    pub fn is_empty(&self) -> bool {
        self.notices.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> SessionId {
        let mut model = crate::workspace::Model::default();
        let (_, crate::session::Effect::Spawn(reservation)) = model.create_tab().unwrap() else {
            panic!("spawn")
        };
        reservation.session
    }

    #[test]
    fn repeated_errors_coalesce_eviction_and_dismissal_never_recycle_authority() {
        let mut store = Store::default();
        let owner = Owner::Session(session());
        let id = store.admit(owner, Error::Cleanup).unwrap();
        for _ in 0..10000 {
            assert_eq!(store.admit(owner, Error::Cleanup), Some(id));
        }
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(id).unwrap().count, 10001);
        store.dismiss(id).unwrap();
        assert_ne!(store.admit(owner, Error::Cleanup), Some(id));
        for _ in 0..NOTICE_LIMIT * 3 {
            store.admit(Owner::Session(session()), Error::Cleanup);
        }
        assert_eq!(store.len(), NOTICE_LIMIT);
        assert_eq!(store.dismiss(id), None);
        store.retain(|candidate| candidate == owner);
        assert!(store.is_empty());
    }

    #[test]
    fn every_error_has_bounded_safe_context_and_an_explicit_recovery() {
        let errors = [
            Error::Launch(LaunchError::Spawn(ErrorKind::NotFound)),
            Error::Io(ErrorKind::BrokenPipe),
            Error::Overload(iced_term::EventBridgeError::PayloadOverflow),
            Error::Input(iced_term::InputError::Full {
                requested: usize::MAX,
                available: 0,
            }),
            Error::Clipboard(iced_term::PasteError::ClipboardUnavailable),
            Error::ClipboardWrite(iced_runtime::clipboard::WriteError::WriteFailed),
            Error::Ime(iced_term::ImeError::WrongSession),
            Error::Config(settings::Error::Invalid(settings::Field::Args)),
            Error::Renderer(iced_runtime::system::RendererFailure::Initialization),
            Error::Cleanup,
            Error::Worker,
            Error::Resize(iced_term::ResizeError {
                kind: ErrorKind::PermissionDenied,
                code: Some(i32::MIN),
            }),
            Error::LinkOpen,
            Error::Operation(crate::app::commands::Error::NoActivePane),
            Error::CleanupPrompt(Some(i32::MAX)),
        ];
        for error in errors {
            assert!(error.message().len() < 1024);
            assert!(!error.message().chars().any(char::is_control));
            let _ = error.recovery();
        }
    }

    #[test]
    fn a_success_resolves_only_its_origin_and_category() {
        let mut store = Store::default();
        let a = Owner::Session(session());
        let b = Owner::Session(session());
        let copy = Error::ClipboardWrite(iced_runtime::clipboard::WriteError::Unavailable);
        let old = store.admit(a, copy).unwrap();
        let other = store.admit(b, copy).unwrap();
        let input = store
            .admit(a, Error::Input(iced_term::InputError::Closed))
            .unwrap();
        store.resolve(a, |error| matches!(error, Error::ClipboardWrite(_)));
        assert!(store.get(old).is_none());
        assert!(store.get(other).is_some());
        assert!(store.get(input).is_some());
    }
}
