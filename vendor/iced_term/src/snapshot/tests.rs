use super::*;
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::Side;
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{test::TermSize, Config};
type Parser = alacritty_terminal::vte::ansi::Processor;

fn terminal(history: usize) -> Term<VoidListener> {
    Term::new(
        Config {
            scrolling_history: history,
            ..Default::default()
        },
        &TermSize::new(12, 3),
        VoidListener,
    )
}

fn output(term: &mut Term<VoidListener>, lines: usize) {
    let mut parser = Parser::new();
    for n in 0..lines {
        parser.advance(term, format!("line{n:06}\r\n").as_bytes());
    }
}

#[test]
fn history_limits_validate_both_boundaries_and_default() {
    assert_eq!(HistoryLimit::default().lines(), 2000);
    assert_eq!(HistoryLimit::try_from(0).unwrap().lines(), 0);
    assert_eq!(HistoryLimit::try_from(10_000).unwrap().lines(), 10_000);
    assert!(HistoryLimit::try_from(10_001).is_err());
    assert!(HistoryLimit::try_from(usize::MAX).is_err());
}

#[test]
fn snapshot_contains_only_visible_cells_and_reuses_its_allocation() {
    let mut term = terminal(2000);
    output(&mut term, 4000);
    let mut content = RenderableContent::default();
    content.capture(&term, TerminalSize::default());
    assert_eq!(term.history_size(), 2000);
    assert_eq!(content.viewport.cells.len(), 36);
    let allocation = content.viewport.cells.as_ptr();
    for _ in 0..10 {
        output(&mut term, 10);
        content.capture(&term, TerminalSize::default());
        assert_eq!(content.viewport.cells.as_ptr(), allocation);
        assert_eq!(content.viewport.cells.len(), 36);
        assert_eq!(term.history_size(), 2000);
    }
}

#[test]
fn scrolled_output_keeps_absolute_points_selection_and_cursor_metadata() {
    let mut term = terminal(2000);
    output(&mut term, 30);
    term.scroll_display(Scroll::Delta(10));
    let point = Point::new(Line(-10), Column(0));
    let mut selection =
        Selection::new(SelectionType::Simple, point, Side::Left);
    selection.update(Point::new(Line(-8), Column(4)), Side::Right);
    term.selection = Some(selection);
    let mut content = RenderableContent::default();
    for _ in 0..2 {
        content.capture(&term, TerminalSize::default());
        let expected: Vec<_> = term.grid().display_iter().collect();
        let actual: Vec<_> = content.viewport.display_iter().collect();
        assert_eq!(actual, expected);
        assert_eq!(
            content.selectable_range,
            term.selection.as_ref().and_then(|s| s.to_range(&term))
        );
        assert_eq!(content.cursor_point, term.grid().cursor.point);
        assert!(!actual.iter().any(|cell| cell.point == content.cursor_point));
        // This output increases display_offset while keeping the viewed text anchored.
        output(&mut term, 1);
    }
}

#[test]
fn alternate_screen_and_zero_history_have_independent_viewports() {
    let mut term = terminal(0);
    output(&mut term, 30);
    assert_eq!(term.history_size(), 0);
    let normal: Vec<_> =
        term.grid().display_iter().map(|cell| cell.c).collect();
    term.swap_alt();
    output(&mut term, 100);
    let mut content = RenderableContent::default();
    content.capture(&term, TerminalSize::default());
    assert!(content.terminal_mode.contains(TermMode::ALT_SCREEN));
    assert_eq!(term.history_size(), 0);
    assert_eq!(content.viewport.cells.len(), 36);
    term.swap_alt();
    content.capture(&term, TerminalSize::default());
    assert!(!content.terminal_mode.contains(TermMode::ALT_SCREEN));
    assert_eq!(
        content
            .viewport
            .cells
            .iter()
            .map(|cell| cell.c)
            .collect::<Vec<_>>(),
        normal
    );
}

#[test]
fn viewport_keeps_combining_and_wide_cell_data_without_history_copies() {
    let mut term = terminal(2000);
    Parser::new().advance(&mut term, "e\u{301}\u{302}界\r\n".as_bytes());
    let mut content = RenderableContent::default();
    content.capture(&term, TerminalSize::default());
    let cell = &content.viewport.cells[0];
    assert_eq!(cell.zerowidth(), Some(&['\u{301}', '\u{302}'][..]));
    assert!(std::sync::Arc::ptr_eq(
        cell.extra.as_ref().unwrap(),
        term.grid()[Point::default()].extra.as_ref().unwrap()
    ));
    assert_eq!(
        content.viewport.cells[1],
        term.grid()[Point::new(Line(0), Column(1))]
    );
    Parser::new().advance(&mut term, b"\x1b[1;1HX");
    assert_eq!(content.viewport.cells[0].c, 'e');
    content.capture(&term, TerminalSize::default());
    assert_eq!(content.viewport.cells[0].c, 'X');
}
