//! Admission and dispatch only; all native ownership stays in the launch pool.
use super::{App, Message};
use crate::launch::LaunchRequest;
use crate::layout::{PaneTarget, SplitGeometry};
use crate::session::Effect;
use crate::workspace::Error;
use iced::widget::pane_grid::Axis;
use iced::{Size, Task};

pub(super) const GUTTER: f32 = 4.0;
pub(super) const HEADER: f32 = 28.0;
pub(super) const BORDER: f32 = 1.0;
pub(super) const FOOTER: f32 = 48.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Right,
    Down,
}

impl Direction {
    pub fn axis(self) -> Axis {
        match self {
            Self::Right => Axis::Vertical,
            Self::Down => Axis::Horizontal,
        }
    }
}

impl App {
    pub(super) fn begin_split(
        &mut self,
        source: PaneTarget,
        direction: Direction,
    ) -> Option<LaunchRequest> {
        if self.exiting {
            return None;
        }
        let Some(state) = self.model.workspace().pane(source) else {
            self.notice =
                Some("This pane is no longer available. Select a terminal and try again.");
            return None;
        };
        let id = state.session;
        let Some(profile) = self.profiles.get(&id).cloned() else {
            self.notice = Some("Wait for the source terminal to start, then try again.");
            return None;
        };
        let Some(content) = self.terminal(id).and_then(|t| t.renderable_content()) else {
            self.notice = Some("Wait for the source terminal to start, then try again.");
            return None;
        };
        let Some((tab, area)) = self.grid_size.filter(|(tab, _)| *tab == source.tab) else {
            self.notice = Some("Wait for the window layout, then try splitting again.");
            return None;
        };
        if self.model.workspace().active() != Some(tab) {
            return None;
        }
        let cell = content.terminal_size;
        let minimum = self.model.workspace().tab(tab)?.grid().iter().fold(
            Size::new(
                20.0 * cell.cell_width + 2.0 * BORDER,
                5.0 * cell.cell_height + HEADER + 2.0 * BORDER,
            ),
            |minimum, (_, pane)| {
                let cell = self
                    .terminal(pane.session)
                    .and_then(|t| t.renderable_content())
                    .map(|content| content.terminal_size)
                    .unwrap_or(cell);
                Size::new(
                    minimum.width.max(20.0 * cell.cell_width + 2.0 * BORDER),
                    minimum.height.max(
                        5.0 * cell.cell_height
                            + HEADER
                            + 2.0 * BORDER
                            + self.footer_height(pane.session),
                    ),
                )
            },
        );
        let geometry = SplitGeometry {
            area,
            minimum,
            spacing: GUTTER,
        };
        let reservation = match self
            .model
            .split_with_geometry(source, direction.axis(), geometry)
        {
            Ok((_, Effect::Spawn(ids))) => ids,
            Ok(_) => unreachable!("split produces a spawn effect"),
            Err(error) => {
                self.notice = Some(match error {
                    Error::Geometry => {
                        "Not enough space for two 20×5 terminals. Enlarge the window or split in the other direction."
                    }
                    Error::SessionLimit => {
                        "The 32-session limit includes starting, closing, exited and error panes. Close a pane, wait for cleanup, and try again."
                    }
                    _ => "This pane cannot be split. Select an available terminal and try again.",
                });
                return None;
            }
        };
        // Keep typing in the source during slow preparation. Only a successfully
        // attached, still-owned reservation selects the new pane.
        let _ = self.model.focus(source);
        self.focus_after_attach(reservation.session);
        self.submit_profile(reservation, profile)
    }

    pub(super) fn split(&mut self, source: PaneTarget, direction: Direction) -> Task<Message> {
        let launch = self
            .begin_split(source, direction)
            .map(Self::receive)
            .unwrap_or_else(Task::none);
        // Toolbar clicks unfocus the terminal widget. Restore the source while
        // preparation runs (or after refusal), so its input stays usable.
        let focus = if self.active_target() == Some(source) {
            self.select(source)
        } else {
            Task::none()
        };
        Task::batch([focus, launch])
    }
}
