//! Recovery controls are owner-bound and passive; they never select a pane.
use std::time::Instant;

use super::{App, Message};
use crate::errors::{Action, Error, ErrorId, Notice, Owner, Recovery};
use crate::{identity::SessionId, session::Lifecycle};
use iced::Task;

mod view;

impl App {
    pub fn set_diagnostics_enabled(&mut self, enabled: bool) {
        self.diagnostics.set_enabled(enabled);
    }

    pub fn diagnostic_capture(&mut self) -> String {
        self.diagnostics.capture(Instant::now())
    }

    pub fn latest_error(&self, owner: Owner) -> Option<Notice> {
        self.errors.latest(owner)
    }

    pub(super) fn report_error(&mut self, owner: Owner, error: Error) {
        if let Owner::Session(id) = owner
            && self.model.sessions().get(id).is_none()
        {
            return;
        }
        self.errors.admit(owner, error);
        self.diagnostics
            .record(owner, error, None, None, Instant::now());
    }

    pub(super) fn error_action(&mut self, id: ErrorId, action: Action) -> Task<Message> {
        let Some(notice) = self.errors.get(id) else {
            return Task::none();
        };
        if let Owner::Session(session) = notice.owner
            && self.model.sessions().get(session).is_none()
        {
            self.errors.dismiss(id);
            return Task::none();
        }
        if action == Action::Dismiss {
            self.errors.dismiss(id);
            if let Owner::Session(session) = notice.owner
                && let Some(entry) = self.sessions.entries.get_mut(&session)
            {
                entry.notice = None;
            }
            self.refresh_resize_layout();
            return Task::none();
        }
        if self.interaction_blocked() {
            return Task::none();
        }
        if notice.resolved {
            return Task::none();
        }
        match (notice.owner, notice.error.recovery()) {
            (Owner::Session(session), Recovery::Resize) if action == Action::Retry => {
                self.update(Message::Terminal((
                    session,
                    iced_term::Event::BackendCall(
                        session.get(),
                        iced_term::BackendCommand::RetryResize,
                    ),
                )))
            }
            (Owner::Session(session), Recovery::Restart) if self.restart_ready(session) => {
                if action == Action::UseDefault && !matches!(notice.error, Error::Launch(_)) {
                    return Task::none();
                }
                // Preserve a newer healthy launch's intent when restarting a
                // background failure; admission/dismissal never touches focus.
                let restore =
                    (self.active_session() != Some(session)).then(|| self.focus_on_attach.clone());
                let task = self.retry(session, action == Action::UseDefault);
                if let Some(restore) = restore {
                    self.focus_on_attach = restore;
                }
                task
            }
            // Fresh clipboard retry joins A32's existing captured owner lease
            // after its relevant main integration; never create another route.
            (Owner::Session(session), Recovery::CopyAgain)
                if action == Action::Retry && self.error_input_owner(session) =>
            {
                self.copy_owned(session)
            }
            (Owner::Application, Recovery::ReloadSettings) if action == Action::Retry => {
                self.reload_settings()
            }
            _ => Task::none(),
        }
    }

    fn restart_ready(&self, id: SessionId) -> bool {
        matches!(
            self.model.sessions().get(id).map(|e| e.lifecycle()),
            Some(Lifecycle::Failed(_) | Lifecycle::Exited { .. })
        )
    }

    fn error_input_owner(&self, id: SessionId) -> bool {
        !self.interaction_blocked()
            && self.active_session() == Some(id)
            && self.window_focus.is_focused()
            && self.terminal(id).is_some_and(|terminal| {
                terminal.is_visible() && !terminal.input_status().closed && !terminal.is_composing()
            })
    }

    /// Use the existing native backend's actual write result. Only a stable
    /// reservation and metadata result return; no readback or retained payload.
    pub(super) fn copy_owned(&self, id: SessionId) -> Task<Message> {
        let Some(terminal) = self.terminal(id) else {
            return Task::none();
        };
        let Some(reservation) = self
            .model
            .sessions()
            .get(id)
            .map(|entry| entry.reservation())
        else {
            return Task::none();
        };
        terminal
            .copy_selection(|contents| {
                iced_runtime::clipboard::write_with_result(contents)
                    .map(move |result| Message::ClipboardWritten(Some(reservation), result))
            })
            .unwrap_or_else(Task::none)
    }

    pub(super) fn copy_diagnostics(&mut self) -> Task<Message> {
        if !self.diagnostics.enabled() {
            return Task::none();
        }
        iced_runtime::clipboard::write_with_result(self.diagnostic_capture())
            .map(|result| Message::ClipboardWritten(None, result))
    }

    pub(super) fn clipboard_written(
        &mut self,
        reservation: Option<crate::identity::Reservation>,
        result: Result<(), iced_runtime::clipboard::WriteError>,
    ) {
        let owner = match reservation {
            Some(reservation)
                if self
                    .model
                    .sessions()
                    .get(reservation.session)
                    .is_some_and(|entry| entry.reservation() == reservation) =>
            {
                Owner::Session(reservation.session)
            }
            Some(_) => return,
            None => Owner::Application,
        };
        if let Err(error) = result {
            self.report_error(owner, Error::ClipboardWrite(error));
        } else {
            self.errors
                .resolve(owner, |error| matches!(error, Error::ClipboardWrite(_)));
        }
        self.refresh_resize_layout();
    }
}
