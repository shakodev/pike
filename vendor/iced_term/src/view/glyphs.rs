//! A terminal cell owns its text span; shaping must not choose grid positions.
use alacritty_terminal::term::cell::{Cell, Flags};
use iced::font::{Style, Weight};
use iced::{Font, Point, Rectangle};
use iced_graphics::text::{self as text_engine, cosmic_text};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(super) struct Glyph {
    pub buffer: Arc<cosmic_text::Buffer>,
    pub position: Point,
    pub cell_index: usize,
    pub clip: Rectangle,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Cache {
    generation: Option<(u64, u64)>,
    pub glyphs: Vec<Glyph>,
}

impl Cache {
    pub fn update(
        &mut self,
        generation: (u64, u64),
        build: impl FnOnce(&mut Vec<Glyph>),
    ) {
        if self.generation == Some(generation) {
            return;
        }
        self.glyphs.clear();
        build(&mut self.glyphs);
        self.generation = Some(generation);
    }
}

// Alacritty's authoritative extras can be arbitrarily long. Bound additional
// paint allocation/shaping work; pathological mark stacks are unsupported.
pub(super) const MAX_COMBINING: usize = 32;

pub(super) fn columns(cell: &Cell, remaining: usize) -> usize {
    if cell
        .flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
    {
        0
    } else if cell.flags.contains(Flags::WIDE_CHAR) {
        remaining.min(2)
    } else {
        remaining.min(1)
    }
}

pub(super) fn visible(cell: &Cell) -> bool {
    !cell.flags.contains(Flags::HIDDEN)
        && (cell.c != ' ' && cell.c != '\t'
            || cell.zerowidth().is_some_and(|marks| !marks.is_empty()))
}

pub(super) fn clipped(cell: &Cell) -> bool {
    !cell.c.is_ascii()
        || cell.flags.contains(Flags::WIDE_CHAR)
        || cell.zerowidth().is_some_and(|marks| !marks.is_empty())
}

pub(super) fn text(cell: &Cell) -> String {
    let marks = cell.zerowidth().unwrap_or_default();
    let mut text = String::with_capacity(
        cell.c.len_utf8() + marks.len().min(MAX_COMBINING) * 4,
    );
    text.push(cell.c);
    text.extend(marks.iter().take(MAX_COMBINING));
    text
}

pub(super) fn font(mut font: Font, flags: Flags) -> Font {
    if flags.intersects(Flags::BOLD | Flags::DIM_BOLD) {
        font.weight = Weight::Bold;
    }
    if flags.contains(Flags::ITALIC) {
        font.style = Style::Italic;
    }
    font
}

pub(super) fn build(
    content: &crate::RenderableContent,
    style: &crate::font::TermFont,
    glyphs: &mut Vec<Glyph>,
) {
    let cell = content.terminal_size;
    // Only this viewport rebuild owns these keys; repeated characters share
    // shaped buffers without a process-wide or growing history cache.
    let mut shapes = std::collections::HashMap::new();
    for (cell_index, indexed) in content.viewport.display_iter().enumerate() {
        let width = columns(
            indexed.cell,
            content.viewport.columns() - indexed.point.column.0,
        );
        if width == 0 || !visible(indexed.cell) || !clipped(indexed.cell) {
            continue;
        }
        let x = indexed.point.column.0 as f32 * cell.cell_width;
        let y = (indexed.point.line.0 as f32
            + content.viewport.display_offset() as f32)
            * cell.cell_height;
        let span = Rectangle::new(
            Point::new(x, y),
            iced::Size::new(width as f32 * cell.cell_width, cell.cell_height),
        );
        let key = (
            font(style.font_type, indexed.flags),
            text(indexed.cell),
            width,
        );
        let (buffer, measured) =
            shapes.entry(key).or_insert_with_key(|(font, text, _)| {
                shape(text, *font, style, span.size())
            });
        glyphs.push(Glyph {
            buffer: Arc::clone(buffer),
            position: Point::new(
                x + (span.width - measured.width) * 0.5,
                y + (cell.cell_height - measured.height) * 0.5,
            ),
            cell_index,
            clip: span,
        });
    }
}

fn shape(
    text: &str,
    font: Font,
    style: &crate::font::TermFont,
    span: iced::Size<f32>,
) -> (Arc<cosmic_text::Buffer>, iced::Size<f32>) {
    use iced_core::text::Shaping;
    let mut fonts = text_engine::font_system()
        .write()
        .expect("Write font system");
    let system = fonts.raw();
    let mut buffer = cosmic_text::Buffer::new(
        system,
        cosmic_text::Metrics::new(style.size, style.size * style.scale_factor),
    );
    // The pinned tiny-skia pipeline skips masks when logical bounds fit,
    // overlooking combining/italic ink overhang. These non-text extents
    // exceed the cell clip so the renderer must apply its actual mask.
    buffer.set_size(system, Some(span.width + 2.0), Some(span.height + 2.0));
    buffer.set_wrap(system, cosmic_text::Wrap::None);
    buffer.set_text(
        system,
        text,
        &text_engine::to_attributes(font),
        text_engine::to_shaping(Shaping::Advanced, text),
        None,
    );
    for line in &mut buffer.lines {
        line.set_align(Some(cosmic_text::Align::Left));
    }
    buffer.shape_until_scroll(system, false);
    let measured = text_engine::measure(&buffer).0;
    drop(fonts);
    (Arc::new(buffer), measured)
}

#[cfg(test)]
mod tests;
