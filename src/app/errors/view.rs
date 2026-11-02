//! Passive banners reserve bounded layout space and never own typing focus.
use super::super::{App, Control, Message};
use crate::errors::{Action, Error, Owner, Recovery};
use crate::identity::SessionId;
use iced::{
    Element,
    widget::{button, column, row, scrollable, text},
};

pub(in crate::app) const ERROR_HEIGHT: f32 = 96.0;

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
        let mut actions =
            row![button("Dismiss").on_press(Control::ErrorAction(notice.id, Action::Dismiss))]
                .spacing(8);
        let retry = if notice.resolved {
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
                (Owner::Application, Recovery::ReloadSettings) if !self.settings.pending => {
                    Some("Retry settings")
                }
                _ => None,
            }
        };
        if let Some(label) = retry {
            actions = actions
                .push(button(label).on_press(Control::ErrorAction(notice.id, Action::Retry)));
        }
        if matches!(notice.error, Error::Launch(_)) {
            actions = actions.push(
                button("Use default shell")
                    .on_press(Control::ErrorAction(notice.id, Action::UseDefault)),
            );
        }
        let mut content = column![scrollable(text(message)).height(40), actions].spacing(4);
        if notice.count > 1 {
            content = content.push(text(format!("Repeated {} times", notice.count)).size(12));
        }
        let content: Element<'_, Control> = content.into();
        Some(content.map(Message::Control))
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
