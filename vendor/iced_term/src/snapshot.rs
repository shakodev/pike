//! Presentation owns only the current viewport. The terminal retains history.
use crate::geometry::TerminalSize;
use crate::settings::HistoryLimit;
use alacritty_terminal::grid::{Dimensions, Indexed};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::{cell::Cell, Term, TermMode};
use std::ops::RangeInclusive;

#[derive(Default)]
pub struct ViewportSnapshot {
    cells: Vec<Cell>,
    columns: usize,
    lines: usize,
    display_offset: usize,
}

impl ViewportSnapshot {
    fn capture<T>(&mut self, terminal: &Term<T>) {
        let grid = terminal.grid();
        self.columns = grid.columns();
        self.lines = grid.screen_lines();
        self.display_offset = grid.display_offset();
        // Retain the allocation across same-sized refreshes. Cell extras use
        // Alacritty's Arc/COW ownership; no history or transcript is cloned.
        self.cells.clear();
        self.cells
            .extend(grid.display_iter().map(|indexed| indexed.cell.clone()));
    }

    pub fn display_iter(
        &self,
    ) -> impl ExactSizeIterator<Item = Indexed<&Cell>> {
        self.cells.iter().enumerate().map(|(index, cell)| Indexed {
            point: Point::new(
                Line(
                    (index / self.columns) as i32 - self.display_offset as i32,
                ),
                Column(index % self.columns),
            ),
            cell,
        })
    }

    pub fn display_offset(&self) -> usize {
        self.display_offset
    }
    pub fn columns(&self) -> usize {
        self.columns
    }
    pub fn lines(&self) -> usize {
        self.lines
    }
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }
}

pub struct RenderableContent {
    pub viewport: ViewportSnapshot,
    pub hovered_hyperlink: Option<RangeInclusive<Point>>,
    pub selectable_range: Option<SelectionRange>,
    pub cursor: Cell,
    /// Absolute terminal coordinates, also when scrolled into history.
    pub cursor_point: Point,
    pub terminal_mode: TermMode,
    pub terminal_size: TerminalSize,
}

impl Default for RenderableContent {
    fn default() -> Self {
        Self {
            viewport: ViewportSnapshot::default(),
            hovered_hyperlink: None,
            selectable_range: None,
            cursor: Cell::default(),
            cursor_point: Point::default(),
            terminal_mode: TermMode::empty(),
            terminal_size: TerminalSize::default(),
        }
    }
}

impl RenderableContent {
    pub(crate) fn capture<T>(
        &mut self,
        terminal: &Term<T>,
        size: TerminalSize,
    ) {
        self.viewport.capture(terminal);
        self.selectable_range = terminal
            .selection
            .as_ref()
            .and_then(|s| s.to_range(terminal));
        self.cursor_point = terminal.grid().cursor.point;
        self.cursor = terminal.grid()[self.cursor_point].clone();
        self.terminal_mode = *terminal.mode();
        self.terminal_size = size;
    }
}

/// Opt-in metadata observation; reading it never creates a viewport snapshot.
#[derive(Debug, Clone, Copy)]
pub struct SnapshotStats {
    pub snapshots: u64,
    pub cells_copied: u64,
    pub viewport_cells: usize,
    /// History in the active screen; alternate screen history is always zero.
    pub history_lines: usize,
    pub history_limit: HistoryLimit,
    pub columns: usize,
    pub screen_lines: usize,
}

#[cfg(test)]
mod profile;
#[cfg(test)]
mod tests;
