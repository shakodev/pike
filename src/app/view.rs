use iced::widget::{PaneGrid, button, column, container, pane_grid, row, scrollable, sensor, text};
use iced::{Border, Element, Fill};
use iced_term::TerminalView;

use super::{
    App, Control, Direction, Message,
    split::{BORDER, FOOTER, GUTTER, HEADER},
};
use crate::identity::SessionId;
use crate::layout::PaneTarget;
use crate::session::Lifecycle;
use crate::terminal::Owner;

impl App {
    pub(super) fn footer_height(&self, id: SessionId) -> f32 {
        if self
            .sessions
            .entries
            .get(&id)
            .is_some_and(|entry| entry.ended || entry.notice.is_some())
            || matches!(
                self.model.sessions().get(id).map(|e| e.lifecycle()),
                Some(Lifecycle::Exited { .. })
            )
        {
            FOOTER
        } else {
            0.0
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut controls = row![text("Pike").size(22)].spacing(12);
        if !self.exiting {
            if let Some(target) = self.active_target() {
                controls = controls
                    .push(button("Split right").on_press(Control::Split(target, Direction::Right)))
                    .push(button("Split down").on_press(Control::Split(target, Direction::Down)));
                if let Some(id) = self.active_session() {
                    controls =
                        controls.push(button("Close terminal").on_press(Control::CloseSession(id)));
                }
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
            let tab_id = self.model.workspace().active().expect("active tab");
            let grid = PaneGrid::new(tab.grid(), |handle, pane, _| {
                let target = PaneTarget {
                    tab: tab_id,
                    pane: handle,
                };
                let focused = handle == tab.focused();
                let header: Element<'_, Control> = container(
                    button(text(format!("Terminal {}", pane.session.get())).size(12))
                        .on_press(Control::Focus(target))
                        .style(button::text),
                )
                .height(HEADER)
                .into();
                let content = column![header.map(Message::Control), self.pane(pane.session)];
                pane_grid::Content::new(container(content).padding(BORDER).clip(true).style(
                    move |theme: &iced::Theme| container::Style {
                        border: Border {
                            width: BORDER,
                            color: if focused {
                                theme.palette().primary
                            } else {
                                theme.extended_palette().background.strong.color
                            },
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                ))
            })
            .spacing(GUTTER)
            .min_size(0)
            .on_click(move |pane| {
                Message::Control(Control::Focus(PaneTarget { tab: tab_id, pane }))
            })
            .width(Fill)
            .height(Fill);
            sensor(grid)
                .key(tab_id)
                .on_resize(move |size| Message::GridSize(tab_id, size))
                .into()
        } else {
            container(text("No terminal open"))
                .width(Fill)
                .height(Fill)
                .into()
        };
        let controls: Element<'_, Control> = controls.wrap().into();
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
            Owner::Starting(_) => {
                let content: Element<'_, Control> = column![
                    text("Starting terminal…"),
                    button("Cancel startup").on_press(Control::CloseSession(id))
                ]
                .into();
                content.map(Message::Control)
            }
            Owner::Disposing(_) => text("Closing terminal…").into(),
            Owner::Failed(error) => {
                let failed: Element<'_, Control> = column![
                    text(error.to_string()),
                    column![
                        button("Retry").on_press(Control::Retry(id)),
                        button("Use default shell").on_press(Control::UseDefault(id))
                    ]
                    .spacing(8)
                ]
                .spacing(8)
                .into();
                scrollable(failed.map(Message::Control)).height(Fill).into()
            }
            Owner::Attached(terminal) => {
                let mut content = column![
                    TerminalView::show(terminal).map(move |event| Message::Terminal((id, event)))
                ];
                let mut footer = column![];
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
                    footer = footer.push(status.map(Message::Control));
                } else if entry.ended {
                    footer = footer.push(text("Terminal exited — finishing output and cleanup…"));
                }
                if let Some(notice) = entry.notice.as_deref() {
                    footer = footer.push(text(notice));
                }
                if self.footer_height(id) > 0.0 {
                    content = content.push(scrollable(footer).height(FOOTER));
                }
                content.height(Fill).into()
            }
        }
    }
}
