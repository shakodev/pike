//! Steady cursor presentation. Blink requests retain their shape but schedule
//! no timer; focus changes presentation without changing the terminal model.
use super::glyphs;
use crate::{theme::Theme, RenderableContent};
use alacritty_terminal::index::Point as GridPoint;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::{
    Color as AnsiColor, CursorShape, NamedColor,
};
use iced::widget::canvas::{Frame, Path, Stroke};
use iced::{Color, Point, Rectangle, Size};

#[derive(Clone, Copy)]
pub(super) struct Cursor {
    pub point: GridPoint,
    pub shape: CursorShape,
    rect: Rectangle,
    color: Color,
    glyph: Color,
    background: Color,
}

impl Cursor {
    pub fn new(
        content: &RenderableContent,
        theme: &Theme,
        focused: bool,
        colors: impl Fn(&Cell, GridPoint) -> (Color, Color),
    ) -> Option<Self> {
        if content.cursor_shape == CursorShape::Hidden {
            return None;
        }
        let mut point = content.cursor_point;
        if content.cursor.flags.contains(Flags::WIDE_CHAR_SPACER)
            && point.column.0 > 0
        {
            point.column.0 -= 1;
        }
        let row = point
            .line
            .0
            .checked_add(content.viewport.display_offset() as i32)?;
        let columns = content.viewport.columns();
        if row < 0
            || row as usize >= content.viewport.lines()
            || point.column.0 >= columns
        {
            return None;
        }
        let cell = content
            .viewport
            .cells()
            .get(row as usize * columns + point.column.0)?;
        let width = glyphs::columns(cell, columns - point.column.0);
        if width == 0 {
            return None;
        }
        let (_, bg) = colors(cell, point);
        // DIM plus INVERSE/selection can make the cell background translucent.
        // Match the container composition, then use an opaque cursor glyph.
        let base = theme.get_color(AnsiColor::Named(NamedColor::Background));
        let bg = Color::from_rgb(
            bg.r * bg.a + base.r * (1.0 - bg.a),
            bg.g * bg.a + base.g * (1.0 - bg.a),
            bg.b * bg.a + base.b * (1.0 - bg.a),
        );
        let preferred = content.cursor_color.map_or_else(
            || theme.get_color(AnsiColor::Named(NamedColor::Foreground)),
            |rgb| Color::from_rgb8(rgb.r, rgb.g, rgb.b),
        );
        // OSC12/configured foreground is honored when visible on this cell.
        // Explicit low-contrast choices fall back to black/white, independently
        // of INVERSE/selection swaps and of APP_CURSOR keyboard mode.
        let color = contrasting(preferred, bg, 3.0);
        let glyph = contrasting(bg, color, 4.5);
        let size = content.terminal_size;
        Some(Self {
            point,
            shape: if focused {
                content.cursor_shape
            } else {
                CursorShape::HollowBlock
            },
            rect: Rectangle::new(
                Point::new(
                    point.column.0 as f32 * size.cell_width,
                    row as f32 * size.cell_height,
                ),
                Size::new(width as f32 * size.cell_width, size.cell_height),
            ),
            color,
            glyph,
            background: bg,
        })
    }

    pub fn glyph_color(&self, point: GridPoint, normal: Color) -> Color {
        if self.point != point {
            return normal;
        }
        if self.shape == CursorShape::Block {
            self.glyph
        } else {
            contrasting(normal, self.background, 4.5)
        }
    }

    pub fn draw(&self, frame: &mut Frame) {
        let rect = self.rect;
        let thickness = (rect.height * 0.08)
            .max(1.0)
            .min(rect.width.min(rect.height));
        let path = match self.shape {
            CursorShape::Block => Path::rectangle(rect.position(), rect.size()),
            CursorShape::Underline => Path::rectangle(
                Point::new(rect.x, rect.y + rect.height - thickness),
                Size::new(rect.width, thickness),
            ),
            CursorShape::Beam => Path::rectangle(
                rect.position(),
                Size::new(thickness, rect.height),
            ),
            CursorShape::HollowBlock => {
                let inset = thickness * 0.5;
                let path = Path::rectangle(
                    Point::new(rect.x + inset, rect.y + inset),
                    Size::new(
                        (rect.width - thickness).max(0.0),
                        (rect.height - thickness).max(0.0),
                    ),
                );
                frame.stroke(
                    &path,
                    Stroke::default()
                        .with_color(self.color)
                        .with_width(thickness),
                );
                return;
            },
            CursorShape::Hidden => return,
        };
        frame.fill(&path, self.color);
    }
}

fn luminance(color: Color) -> f32 {
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    linear(color.r) * 0.2126
        + linear(color.g) * 0.7152
        + linear(color.b) * 0.0722
}
fn contrast(a: Color, b: Color) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn contrasting(preferred: Color, background: Color, minimum: f32) -> Color {
    if contrast(preferred, background) >= minimum {
        preferred
    } else if contrast(Color::BLACK, background)
        >= contrast(Color::WHITE, background)
    {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

#[cfg(test)]
mod tests;
