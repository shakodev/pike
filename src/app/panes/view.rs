use iced::widget::{PaneGrid, button, column, container, pane_grid, row, scrollable, sensor, text};
use iced::{Border, Element, Fill};
use iced_term::TerminalView;

use super::super::{
    App, Control, Message, close,
    split::{BORDER, FOOTER, GUTTER, HEADER},
};
use crate::identity::{SessionId, TabId};
use crate::layout::PaneTarget;
use crate::session::Lifecycle;
use crate::terminal::Owner;

impl App {
    pub(in crate::app) fn footer_height(&self, id: SessionId) -> f32 {
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

    pub(in crate::app) fn pane_grid(&self, tab_id: TabId) -> Element<'_, Message> {
        let tab = self.model.workspace().tab(tab_id).expect("active tab");
        let presentation = self.resize_layout.as_ref().filter(|p| p.tab == tab_id);
        let source = presentation.map(|p| &p.grid).unwrap_or_else(|| tab.grid());
        let divider = super::super::resize::Divider::default();
        let grid = PaneGrid::new(source, |handle, pane, _| {
            let focused = handle == tab.focused();
            let header: Element<'_, Control> = container(
                row![
                    container(text(format!("Terminal {}", pane.session.get())).size(12))
                        .padding([5, 10])
                        .width(Fill),
                    close::close_button(pane.session)
                ]
                .spacing(4),
            )
            .height(HEADER)
            .into();
            let content = column![header.map(Message::Control), self.pane(pane.session)];
            pane_grid::Content::new(
                divider.content(container(content).padding(BORDER).clip(true).style(
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
                )),
            )
        })
        .spacing(GUTTER)
        .min_size(0)
        .on_resize(8, move |event| {
            Message::Control(Control::Resize(tab_id, event))
        })
        .on_click(move |pane| Message::Control(Control::Focus(PaneTarget { tab: tab_id, pane })))
        .width(
            presentation
                .map(|p| iced::Length::Fixed(p.size.width))
                .unwrap_or(Fill),
        )
        .height(
            presentation
                .map(|p| iced::Length::Fixed(p.size.height))
                .unwrap_or(Fill),
        );
        // Scrollbar dragging provides safe panning even while the terminal
        // owns wheel events. Both renderer and pointer paths clip to viewport.
        let active = self
            .active_session()
            .and_then(|id| self.terminal(id))
            .filter(|_| self.window_focus.is_focused());
        let grid: Element<'_, Message> = if source.maximized().is_none() {
            divider.grid(grid, source.layout(), active)
        } else {
            grid.into()
        };
        let grid = scrollable(grid)
            .direction(scrollable::Direction::Both {
                vertical: scrollable::Scrollbar::new(),
                horizontal: scrollable::Scrollbar::new(),
            })
            .width(Fill)
            .height(Fill);
        sensor(grid)
            .key(tab_id)
            .on_resize(move |size| Message::GridSize(tab_id, size))
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
