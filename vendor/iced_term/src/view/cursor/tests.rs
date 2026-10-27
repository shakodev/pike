use super::*;
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::index::{Column, Line, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{test::TermSize, Term};

fn model(input: &str) -> Term<VoidListener> {
    let mut term =
        Term::new(Default::default(), &TermSize::new(12, 4), VoidListener);
    let mut parser: alacritty_terminal::vte::ansi::Processor =
        Default::default();
    parser.advance(&mut term, input.as_bytes());
    term
}
fn content(term: &Term<VoidListener>) -> RenderableContent {
    let mut content = RenderableContent::default();
    content.capture(
        term,
        crate::geometry::TerminalSize {
            cell_width: 8.0,
            cell_height: 16.0,
            ..Default::default()
        },
    );
    content
}
fn paint(content: &RenderableContent, focused: bool) -> Option<Cursor> {
    Cursor::new(content, &Theme::default(), focused, |cell, point| {
        let theme = Theme::default();
        let (mut fg, mut bg) =
            (theme.get_color(cell.fg), theme.get_color(cell.bg));
        if cell.flags.contains(Flags::INVERSE)
            || content.selectable_range.is_some_and(|s| s.contains(point))
        {
            std::mem::swap(&mut fg, &mut bg);
        }
        (fg, bg)
    })
}

#[test]
fn vt_shapes_hide_show_blink_requests_and_alternate_screen_are_authoritative() {
    assert!(!model("").cursor_style().blinking);
    assert_eq!(
        paint(&content(&model("")), true).unwrap().shape,
        CursorShape::Block
    );
    for (sequence, shape) in [
        ("1", CursorShape::Block),
        ("2", CursorShape::Block),
        ("3", CursorShape::Underline),
        ("4", CursorShape::Underline),
        ("5", CursorShape::Beam),
        ("6", CursorShape::Beam),
    ] {
        let term = model(&format!("X\x1b[1G\x1b[{sequence} q"));
        let content = content(&term);
        assert_eq!(paint(&content, true).unwrap().shape, shape);
        assert_eq!(
            paint(&content, false).unwrap().shape,
            CursorShape::HollowBlock
        );
    }
    assert!(paint(&content(&model("X\x1b[1G\x1b[?25l")), true).is_none());
    let term = model("\x1b[?25l\x1b[?1049h\x1b[?25h\x1b[6 q");
    assert_eq!(
        paint(&content(&term), true).unwrap().shape,
        CursorShape::Beam
    );
    let term = model("\x1b[?1049h\x1b[6 q\x1b[?1049l\x1b[?25l");
    assert!(paint(&content(&term), true).is_none());
}

#[test]
fn inverse_selection_micro_highlight_and_equal_colors_keep_cursor_and_glyph_contrast(
) {
    for input in [
        "X\x1b[1G",
        "X\x1b[7mY\x1b[27mZ\x1b[2D",
        "\x1b[44mXYZ\x1b[2D",
        "\x1b[37;47mXYZ\x1b[2D",
        "\x1b[30;40mXYZ\x1b[2D",
    ] {
        for selected in [false, true] {
            let mut term = model(input);
            if selected {
                let point = term.grid().cursor.point;
                let mut selection =
                    Selection::new(SelectionType::Simple, point, Side::Left);
                selection.update(point, Side::Right);
                term.selection = Some(selection);
            }
            let content = content(&term);
            let cursor = paint(&content, true).unwrap();
            assert!(contrast(cursor.glyph, cursor.color) >= 4.5);
            let inactive = paint(&content, false).unwrap();
            assert!(
                contrast(
                    inactive.glyph_color(cursor.point, inactive.background),
                    inactive.background
                ) >= 4.5
            );
            let unchanged = contrasting(Color::WHITE, inactive.background, 4.5);
            assert_eq!(
                paint(&content, false)
                    .unwrap()
                    .glyph_color(cursor.point, unchanged),
                unchanged
            );
        }
    }
}

#[test]
fn osc12_color_reset_and_wide_continuation_use_real_lead_and_span() {
    let term = model("\x1b]12;#ffaa00\x07A界Be\u{301}\x1b[1;3H");
    let cursor = paint(&content(&term), true).unwrap();
    assert_eq!(cursor.point, GridPoint::new(Line(0), Column(1)));
    assert_eq!(cursor.rect.width, 16.0);
    assert_eq!(cursor.color, Color::from_rgb8(255, 170, 0));
    let term = model("\x1b]12;#ffaa00\x07\x1b]112\x07");
    assert!(content(&term).cursor_color.is_none());
    let term = model("A界Be\u{301}\x1b[1;5H");
    assert_eq!(paint(&content(&term), true).unwrap().rect.width, 8.0);
}

#[test]
fn scrolled_offscreen_cursor_is_not_painted_at_an_invented_position() {
    let mut term = model("1\r\n2\r\n3\r\n4\r\n5\r\n6");
    term.scroll_display(alacritty_terminal::grid::Scroll::Top);
    assert!(paint(&content(&term), true).is_none());
}
