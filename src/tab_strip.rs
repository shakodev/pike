//! Compact tab presentation. Callers retain all selection/modal/lifecycle policy.
use std::ops::Range;

use iced::widget::{button, column, container, responsive, row, scrollable, text, tooltip};
use iced::{Element, Fill};

use crate::identity::TabId;
use crate::tabs::{Activity, Placement, Selection, Title};
use crate::ui::{self, style, tokens};

mod reveal;
pub use reveal::reveal;

fn row_id(tab: TabId) -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::from(format!("tab-overflow-row-{}", tab.get()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabAction {
    BeginRename(TabId),
    ResetName(TabId),
    Move {
        tab: TabId,
        anchor: TabId,
        placement: Placement,
    },
    Select(Selection),
    OpenOverflow,
}

pub struct Item {
    pub id: TabId,
    pub title: Title,
    pub activity: Activity,
}

impl Item {
    fn label(&self, position: usize) -> String {
        let mut label = format!("{} · {}", position + 1, self.title.as_str());
        for (set, indicator) in [
            (self.activity.unread, " · unread"),
            (self.activity.bell, " · bell"),
            (self.activity.exited, " · exited"),
        ] {
            if set {
                label.push_str(indicator);
            }
        }
        label
    }
}

/// Keep the active item in the compact strip without unbounded row wrapping.
/// The overflow list exposes every retained tab, independently of this range.
fn visible_range(length: usize, active: usize, capacity: usize) -> Range<usize> {
    let visible = capacity.clamp(1, 5);
    let start = active
        .saturating_sub(visible / 2)
        .min(length.saturating_sub(visible));
    start..(start + visible).min(length)
}

pub fn view<'a, M: Clone + 'a>(
    items: Vec<Item>,
    active: Option<TabId>,
    activate: fn(TabId) -> M,
    close: fn(TabId) -> M,
    action: fn(TabAction) -> M,
) -> Element<'a, M> {
    container(responsive(move |size| {
        compact(&items, active, activate, close, action, size.width)
    }))
    .height(tokens::TAB_HEIGHT)
    .padding([4, 4])
    .style(style::surface)
    .id("tab-strip")
    .into()
}

fn compact<'a, M: Clone + 'a>(
    items: &[Item],
    active: Option<TabId>,
    activate: fn(TabId) -> M,
    close: fn(TabId) -> M,
    action: fn(TabAction) -> M,
    width: f32,
) -> Element<'a, M> {
    let selected = items.iter().position(|item| Some(item.id) == active);
    // Preserve a useful active label before allocating space to less frequent
    // management actions. Overflow always has its own unclipped target.
    let slot = 150.0 + tokens::CONTROL + tokens::SPACE;
    let management_width =
        (width - slot - tokens::CONTROL - 2.0 * tokens::SPACE).clamp(0.0, 4.0 * tokens::CONTROL);
    let available = (width - management_width - tokens::CONTROL - 2.0 * tokens::SPACE).max(0.0);
    let capacity = ((available / 180.0).floor() as usize).clamp(1, 5);
    let visible = visible_range(items.len(), selected.unwrap_or(0), capacity);
    let tab_width = ((available - tokens::SPACE * visible.len().saturating_sub(1) as f32)
        / visible.len().max(1) as f32
        - tokens::CONTROL)
        .clamp(0.0, 150.0);
    let count = items.len();
    let mut controls = row![
        ui::icon(
            "‹",
            "Previous tab",
            action(TabAction::Select(Selection::Previous))
        ),
        ui::icon("›", "Next tab", action(TabAction::Select(Selection::Next))),
    ]
    .spacing(0);
    if let Some(index) = selected {
        let id = items[index].id;
        controls = controls
            .push(ui::icon(
                "✎",
                "Rename tab",
                action(TabAction::BeginRename(id)),
            ))
            .push(ui::icon(
                "↺",
                "Use automatic tab name",
                action(TabAction::ResetName(id)),
            ));
        if let Some(left) = index.checked_sub(1) {
            controls = controls.push(ui::icon(
                "←",
                "Move tab left",
                action(TabAction::Move {
                    tab: id,
                    anchor: items[left].id,
                    placement: Placement::Before,
                }),
            ));
        }
        if let Some(right) = items.get(index + 1) {
            controls = controls.push(ui::icon(
                "→",
                "Move tab right",
                action(TabAction::Move {
                    tab: id,
                    anchor: right.id,
                    placement: Placement::After,
                }),
            ));
        }
    }
    let tabs = items
        .iter()
        .enumerate()
        .filter(|(index, _)| visible.contains(index))
        .fold(row![], |tabs, (index, item)| {
            let selected = active == Some(item.id);
            let label = item.label(index);
            tabs.push(
                row![
                    tooltip(
                        button(container(ui::label(label.clone())).width(Fill).clip(true))
                            .on_press(activate(item.id))
                            .height(tokens::CONTROL)
                            .padding([0, 8])
                            .style(move |theme, status| style::tab(theme, status, selected))
                            .width(tab_width),
                        container(text(label).font(tokens::FONT).size(tokens::SMALL))
                            .padding([4, 8])
                            .style(style::tooltip),
                        tooltip::Position::Bottom,
                    ),
                    ui::reveal_icon("×", "Close tab", close(item.id), selected),
                ]
                .spacing(0),
            )
        })
        .spacing(tokens::SPACE);
    row![
        container(tabs)
            .width(Fill)
            .height(tokens::CONTROL)
            .clip(true),
        // Less frequent tab-management actions remain reachable in a bounded
        // horizontal scroller; overflow itself is always outside that clip.
        scrollable(controls)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(2).scroller_width(2)
            ))
            .width(management_width)
            .height(tokens::CONTROL),
        container(ui::icon(
            "⋯",
            format!("All tabs ({count})"),
            action(TabAction::OpenOverflow)
        ))
        .id("tab-overflow-control"),
    ]
    .spacing(tokens::SPACE)
    .align_y(iced::Alignment::Center)
    .into()
}

/// Finite, vertically scrollable list. Selection/close/cancel messages return to
/// the caller's existing input owner; this component starts no subscription.
pub fn overflow<'a, M: Clone + 'a>(
    items: Vec<Item>,
    selected: Option<TabId>,
    scroll_id: iced::advanced::widget::Id,
    activate: fn(TabId) -> M,
    close: fn(TabId) -> M,
    cancel: M,
) -> Element<'a, M> {
    let rows = items
        .into_iter()
        .enumerate()
        .fold(column![], |rows, (index, item)| {
            rows.push(
                container(
                    row![
                        button(
                            container(ui::label(item.label(index)))
                                .width(Fill)
                                .clip(true)
                        )
                        .on_press(activate(item.id))
                        .height(tokens::CONTROL)
                        .padding([0, 8])
                        .style(move |theme, status| style::tab(
                            theme,
                            status,
                            selected == Some(item.id)
                        ))
                        .width(Fill),
                        ui::icon("×", "Close tab", close(item.id)),
                    ]
                    .spacing(8),
                )
                .id(row_id(item.id)),
            )
        })
        .spacing(4);
    container(
        column![
            text("All tabs").size(tokens::HEADING),
            text("↑/↓ selects · Enter opens · Escape cancels").size(tokens::SMALL),
            scrollable(rows).id(scroll_id).height(Fill).width(Fill),
            button(text("Cancel").size(tokens::BODY))
                .style(style::quiet)
                .height(tokens::CONTROL)
                .on_press(cancel),
        ]
        .spacing(tokens::INSET),
    )
    .max_width(700)
    .max_height(440)
    .height(Fill)
    .width(Fill)
    .padding(tokens::INSET)
    .style(style::workspace)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_range_is_bounded_and_always_contains_the_selected_tab() {
        for capacity in 1..=5 {
            for length in 1..=32 {
                for active in 0..length {
                    let visible = visible_range(length, active, capacity);
                    assert!(visible.contains(&active));
                    assert!(visible.end <= length);
                    assert!(visible.len() <= capacity);
                }
            }
        }
        assert_eq!(visible_range(0, 0, 1), 0..0);
    }
}
