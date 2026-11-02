use iced::widget::{PaneGrid, button, column, container, pane_grid, row, scrollable, sensor, text};
use iced::{Element, Fill};
use iced_term::TerminalView;

use super::super::{
    App, Control, Direction, Message,
    split::{BORDER, FOOTER, GUTTER, HEADER},
};
use crate::identity::{SessionId, TabId};
use crate::layout::PaneTarget;
use crate::session::Lifecycle;
use crate::terminal::Owner;
use crate::ui::{self, style, tokens};

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
        let boundary = super::gesture::Boundary::default();
        let grid = PaneGrid::new(source, |handle, pane, _| {
            let focused = handle == tab.focused();
            let target = PaneTarget {
                tab: tab_id,
                pane: handle,
            };
            let mut controls = row![];
            if focused {
                controls = controls
                    .push(ui::icon(
                        "⊞",
                        "Split right",
                        Control::Split(target, Direction::Right),
                    ))
                    .push(ui::icon(
                        "⊟",
                        "Split down",
                        Control::Split(target, Direction::Down),
                    ));
            }
            let controls: Element<'_, Control> = container(
                controls
                    .push(ui::icon(
                        if source.maximized().is_some() {
                            "↙"
                        } else {
                            "↗"
                        },
                        "Maximize / restore pane",
                        Control::Zoom(target),
                    ))
                    .push(ui::icon(
                        "×",
                        "Close pane",
                        Control::CloseSession(pane.session),
                    ))
                    .spacing(0),
            )
            .height(HEADER)
            .align_y(iced::Alignment::Center)
            .into();
            let title = pane_grid::TitleBar::new(super::gesture::title(
                container(text(format!("Terminal {}", pane.session.get())).size(tokens::SMALL))
                    .id(format!("pane-header-{}", pane.session.get()))
                    .height(HEADER)
                    .padding([0.0, tokens::INSET])
                    .align_y(iced::Alignment::Center),
            ))
            .controls(controls.map(Message::Control))
            .style(move |theme| style::header(theme, focused));
            // Focus comes from the existing pane/keyboard authority. Inactive
            // close controls reveal on hover; active controls remain visible.
            let title = if focused {
                title.always_show_controls()
            } else {
                title
            };
            pane_grid::Content::new(
                divider.content(
                    boundary.body(
                        container(self.pane(pane.session))
                            .padding(BORDER)
                            .clip(true),
                    ),
                ),
            )
            .title_bar(title)
            .style(move |theme| style::pane(theme, focused))
        })
        .spacing(GUTTER)
        .min_size(0)
        .style(style::dividers)
        .on_resize(8, move |event| {
            Message::Control(Control::Resize(tab_id, event))
        })
        .on_click(move |pane| Message::Control(Control::Focus(PaneTarget { tab: tab_id, pane })))
        .on_drag(move |event| {
            let pane = match event {
                pane_grid::DragEvent::Picked { pane }
                | pane_grid::DragEvent::Dropped { pane, .. }
                | pane_grid::DragEvent::Canceled { pane } => pane,
            };
            let target = PaneTarget { tab: tab_id, pane };
            let owner = self
                .model
                .sessions()
                .get(source.get(pane).expect("view pane").session)
                .expect("view owner")
                .reservation();
            Message::Control(Control::PaneDrag(
                self.pane_gestures.action(target, owner, event),
            ))
        })
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
        let grid = boundary.grid(
            grid,
            &self.pane_gestures,
            &self.window_focus,
            active,
            !self.interaction_blocked(),
        );
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
            return container(text("Starting terminal…").size(tokens::BODY))
                .padding(tokens::INSET)
                .style(style::workspace)
                .into();
        };
        match &entry.owner {
            Owner::Starting(_) => {
                let content: Element<'_, Control> = column![
                    text("Starting terminal…").size(tokens::BODY),
                    button(text("Cancel startup").size(tokens::BODY))
                        .height(tokens::CONTROL)
                        .style(style::quiet)
                        .on_press(Control::CloseSession(id))
                ]
                .spacing(tokens::INSET)
                .into();
                container(content.map(Message::Control))
                    .padding(tokens::INSET)
                    .width(Fill)
                    .height(Fill)
                    .style(style::workspace)
                    .into()
            }
            Owner::Disposing(_) => container(text("Closing terminal…").size(tokens::BODY))
                .padding(tokens::INSET)
                .width(Fill)
                .height(Fill)
                .style(style::workspace)
                .into(),
            Owner::Failed(error) => {
                let failed: Element<'_, Control> = column![
                    text("Terminal could not start").size(tokens::HEADING),
                    text(ui::plain_title(&error.to_string(), 512)).size(tokens::BODY),
                    row![
                        button(text("Retry").size(tokens::BODY))
                            .height(tokens::CONTROL)
                            .style(style::quiet)
                            .on_press(Control::Retry(id)),
                        button(text("Use default shell").size(tokens::BODY))
                            .height(tokens::CONTROL)
                            .style(style::quiet)
                            .on_press(Control::UseDefault(id))
                    ]
                    .spacing(tokens::SPACE)
                    .wrap()
                ]
                .spacing(tokens::INSET)
                .into();
                scrollable(
                    container(failed.map(Message::Control))
                        .padding(tokens::INSET)
                        .style(style::workspace),
                )
                .height(Fill)
                .into()
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
                    let status: Element<'_, Control> = row![
                        text(status).size(tokens::SMALL),
                        button(text("Restart").size(tokens::SMALL))
                            .height(tokens::CONTROL)
                            .style(style::quiet)
                            .on_press(Control::Retry(id))
                    ]
                    .align_y(iced::Alignment::Center)
                    .spacing(tokens::INSET)
                    .into();
                    footer = footer.push(status.map(Message::Control));
                } else if entry.ended {
                    footer = footer.push(
                        text("Terminal exited — finishing output and cleanup…").size(tokens::SMALL),
                    );
                }
                if let Some(notice) = entry.notice.as_deref() {
                    footer = footer.push(text(ui::plain_title(notice, 512)).size(tokens::SMALL));
                }
                if self.footer_height(id) > 0.0 {
                    content = content.push(
                        scrollable(
                            container(footer)
                                .padding(tokens::SPACE)
                                .style(style::surface),
                        )
                        .height(FOOTER),
                    );
                }
                content.height(Fill).into()
            }
        }
    }
}
