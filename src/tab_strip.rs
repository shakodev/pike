//! Compact tab presentation. Callers retain all selection/modal/lifecycle policy.
use std::ops::Range;

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Fill};

use crate::identity::TabId;
use crate::tabs::{Activity, Placement, Selection, Title};

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
fn visible_range(length: usize, active: usize) -> Range<usize> {
    const VISIBLE: usize = 5;
    let start = active
        .saturating_sub(VISIBLE / 2)
        .min(length.saturating_sub(VISIBLE));
    start..(start + VISIBLE).min(length)
}

pub fn view<'a, M: Clone + 'a>(
    items: Vec<Item>,
    active: Option<TabId>,
    activate: fn(TabId) -> M,
    close: fn(TabId) -> M,
    action: fn(TabAction) -> M,
) -> Element<'a, M> {
    let selected = items.iter().position(|item| Some(item.id) == active);
    let visible = visible_range(items.len(), selected.unwrap_or(0));
    let mut controls = row![
        button("‹ Previous").on_press(action(TabAction::Select(Selection::Previous))),
        button("Next ›").on_press(action(TabAction::Select(Selection::Next))),
    ]
    .spacing(4);
    if let Some(index) = selected {
        let id = items[index].id;
        controls = controls
            .push(button("Rename").on_press(action(TabAction::BeginRename(id))))
            .push(button("Use automatic name").on_press(action(TabAction::ResetName(id))));
        let left = index.checked_sub(1).map(|left| {
            action(TabAction::Move {
                tab: id,
                anchor: items[left].id,
                placement: Placement::Before,
            })
        });
        let right = items.get(index + 1).map(|right| {
            action(TabAction::Move {
                tab: id,
                anchor: right.id,
                placement: Placement::After,
            })
        });
        controls = controls
            .push(button("Move left").on_press_maybe(left))
            .push(button("Move right").on_press_maybe(right));
    }
    let count = items.len();
    let tabs = items
        .into_iter()
        .enumerate()
        .filter(|(index, _)| visible.contains(index))
        .fold(row![], |tabs, (index, item)| {
            let label = item.label(index);
            tabs.push(
                row![
                    button(
                        text(label)
                            .size(13)
                            .wrapping(iced::widget::text::Wrapping::None)
                    )
                    .on_press(activate(item.id))
                    .style(if active == Some(item.id) {
                        button::primary
                    } else {
                        button::secondary
                    })
                    .width(150),
                    button(text("×").size(13))
                        .on_press(close(item.id))
                        .padding([5, 6]),
                ]
                .spacing(2),
            )
        })
        .spacing(4);
    let tab_row = row![
        scrollable(tabs).horizontal().width(Fill).height(38),
        button(text(format!("All tabs ({count})"))).on_press(action(TabAction::OpenOverflow)),
    ]
    .spacing(8);
    column![
        tab_row,
        scrollable(controls).horizontal().width(Fill).height(38)
    ]
    .spacing(2)
    .into()
}

/// Finite, vertically scrollable list. Selection/close/cancel messages return to
/// the caller's existing input owner; this component starts no subscription.
pub fn overflow<'a, M: Clone + 'a>(
    items: Vec<Item>,
    selected: Option<TabId>,
    activate: fn(TabId) -> M,
    close: fn(TabId) -> M,
    cancel: M,
) -> Element<'a, M> {
    let rows = items
        .into_iter()
        .enumerate()
        .fold(column![], |rows, (index, item)| {
            rows.push(
                row![
                    button(text(item.label(index)).wrapping(iced::widget::text::Wrapping::None))
                        .on_press(activate(item.id))
                        .style(if selected == Some(item.id) {
                            button::primary
                        } else {
                            button::secondary
                        })
                        .width(Fill),
                    button("Close tab").on_press(close(item.id)),
                ]
                .spacing(8),
            )
        })
        .spacing(4);
    container(
        column![
            text("All tabs").size(22),
            text("↑/↓ selects · Enter opens · Escape cancels"),
            scrollable(rows).height(300).width(Fill),
            button("Cancel").on_press(cancel),
        ]
        .spacing(12),
    )
    .max_width(700)
    .width(Fill)
    .padding(24)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_range_is_bounded_and_always_contains_the_selected_tab() {
        for length in 1..=32 {
            for active in 0..length {
                let visible = visible_range(length, active);
                assert!(visible.contains(&active));
                assert!(visible.end <= length);
                assert!(visible.len() <= 5);
            }
        }
        assert_eq!(visible_range(0, 0), 0..0);
    }
}
