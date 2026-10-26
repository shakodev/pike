//! Logical-pixel geometry. Iced already accounts for window DPI in its layout.
use alacritty_terminal::event::WindowSize;
use alacritty_terminal::grid::Dimensions;
use iced_core::Size;

// Bound viewport allocation and stay well inside ConPTY's signed COORD range.
pub const MAX_COLUMNS: u16 = 512;
pub const MAX_LINES: u16 = 512;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalSize {
    pub cell_width: f32,
    pub cell_height: f32,
    pub(crate) num_cols: u16,
    pub(crate) num_lines: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            cell_width: 1.0,
            cell_height: 1.0,
            num_cols: 80,
            num_lines: 50,
        }
    }
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }
    fn columns(&self) -> usize {
        self.num_cols as usize
    }
    fn screen_lines(&self) -> usize {
        self.num_lines as usize
    }
}

impl From<TerminalSize> for WindowSize {
    fn from(size: TerminalSize) -> Self {
        Self {
            num_lines: size.num_lines,
            num_cols: size.num_cols,
            // Integer pixel metadata exists only at the native API boundary.
            cell_width: size.cell_width.round().clamp(1.0, u16::MAX as f32)
                as u16,
            cell_height: size.cell_height.round().clamp(1.0, u16::MAX as f32)
                as u16,
        }
    }
}

pub(crate) fn same_pty_size(a: WindowSize, b: WindowSize) -> bool {
    let same = a.num_cols == b.num_cols && a.num_lines == b.num_lines;
    // ConPTY consumes only rows/columns. Unix also consumes pixel metadata.
    #[cfg(windows)]
    {
        same
    }
    #[cfg(not(windows))]
    {
        same && a.cell_width == b.cell_width && a.cell_height == b.cell_height
    }
}

#[derive(Debug)]
pub(crate) struct Geometry {
    layout: Size<f64>,
    cell: Size<f32>,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            layout: Size::new(80.0, 50.0),
            cell: Size::new(1.0, 1.0),
        }
    }
}

impl Geometry {
    pub(crate) fn initialize(&mut self, cell: Size<f32>, size: TerminalSize) {
        self.cell = cell;
        self.layout = Size::new(
            f64::from(size.num_cols) * f64::from(cell.width),
            f64::from(size.num_lines) * f64::from(cell.height),
        );
    }

    pub(crate) fn update(
        &mut self,
        layout: Option<Size<f32>>,
        cell: Option<Size<f32>>,
    ) -> Option<TerminalSize> {
        if let Some(cell) = cell {
            if !valid_size(cell) {
                return None;
            }
            // Cache a valid font even when the current pane cannot fit it.
            // Restore must use the newest metrics without remeasuring text.
            self.cell = cell;
        }
        let layout = match layout {
            Some(size) if valid_size(size) => {
                Size::new(f64::from(size.width), f64::from(size.height))
            },
            Some(_) => return None,
            None => self.layout,
        };
        let cell = self.cell;
        // f64 division avoids overflow even for extreme finite f32 inputs.
        let cols = (layout.width / f64::from(cell.width)).floor();
        let lines = (layout.height / f64::from(cell.height)).floor();
        if cols < 1.0 || lines < 1.0 {
            return None;
        }
        // A minimized/tiny/invalid layout must not replace the last usable one.
        self.layout = layout;
        self.cell = cell;
        Some(TerminalSize {
            num_cols: cols.min(f64::from(MAX_COLUMNS)) as u16,
            num_lines: lines.min(f64::from(MAX_LINES)) as u16,
            cell_width: cell.width,
            cell_height: cell.height,
        })
    }
}

pub(crate) fn valid_size(size: Size<f32>) -> bool {
    size.width.is_finite()
        && size.height.is_finite()
        && size.width > 0.0
        && size.height > 0.0
}

#[cfg(test)]
mod tests;
