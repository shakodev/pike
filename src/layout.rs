//! Pane presentation and canonical layout; no backend or process ownership.
use iced::widget::pane_grid::{self, Axis, Node, Pane, Split};

use crate::identity::{SessionId, TabId};

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
