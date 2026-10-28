use super::*;
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{test::TermSize, Config, Term};

fn model(columns: usize, input: &str) -> Term<VoidListener> {
    let mut term =
        Term::new(Config::default(), &TermSize::new(columns, 4), VoidListener);
    let mut parser: alacritty_terminal::vte::ansi::Processor =
        Default::default();
    parser.advance(&mut term, input.as_bytes());
    term
}

fn runs(term: &Term<VoidListener>) -> Vec<(i32, usize, usize, String)> {
    term.grid()
        .display_iter()
        .filter_map(|indexed| {
            let width =
                columns(indexed.cell, term.columns() - indexed.point.column.0);
            (width != 0 && visible(indexed.cell)).then(|| {
                (
                    indexed.point.line.0,
                    indexed.point.column.0,
                    width,
                    text(indexed.cell),
                )
            })
        })
        .collect()
}

fn selection(term: &mut Term<VoidListener>, end: Point) -> String {
    let mut selection =
        Selection::new(SelectionType::Simple, Point::default(), Side::Left);
    selection.update(end, Side::Right);
    term.selection = Some(selection);
    term.selection_to_string().unwrap()
}

#[test]
fn authoritative_cjk_combining_and_ascii_positions_match_cursor_and_copy() {
    let mut term = model(12, "A界Be\u{301}");
    assert_eq!(
        runs(&term),
        vec![
            (0, 0, 1, "A".into()),
            (0, 1, 2, "界".into()),
            (0, 3, 1, "B".into()),
            (0, 4, 1, "e\u{301}".into())
        ]
    );
    assert_eq!(term.grid().cursor.point, Point::new(Line(0), Column(5)));
    assert_eq!(
        selection(&mut term, Point::new(Line(0), Column(4))),
        "A界Be\u{301}"
    );
}

#[test]
fn leading_wide_wrap_placeholder_is_not_a_glyph_and_resize_preserves_copy() {
    let mut term = model(8, "1234567界Z");
    assert!(term.grid()[Point::new(Line(0), Column(7))]
        .flags
        .contains(Flags::LEADING_WIDE_CHAR_SPACER));
    assert_eq!(
        &runs(&term)[7..],
        &[(1, 0, 2, "界".into()), (1, 2, 1, "Z".into())]
    );
    assert_eq!(term.grid().cursor.point, Point::new(Line(1), Column(3)));
    assert_eq!(
        selection(&mut term, Point::new(Line(1), Column(2))),
        "1234567界Z"
    );
    term.resize(TermSize::new(6, 4));
    assert_eq!(
        &runs(&term)[6..],
        &[
            (1, 0, 1, "7".into()),
            (1, 1, 2, "界".into()),
            (1, 3, 1, "Z".into())
        ]
    );
    assert_eq!(term.grid().cursor.point, Point::new(Line(1), Column(4)));
    assert_eq!(
        selection(&mut term, Point::new(Line(1), Column(3))),
        "1234567界Z"
    );
}

#[test]
fn wide_cell_at_exact_row_end_does_not_shift_next_ascii_or_spacer_selection() {
    let mut term = model(8, "123456界Z");
    assert_eq!(
        &runs(&term)[6..],
        &[(0, 6, 2, "界".into()), (1, 0, 1, "Z".into())]
    );
    assert_eq!(term.grid().cursor.point, Point::new(Line(1), Column(1)));
    let mut selected = Selection::new(
        SelectionType::Simple,
        Point::new(Line(0), Column(7)),
        Side::Left,
    );
    selected.update(Point::new(Line(0), Column(7)), Side::Right);
    term.selection = Some(selected);
    assert_eq!(term.selection_to_string().unwrap(), "界");
}

#[test]
fn pathological_mark_stack_has_bounded_paint_text_without_mutating_model() {
    let term = model(12, &format!("e{}", "\u{301}".repeat(200)));
    let cell = &term.grid()[Point::default()];
    assert_eq!(cell.zerowidth().unwrap().len(), 200);
    let painted = text(cell);
    assert_eq!(painted.chars().count(), MAX_COMBINING + 1);
    assert!(painted.capacity() <= 1 + MAX_COMBINING * 4);
    assert!(clipped(cell));
    assert_eq!(cell.zerowidth().unwrap().len(), 200);
}

#[test]
fn continuation_and_hidden_cells_do_not_emit_duplicate_or_concealed_text() {
    let mut cell = Cell {
        c: '界',
        flags: Flags::WIDE_CHAR_SPACER,
        ..Default::default()
    };
    assert_eq!(columns(&cell, 3), 0);
    cell.flags = Flags::LEADING_WIDE_CHAR_SPACER;
    assert_eq!(columns(&cell, 3), 0);
    cell.flags = Flags::WIDE_CHAR;
    assert_eq!(columns(&cell, 1), 1);
    cell.flags = Flags::HIDDEN;
    assert!(!visible(&cell));
}

#[test]
fn unchanged_generation_never_rebuilds_or_reshapes_unicode_spans() {
    let mut cache = Cache::default();
    let mut builds = 0;
    cache.update((4, 8), |_| builds += 1);
    for _ in 0..32 {
        cache.update((4, 8), |_| panic!("unrelated redraw reshaped glyphs"));
    }
    cache.update((5, 8), |_| builds += 1); // New viewport snapshot.
    cache.update((5, 9), |_| builds += 1); // Actual font measurement change.
    assert_eq!(builds, 3);
    assert!(Cache::default().generation.is_none()); // Session-ID diff resets it.
}

#[test]
fn repeated_unicode_shares_one_bounded_shaped_buffer_and_keeps_clip_guards() {
    let term = model(12, "界界界");
    let mut content = crate::RenderableContent::default();
    content.capture(&term, crate::geometry::TerminalSize::default());
    let style = crate::font::TermFont::new(Default::default());
    let mut glyphs = Vec::new();
    build(&content, &style, &mut glyphs);
    assert_eq!(glyphs.len(), 3);
    assert!(Arc::ptr_eq(&glyphs[0].buffer, &glyphs[1].buffer));
    assert!(Arc::ptr_eq(&glyphs[1].buffer, &glyphs[2].buffer));
    let (width, height) = glyphs[0].buffer.size();
    assert!(width.unwrap() > glyphs[0].clip.width);
    assert!(height.unwrap() > glyphs[0].clip.height);
}
