//! Pane presentation and canonical layout; no backend or process ownership.
use iced::widget::pane_grid::{self, Axis, Node, Pane, Split};
use iced::{Rectangle, Size};

use crate::identity::{SessionId, TabId};

mod resize;
pub use resize::ResizeGeometry;
pub(crate) use resize::nearest as nearest_split;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneState {
    pub session: SessionId,
}

/// Pane handles are only meaningful in their originating tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneTarget {
    pub tab: TabId,
    pub pane: Pane,
}

/// Measured available grid area and per-pane minimum, including chrome.
/// Uses the same rounding/gutter calculation as the pinned pane_grid widget.
#[derive(Clone, Copy, Debug)]
pub struct SplitGeometry {
    pub area: Size,
    pub minimum: Size,
    pub spacing: f32,
}

impl SplitGeometry {
    pub(crate) fn admits(self, tab: &TabState, pane: Pane, axis: Axis) -> bool {
        let positive = |size: Size| {
            size.width.is_finite()
                && size.height.is_finite()
                && size.width > 0.0
                && size.height > 0.0
        };
        if !positive(self.area)
            || !positive(self.minimum)
            || !self.spacing.is_finite()
            || self.spacing < 0.0
            || tab.grid.maximized().is_some()
        {
            return false;
        }
        let fits = |r: Rectangle| r.width >= self.minimum.width && r.height >= self.minimum.height;
        let regions = tab.grid.layout().pane_regions(self.spacing, 0.0, self.area);
        regions.values().all(|r| fits(*r))
            && regions.get(&pane).is_some_and(|r| {
                let (a, b, _) = axis.split(r, 0.5, self.spacing, 0.0, 0.0);
                fits(a) && fits(b)
            })
    }
}

#[derive(Debug)]
pub struct TabState {
    pub(crate) grid: pane_grid::State<PaneState>,
    pub(crate) focused: Pane,
}

impl TabState {
    pub(crate) fn new(session: SessionId) -> Self {
        let (grid, focused) = pane_grid::State::new(PaneState { session });
        Self { grid, focused }
    }

    pub fn grid(&self) -> &pane_grid::State<PaneState> {
        &self.grid
    }

    pub fn focused(&self) -> Pane {
        self.focused
    }

    pub(crate) fn split(&mut self, pane: Pane, axis: Axis, state: PaneState) -> Option<Pane> {
        let (created, _) = self.grid.split(axis, pane, state)?;
        self.focused = created;
        Some(created)
    }

    pub(crate) fn remove(&mut self, pane: Pane) {
        // The workspace removes the entire tab for its final pane.
        let (_, sibling) = self.grid.close(pane).expect("validated non-final pane");
        if self.focused == pane {
            self.focused = sibling;
        }
    }
}

// Unlike Node::splits(), this lookup needs no temporary traversal Vec.
pub(crate) fn contains_split(node: &Node, split: Split) -> bool {
    match node {
        Node::Pane(_) => false,
        Node::Split { id, a, b, .. } => {
            *id == split || contains_split(a, split) || contains_split(b, split)
        }
    }
}
