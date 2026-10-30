//! Resize-only commands. Monitor DPI is already applied by Iced's logical layout;
//! FontSettings.scale_factor is relative line height and is never monitor DPI.
use iced::Size;
use iced::widget::pane_grid::{Axis, ResizeEvent};

use super::{
    App,
    split::{BORDER, GUTTER, HEADER},
};
use crate::identity::TabId;
use crate::layout::{ResizeGeometry, nearest_split};

pub(super) mod capture;
pub(super) use capture::{capture, divider_focus, resize_key};

/// Move the nearest divider on this axis by one focused terminal cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeStep {
    Left,
    Right,
    Up,
    Down,
}

pub(super) struct Presentation {
    pub tab: TabId,
    pub size: Size,
    pub grid: iced::widget::pane_grid::State<crate::layout::PaneState>,
    pub frozen: bool,
}

impl ResizeStep {
    fn axis(self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Vertical,
            _ => Axis::Horizontal,
        }
    }
    fn sign(self) -> f32 {
        match self {
            Self::Left | Self::Up => -1.0,
            _ => 1.0,
        }
    }
}

pub(super) fn valid_area(size: Size) -> bool {
    size.width.is_finite() && size.height.is_finite() && size.width > 0.0 && size.height > 0.0
}

impl App {
    /// Apply measured font changes on the UI thread and refresh cell constraints.
    /// Settings UI is intentionally deferred; fixtures use this same host path.
    pub fn change_terminal_font(
        &mut self,
        id: crate::identity::SessionId,
        font: iced_term::settings::FontSettings,
    ) {
        if self.exiting {
            return;
        }
        if let Some(crate::terminal::Entry {
            owner: crate::terminal::Owner::Attached(terminal),
            ..
        }) = self.sessions.entries.get_mut(&id)
        {
            terminal.handle(iced_term::Command::ChangeFont(font));
            self.refresh_resize_layout();
        }
    }
    pub(super) fn refresh_resize_layout(&mut self) {
        let next = self.grid_size.and_then(|(tab, area)| {
            let source = self.model.workspace().tab(tab)?;
            let mut geometry = self.resize_geometry(tab, area)?;
            if let Some((size, grid)) = geometry.presentation(source.grid()) {
                return Some(Presentation {
                    tab,
                    size,
                    grid,
                    frozen: false,
                });
            }
            // Reuse only bounded geometry. Pane/session membership always
            // comes from today's canonical tree, including close and restart.
            let previous = self.resize_layout.as_ref().filter(|p| p.tab == tab)?;
            geometry.area = previous.size;
            let (size, grid) = geometry
                .presentation(source.grid())
                .unwrap_or_else(|| (previous.size, source.grid().clone()));
            Some(Presentation {
                tab,
                size,
                grid,
                frozen: true,
            })
        });
        self.resize_layout = next;
    }
    pub(super) fn resize_geometry(&self, tab: TabId, area: Size) -> Option<ResizeGeometry> {
        let tab = self.model.workspace().tab(tab)?;
        // Starting/error panes share the current measured metrics; their native
        // session is untouched. A first, unattached tab uses the existing layout.
        let fallback = tab.grid().iter().find_map(|(_, pane)| {
            self.terminal(pane.session)?
                .renderable_content()
                .map(|c| c.terminal_size)
        })?;
        let minimum = tab
            .grid()
            .iter()
            .map(|(handle, pane)| {
                let cell = self
                    .terminal(pane.session)
                    .and_then(|t| t.renderable_content())
                    .map(|c| c.terminal_size)
                    .unwrap_or(fallback);
                (
                    *handle,
                    Size::new(
                        20.0 * cell.cell_width + 2.0 * BORDER,
                        5.0 * cell.cell_height
                            + HEADER
                            + 2.0 * BORDER
                            + self.footer_height(pane.session),
                    ),
                )
            })
            .collect();
        Some(ResizeGeometry {
            area,
            minimum,
            spacing: GUTTER,
        })
    }

    pub(super) fn resize_divider(&mut self, tab: TabId, event: ResizeEvent) -> bool {
        if !self.resize_allowed() || self.model.workspace().active() != Some(tab) {
            return false;
        }
        let Some((_, area)) = self.grid_size.filter(|(id, _)| *id == tab) else {
            return false;
        };
        let Some(geometry) = self.resize_geometry(tab, area) else {
            return false;
        };
        let accepted = self
            .model
            .resize_with_geometry(tab, event.split, event.ratio, &geometry)
            .is_ok();
        if accepted {
            // Divider presses lie outside terminal bounds. Restore both model
            // input authority and the actual widget focus after the drag.
            self.synchronize();
        }
        accepted
    }

    fn resize_allowed(&self) -> bool {
        !self.exiting
            && self.window_focus.is_focused()
            && self
                .active_session()
                .and_then(|id| self.terminal(id))
                .is_none_or(|terminal| !terminal.is_composing())
    }

    pub(super) fn resize_step(&mut self, step: ResizeStep) -> iced::Task<super::Message> {
        if !self.resize_allowed() {
            return iced::Task::none();
        }
        let Some(target) = self.active_target() else {
            return iced::Task::none();
        };
        let Some((_, area)) = self.grid_size.filter(|(id, _)| *id == target.tab) else {
            return iced::Task::none();
        };
        let Some(geometry) = self.resize_geometry(target.tab, area) else {
            return iced::Task::none();
        };
        let tab = self.model.workspace().tab(target.tab).unwrap();
        let Some(split) = nearest_split(tab.grid().layout(), target.pane, step.axis()) else {
            return iced::Task::none();
        };
        let Some((canvas, grid)) = geometry.presentation(tab.grid()) else {
            return iced::Task::none();
        };
        let Some((_, bounds, ratio)) = grid
            .layout()
            .split_regions(GUTTER, 0.0, canvas)
            .remove(&split)
        else {
            return iced::Task::none();
        };
        let Some(cell) = self
            .active_session()
            .and_then(|id| self.terminal(id))
            .and_then(|terminal| terminal.renderable_content())
            .map(|c| c.terminal_size)
        else {
            return iced::Task::none();
        };
        let delta = match step.axis() {
            Axis::Vertical => cell.cell_width / bounds.width,
            Axis::Horizontal => cell.cell_height / bounds.height,
        };
        if self.resize_divider(
            target.tab,
            ResizeEvent {
                split,
                ratio: (ratio + step.sign() * delta).clamp(0.0, 1.0),
            },
        ) {
            self.focus_active()
        } else {
            iced::Task::none()
        }
    }
}
