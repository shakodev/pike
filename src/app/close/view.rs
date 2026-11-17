use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Fill};

use super::{App, CloseAction, Control, Message};
use crate::ui::{style, tokens};

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
                scrollable(column![
                    text(count).size(tokens::BODY),
                    text("Cancel is the default. Tab changes selection; Enter chooses; Escape cancels.")
                        .size(tokens::SMALL),
                ].spacing(tokens::INSET)).height(Fill),
                row![
                    container(button(if c.affirmative {
                        "Cancel"
                    } else {
                        "› Cancel"
                    })
                    .style(move |theme, status| style::choice(theme, status, !c.affirmative))
                    .on_press(Control::CloseAction(CloseAction::Cancel(c.token)))).id("close-cancel"),
                    container(button(if c.affirmative {
                        "› Close selected"
                    } else {
                        "Close selected"
                    })
                    .style(move |theme, status| style::choice(theme, status, c.affirmative))
                    .on_press(Control::CloseAction(CloseAction::Confirm(c.token)))).id("close-confirm"),
                ]
                .spacing(tokens::INSET)
                .wrap(),
            ]
            .spacing(tokens::INSET)
            .height(Fill)
            .into()
        } else if self.exiting {
            let count = self.model.sessions().len();
            let mut details = column![text(format!(
                "Waiting for cleanup of {count} terminal session(s)."
            ))]
            .spacing(tokens::INSET);
            if self.close_policy.timed_out
                && let Some(token) = self.close_policy.timer
            {
                details = details.push(text("Cleanup is taking longer than three seconds. Wait continues cleanup; Force close exits Pike before cleanup is acknowledged."))
                    .push(text("Wait is the default. Tab changes selection; Enter chooses; Escape waits."));
                column![
                    text("Closing Pike…").size(tokens::HEADING),
                    scrollable(details).height(Fill),
                    row![
                        button(if self.close_policy.force_selected {
                            "Wait"
                        } else {
                            "› Wait"
                        })
                        .style(move |theme, status| style::choice(
                            theme,
                            status,
                            !self.close_policy.force_selected
                        ))
                        .on_press(Control::CloseAction(CloseAction::Wait(token))),
                        button(if self.close_policy.force_selected {
                            "› Force close Pike"
                        } else {
                            "Force close Pike"
                        })
                        .style(move |theme, status| style::choice(
                            theme,
                            status,
                            self.close_policy.force_selected
                        ))
                        .on_press(Control::CloseAction(CloseAction::Force(token))),
                    ]
                    .spacing(tokens::INSET)
                    .wrap(),
                ]
                .spacing(tokens::INSET)
                .height(Fill)
                .into()
            } else {
                column![
                    text("Closing Pike…").size(tokens::HEADING),
                    scrollable(details).height(Fill)
                ]
                .spacing(tokens::INSET)
                .height(Fill)
                .into()
            }
        } else {
            return None;
        };
        Some(
            container(
                container(content.map(Message::Control))
                    .padding(12)
                    .style(style::surface)
                    .max_width(560)
                    .max_height(400)
                    .width(Fill)
                    .height(Fill),
            )
            .padding(tokens::INSET)
            .center_x(Fill)
            .center_y(Fill)
            .style(style::workspace)
            .into(),
        )
    }
}
