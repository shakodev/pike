use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Fill};

use super::{App, CloseAction, Control, Message};
use crate::identity::{SessionId, TabId};
use crate::ui::{self, style, tokens};

pub(crate) fn close_button(id: SessionId) -> Element<'static, Control> {
    ui::icon("×", "Close pane", Control::CloseSession(id))
}

pub(crate) fn close_tab_button(tab: TabId) -> Element<'static, Control> {
    ui::icon("×", "Close tab", Control::CloseTab(tab))
}

impl App {
    /// Replace the workspace while modal; terminal widgets receive no native
    /// keyboard, IME, mouse or clipboard callbacks during a decision.
    pub(in crate::app) fn close_view(&self) -> Option<Element<'_, Message>> {
        let content: Element<'_, Control> = if let Some(c) = &self.close_policy.confirmation {
            let label = if c.window {
                "Close Pike?"
            } else {
                "Close terminals?"
            };
            let count = format!(
                "{} starting or running terminal(s) will be stopped. {} terminal(s) selected.",
                c.running,
                c.sessions.len()
            );
            column![
                text(label).size(tokens::HEADING),
                text(count).size(tokens::BODY),
                text("Cancel is selected. Tab changes selection; Enter chooses; Escape cancels.")
                    .size(tokens::SMALL),
                row![
                    button(if c.affirmative {
                        "Cancel"
                    } else {
                        "› Cancel"
                    })
                    .style(style::quiet)
                    .on_press(Control::CloseAction(CloseAction::Cancel(c.token))),
                    button(if c.affirmative {
                        "› Close selected"
                    } else {
                        "Close selected"
                    })
                    .style(style::quiet)
                    .on_press(Control::CloseAction(CloseAction::Confirm(c.token))),
                ]
                .spacing(tokens::INSET)
                .wrap(),
            ]
            .spacing(tokens::INSET)
            .into()
        } else if self.exiting {
            let count = self.model.sessions().len();
            let mut content = column![
                text("Closing Pike…").size(tokens::HEADING),
                text(format!(
                    "Waiting for cleanup of {count} terminal session(s)."
                ))
            ]
            .spacing(16);
            if self.close_policy.timed_out
                && let Some(token) = self.close_policy.timer
            {
                content = content.push(text("Cleanup is taking longer than three seconds. Wait continues cleanup; Force close exits Pike before cleanup is acknowledged."))
                    .push(text("Wait is selected by default. Tab changes selection; Enter chooses; Escape waits."))
                    .push(row![
                        button(if self.close_policy.force_selected { "Wait" } else { "› Wait" })
                            .on_press(Control::CloseAction(CloseAction::Wait(token))),
                        button(if self.close_policy.force_selected { "› Force close Pike" } else { "Force close Pike" })
                            .on_press(Control::CloseAction(CloseAction::Force(token))),
                    ].spacing(12));
            }
            content.into()
        } else {
            return None;
        };
        Some(
            container(scrollable(
                container(content.map(Message::Control))
                    .max_width(560)
                    .padding(16)
                    .style(style::surface),
            ))
            .padding(tokens::INSET)
            .center_x(Fill)
            .center_y(Fill)
            .style(style::workspace)
            .into(),
        )
    }
}
