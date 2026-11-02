//! Passive banners reserve bounded layout space and never own typing focus.
use super::super::{App, Control, Message};
use crate::errors::{Action, Error, Owner, Recovery};
use iced::{
    Element, Fill,
    widget::{button, column, container, row, scrollable, text},
};

pub(in crate::app) const ERROR_HEIGHT: f32 = 96.0;
const ROOT_ERROR_HEIGHT: f32 = 128.0;
const DIAGNOSTICS_HEIGHT: f32 = 40.0;

#[derive(Clone)]
enum Event {
    Control(Control),
    Retry(super::RecoveryRequest),
}

impl App {
    pub(in crate::app) fn passive_error_height(&self) -> f32 {
        (if self.errors.is_empty() {
            0.0
        } else {
            ROOT_ERROR_HEIGHT
        }) + if self.diagnostics.enabled() {
            DIAGNOSTICS_HEIGHT
        } else {
            0.0
        }
    }

    /// One bounded row outside PaneGrid's unconditional press-to-focus path.
    /// Provenance is derived from current stable ownership, never cached indices.
    pub(in crate::app) fn passive_error_view(&self) -> Option<Element<'_, Message>> {
        let height = self.passive_error_height();
        if height == 0.0 {
            return None;
        }
        let mut content = column![].spacing(0);
        if !self.errors.is_empty() {
            let mut cards = column![].spacing(8);
            for notice in self.errors.notices().rev() {
                let label = match notice.owner {
                    Owner::Application => "Pike application".into(),
                    Owner::Session(id) => match self.model.target(id) {
                        Some(target) => {
                            let tab_number = self
                                .model
                                .workspace()
                                .order()
                                .iter()
                                .position(|tab| *tab == target.tab)
                                .map_or(0, |position| position + 1);
                            let title = self
                                .tab_title(target.tab)
                                .map_or_else(String::new, |t| t.as_str().to_owned());
                            format!("Tab {tab_number} · {title} · Terminal {}", id.get())
                        }
                        None => format!("Terminal {} · cleanup", id.get()),
                    },
                };
                cards =
                    cards.push(column![text(label).size(12), self.notice_view(notice)].spacing(2));
            }
            content = content.push(scrollable(cards).height(ROOT_ERROR_HEIGHT));
        }
        if self.diagnostics.enabled() {
            let controls: Element<'_, Control> = row![
                text("Diagnostics · memory only · 15 min / 64 records").size(12),
                button("Copy diagnostics").on_press(Control::CopyDiagnostics),
                button("Clear").on_press(Control::ClearDiagnostics),
            ]
            .spacing(8)
            .into();
            content = content.push(
                scrollable(controls.map(Message::Control))
                    .direction(scrollable::Direction::Horizontal(
                        scrollable::Scrollbar::new().width(4),
                    ))
                    .height(DIAGNOSTICS_HEIGHT),
            );
        }
        Some(
            container(content)
                .height(height)
                .width(Fill)
                .clip(true)
                .into(),
        )
    }

    #[cfg(test)]
    pub(in crate::app) fn error_view(&self, owner: Owner) -> Option<Element<'_, Message>> {
        let notice = self.errors.latest(owner)?;
        Some(self.notice_view(notice))
    }

    fn notice_view(&self, notice: crate::errors::Notice) -> Element<'_, Message> {
        let owner = notice.owner;
        let mut message = if notice.resolved {
            "The terminal size was applied. Dismiss this message when convenient.".into()
        } else {
            notice.error.message()
        };
        if matches!(
            notice.error.recovery(),
            Recovery::PasteAgain | Recovery::CopyAgain
        ) && let Owner::Session(id) = owner
            && !self.error_input_owner(id)
        {
            message.push_str(
                if self.terminal(id).is_some_and(|t| !t.input_status().closed) {
                    " Select this terminal with the window focused to retry."
                } else {
                    " Clipboard retry is unavailable while this terminal is stopped."
                },
            );
        }
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
        let actions = scrollable(actions)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(4),
            ))
            .height(32);
        let mut content = column![scrollable(text(message)).height(40), actions].spacing(4);
        if notice.count > 1 {
            content = content.push(text(format!("Repeated {} times", notice.count)).size(12));
        }
        let content: Element<'_, Event> = container(content)
            .height(ERROR_HEIGHT)
            .width(Fill)
            .clip(true)
            .into();
        content.map(|event| match event {
            Event::Control(control) => Message::Control(control),
            Event::Retry(request) => Message::ErrorRetry(request),
        })
    }
}
