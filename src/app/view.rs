use iced::widget::{PaneGrid, button, column, container, pane_grid, row, text};
use iced::{Element, Fill};
use iced_term::TerminalView;

use super::{App, Control, Message};
use crate::identity::SessionId;
use crate::session::Lifecycle;
use crate::terminal::Owner;

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let mut controls = row![text("Pike").size(22)].spacing(12);
        if !self.exiting {
            if let Some(target) = self.active_target() {
                controls = controls.push(button("Close terminal").on_press(Control::Close(target)));
                if let Some(id) = self.active_session()
                    && self.terminal(id).is_some()
                {
                    controls = controls.push(button("Copy selection").on_press(Control::Copy(id)));
                }
            } else {
                controls = controls.push(button("New terminal").on_press(Control::New));
            }
        }
        let pane: Element<'_, Message> = if self.exiting {
            text("Closing terminals…").into()
        } else if let Some(tab) = self
            .model
            .workspace()
            .active()
            .and_then(|id| self.model.workspace().tab(id))
        {
            PaneGrid::new(tab.grid(), |_, pane, _| {
                pane_grid::Content::new(self.pane(pane.session))
            })
            .width(Fill)
            .height(Fill)
            .into()
        } else {
            container(text("No terminal open"))
                .width(Fill)
                .height(Fill)
                .into()
        };
        let controls: Element<'_, Control> = controls.into();
        let mut content = column![controls.map(Message::Control), pane].spacing(8);
        if let Some(notice) = self.notice {
            content = content.push(text(notice));
        }
        container(content)
            .padding(8)
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn pane(&self, id: SessionId) -> Element<'_, Message> {
        let Some(entry) = self.sessions.entries.get(&id) else {
            return text("Starting terminal…").into();
        };
        match &entry.owner {
            Owner::Starting(_) => text("Starting terminal…").into(),
            Owner::Disposing(_) => text("Closing terminal…").into(),
            Owner::Failed(error) => {
                let failed: Element<'_, Control> = column![
                    text(error.to_string()),
                    row![
                        button("Retry").on_press(Control::Retry(id)),
                        button("New default terminal").on_press(Control::UseDefault(id))
                    ]
                    .spacing(8)
                ]
                .spacing(8)
                .into();
                failed.map(Message::Control)
            }
            Owner::Attached(terminal) => {
                let mut content = column![
                    TerminalView::show(terminal).map(move |event| Message::Terminal((id, event)))
                ];
                if let Some(Lifecycle::Exited { code }) =
                    self.model.sessions().get(id).map(|e| e.lifecycle())
                {
                    let status = code.map_or_else(
                        || "Terminal exited (status unavailable)".into(),
                        |code| format!("Terminal exited ({code})"),
                    );
                    let status: Element<'_, Control> =
                        row![text(status), button("Restart").on_press(Control::Retry(id))]
                            .spacing(8)
                            .into();
                    content = content.push(status.map(Message::Control));
                } else if entry.ended {
                    content = content.push(text("Terminal exited — finishing output and cleanup…"));
                }
                if let Some(notice) = entry.notice {
                    content = content.push(text(notice));
                }
                content.height(Fill).into()
            }
        }
    }
}
