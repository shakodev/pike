use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Fill};

use super::{App, Control, Message, commands::Command, split::GUTTER};
use crate::ui::{self, style, tokens};

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        if let Some(modal) = self.close_view() {
            return modal;
        }
        if let Some(help) = self.help_view() {
            return help;
        }
        if let Some(dialog) = self.tab_dialog_view() {
            return dialog;
        }
        let references = self.command_reference();
        let mut chrome = row![]
            .spacing(tokens::SPACE)
            .align_y(iced::Alignment::Center);
        if !self.exiting && !self.model.workspace().order().is_empty() {
            chrome = chrome.push(self.tabs_view(&references));
        } else {
            chrome = chrome.push(container(text("Pike").size(tokens::BODY)).width(Fill));
        }
        chrome = chrome
            .push(
                container(ui::icon_maybe(
                    "+",
                    ui::command_hint(&references, Command::NewTab, "New terminal tab"),
                    self.command_enabled(Command::NewTab)
                        .then_some(Control::NewTab),
                ))
                .id("new-tab-control"),
            )
            .push(container(self.command_menu()).id("command-menu-control"));
        let chrome: Element<'_, Control> = container(chrome)
            .height(tokens::TAB_HEIGHT)
            .padding([0.0, tokens::SPACE])
            .style(style::surface)
            .id("workspace-chrome")
            .into();
        let pane: Element<'_, Message> = if self.exiting {
            container(text("Closing terminals…").size(tokens::BODY))
                .center(Fill)
                .into()
        } else if self.boot.is_some()
            || (self.settings.booting && self.model.workspace().order().is_empty())
        {
            container(
                text(if self.settings.pending {
                    "Loading settings…"
                } else {
                    "Preparing Pike…"
                })
                .size(tokens::BODY),
            )
            .center(Fill)
            .into()
        } else if let Some(tab_id) = self.model.workspace().active() {
            self.pane_grid(tab_id, &references)
        } else {
            let empty: Element<'_, Control> = column![
                text("No terminal open").size(tokens::HEADING),
                text("Start a terminal, or choose a profile from the Commands menu.")
                    .size(tokens::BODY),
                button(text("New terminal").size(tokens::BODY))
                    .height(tokens::CONTROL)
                    .style(style::quiet)
                    .on_press(Control::New),
            ]
            .spacing(tokens::INSET)
            .into();
            scrollable(
                container(empty.map(Message::Control))
                    .padding(tokens::INSET)
                    .center(Fill),
            )
            .height(Fill)
            .width(Fill)
            .into()
        };
        let constrained = self
            .grid_size
            .and_then(|(tab, area)| {
                let geometry = self.resize_geometry(tab, area)?;
                let node = self.model.workspace().tab(tab)?.grid().layout();
                Some(geometry.canvas(node).is_none_or(|canvas| {
                    canvas.width > area.width
                        || canvas.height > area.height
                        || node
                            .pane_regions(GUTTER, 0.0, canvas)
                            .iter()
                            .any(|(pane, r)| {
                                let minimum = geometry.minimum[pane];
                                r.width < minimum.width || r.height < minimum.height
                            })
                }))
            })
            .unwrap_or(false);
        let mut content = column![chrome.map(Message::Control)].spacing(0);
        // Passive host rows stay outside the terminal's clipped/pannable area.
        let mut passive = column![].spacing(tokens::SPACE);
        let mut has_passive = false;
        if !self.exiting && self.settings.enabled && self.settings.pending {
            passive = passive.push(text("Reloading settings…").size(tokens::SMALL));
            has_passive = true;
        }
        if constrained {
            passive = passive.push(
                text("Size constrained · 20×5 minimum · drag scrollbars to pan")
                    .size(tokens::SMALL),
            );
            has_passive = true;
        }
        if let Some(notice) = self.notice {
            passive = passive.push(text(ui::plain_title(notice, 512)).size(tokens::SMALL));
            has_passive = true;
        }
        if has_passive {
            content = content.push(
                container(scrollable(passive).height(iced::Length::Shrink))
                    .max_height(72.0)
                    .padding([tokens::SPACE, tokens::INSET])
                    .style(style::surface)
                    .id("workspace-notices"),
            );
        }
        if let Some(recovery) = self.passive_error_view() {
            content = content.push(recovery);
        }
        let content = content.push(pane);
        super::resize::capture(
            container(content)
                .width(Fill)
                .height(Fill)
                .style(style::workspace),
            self.interaction_blocked(),
            self.active_session().and_then(|id| self.terminal(id)),
            move |event| {
                super::commands::on_key(
                    event,
                    self.settings.active.shortcuts(),
                    self.active_session().and_then(|id| self.terminal(id)),
                )
            },
        )
        .map(move |message| match message {
            Message::Control(Control::Command(command)) => Message::Control(
                Control::MappedCommand(self.active_session(), self.settings.generation, command),
            ),
            message => message,
        })
    }
}
