//! Passive banners reserve bounded layout space and never own typing focus.
use super::super::{App, Control, Message};
use crate::errors::{Action, Error, Owner, Recovery};
use crate::identity::SessionId;
use iced::{
    Element,
    widget::{button, column, row, scrollable, text},
};

pub(in crate::app) const ERROR_HEIGHT: f32 = 96.0;

#[derive(Clone)]
enum Event {
    Control(Control),
    Retry(super::RecoveryRequest),
}

impl App {
    pub(in crate::app) fn error_height(&self, id: SessionId) -> f32 {
        if self.errors.latest(Owner::Session(id)).is_some() {
            ERROR_HEIGHT
        } else {
            0.0
        }
    }
    pub(in crate::app) fn passive_error_view(&self) -> Option<Element<'_, Message>> {
        self.application_error_view()
    }
    pub(in crate::app) fn error_view(&self, owner: Owner) -> Option<Element<'_, Message>> {
        let notice = self.errors.latest(owner)?;
        let message = if notice.resolved {
            "The terminal size was applied. Dismiss this message when convenient.".into()
        } else {
            notice.error.message()
        };
        let mut actions = row![
            button("Dismiss").on_press(Event::Control(Control::ErrorAction(
                notice.id,
                Action::Dismiss
            )))
        ]
        .spacing(8);
        let retry = if notice.resolved || notice.retrying {
            None
        } else {
            match (owner, notice.error.recovery()) {
                (Owner::Session(id), Recovery::Resize)
                    if self.terminal(id).is_some_and(|t| !t.input_status().closed) =>
                {
                    Some("Retry size")
                }
                (Owner::Session(id), Recovery::Restart) if self.restart_ready(id) => {
                    Some("Retry terminal")
                }
                (Owner::Session(id), Recovery::PasteAgain) if self.error_input_owner(id) => {
                    Some("Try paste again")
                }
                (Owner::Session(id), Recovery::CopyAgain) if self.error_input_owner(id) => {
                    Some("Try copy again")
                }
                (Owner::Application, Recovery::CopyAgain) if self.diagnostics.enabled() => {
                    Some("Try copy again")
                }
                (Owner::Application, Recovery::ReloadSettings) if !self.settings.pending => {
                    Some("Retry settings")
                }
                _ => None,
            }
        };
        if let Some(label) = retry {
            let event = if matches!(
                notice.error.recovery(),
                Recovery::PasteAgain | Recovery::CopyAgain
            ) && matches!(owner, Owner::Session(_))
            {
                self.recovery_request(notice.id).map(Event::Retry)
            } else {
                Some(Event::Control(Control::ErrorAction(
                    notice.id,
                    Action::Retry,
                )))
            };
            if let Some(event) = event {
                actions = actions.push(button(label).on_press(event));
            }
        }
        if matches!(notice.error, Error::Launch(_)) {
            actions = actions.push(button("Use default shell").on_press(Event::Control(
                Control::ErrorAction(notice.id, Action::UseDefault),
            )));
        }
        let mut content = column![scrollable(text(message)).height(40), actions].spacing(4);
        if notice.count > 1 {
            content = content.push(text(format!("Repeated {} times", notice.count)).size(12));
        }
        let content: Element<'_, Event> = content.into();
        Some(content.map(|event| match event {
            Event::Control(control) => Message::Control(control),
            Event::Retry(request) => Message::ErrorRetry(request),
        }))
    }

    pub(in crate::app) fn application_error_view(&self) -> Option<Element<'_, Message>> {
        let error = self.error_view(Owner::Application);
        if !self.diagnostics.enabled() {
            return error;
        }
        let controls: Element<'_, Control> = row![
            text("Private diagnostics enabled · memory only · 15 min / 64 records").size(12),
            button("Copy diagnostics").on_press(Control::CopyDiagnostics),
            button("Clear diagnostics").on_press(Control::ClearDiagnostics),
        ]
        .spacing(8)
        .into();
        let mut content = column![];
        if let Some(error) = error {
            content = content.push(error);
        }
        Some(content.push(controls.map(Message::Control)).into())
    }
}
