use super::*;
use crate::{PreparedTerminal, Terminal};
use std::time::Duration;

struct Session(Terminal);
impl Session {
    fn new(id: u64) -> Self {
        #[cfg(windows)]
        let (program, args) =
            ("cmd.exe", vec!["/D", "/Q", "/C", "set /p PIKE_WAIT="]);
        #[cfg(not(windows))]
        let (program, args) = ("/bin/sh", vec!["-c", "read pike_wait"]);
        Self(
            PreparedTerminal::new(
                id,
                BackendSettings {
                    program: program.into(),
                    args: args.into_iter().map(str::to_owned).collect(),
                    #[cfg(windows)]
                    escape_args: false,
                    ..Default::default()
                },
            )
            .unwrap()
            .attach(Default::default(), Default::default()),
        )
    }
    fn model(&self, columns: usize, lines: usize, input: &str) {
        let mut term = self.0.backend.term.lock();
        term.resize(TermSize::new(columns, lines));
        let mut parser: alacritty_terminal::vte::ansi::Processor =
            Default::default();
        parser.advance(&mut *term, b"\x1bc");
        parser.advance(&mut *term, input.as_bytes());
    }
    fn select(
        &self,
        ty: SelectionType,
        start: (i32, usize, Side),
        end: (i32, usize, Side),
    ) {
        let mut selection = Selection::new(
            ty,
            Point::new(Line(start.0), Column(start.1)),
            start.2,
        );
        selection.update(Point::new(Line(end.0), Column(end.1)), end.2);
        self.0.backend.term.lock().selection = Some(selection);
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.0.request_close();
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                tokio::time::timeout(
                    Duration::from_secs(5),
                    self.0.shutdown_handle().wait(),
                )
                .await
                .unwrap();
            });
    }
}

#[test]
fn exact_selection_strings_follow_authoritative_line_wrap_padding_and_unicode_rules(
) {
    use alacritty_terminal::index::Direction::{Left, Right};
    use SelectionType::*;
    let session = Session::new(151001);
    for (label, columns, input, ty, start, end, expected) in [
        (
            "hard lines",
            12,
            "alpha\r\nbeta",
            Simple,
            (0, 0, Left),
            (1, 3, Right),
            "alpha\nbeta",
        ),
        (
            "soft wrap",
            8,
            "abcdefghij",
            Simple,
            (0, 0, Left),
            (1, 1, Right),
            "abcdefghij",
        ),
        (
            "padding",
            12,
            "alpha",
            Simple,
            (0, 0, Left),
            (0, 11, Right),
            "alpha",
        ),
        (
            "blank hard line",
            12,
            "alpha\r\n\r\nbeta",
            Simple,
            (0, 0, Left),
            (2, 3, Right),
            "alpha\n\nbeta",
        ),
        (
            "selected spaces",
            12,
            "a  b",
            Simple,
            (0, 1, Left),
            (0, 2, Right),
            "  ",
        ),
        (
            "wide and accent",
            12,
            "A界Be\u{301}Z",
            Simple,
            (0, 0, Left),
            (0, 5, Right),
            "A界Be\u{301}Z",
        ),
        (
            "wide spacer",
            12,
            "A界B",
            Simple,
            (0, 2, Left),
            (0, 2, Right),
            "界",
        ),
        (
            "wide wrap",
            8,
            "1234567界Z",
            Simple,
            (0, 0, Left),
            (1, 2, Right),
            "1234567界Z",
        ),
        (
            "word",
            12,
            "alpha beta!",
            Semantic,
            (0, 7, Left),
            (0, 7, Right),
            "beta!",
        ),
        (
            "line",
            12,
            "alpha\r\nbeta",
            Lines,
            (0, 3, Left),
            (1, 1, Right),
            "alpha\nbeta\n",
        ),
        (
            "block",
            12,
            "abcd\r\nefgh",
            Block,
            (0, 1, Left),
            (1, 2, Right),
            "bc\nfg",
        ),
        (
            "reverse",
            12,
            "alpha\r\nbeta",
            Simple,
            (1, 3, Right),
            (0, 0, Left),
            "alpha\nbeta",
        ),
        (
            "tab",
            12,
            "A\tB",
            Simple,
            (0, 0, Left),
            (0, 8, Right),
            "A\tB",
        ),
    ] {
        session.model(columns, 4, input);
        session.select(ty, start, end);
        let before = session.0.backend.term.lock().selection.clone();
        let mut writes = 0;
        let result = session.0.copy_selection(|text| {
            writes += 1;
            text
        });
        assert_eq!(result.as_deref(), Some(expected), "{label}");
        assert_eq!(writes, 1, "{label}");
        assert_eq!(session.0.backend.term.lock().selection, before, "{label}");
    }
}

#[test]
fn no_selection_empty_anchor_and_padding_do_not_call_writer() {
    let session = Session::new(151002);
    session.model(12, 4, " ");
    assert_eq!(
        session
            .0
            .copy_selection(|_| panic!("no selection overwrote clipboard")),
        None::<()>
    );
    session.select(
        SelectionType::Simple,
        (0, 0, Side::Left),
        (0, 0, Side::Left),
    );
    assert_eq!(
        session
            .0
            .copy_selection(|_| panic!("empty anchor overwrote clipboard")),
        None::<()>
    );
    session.select(
        SelectionType::Simple,
        (0, 0, Side::Left),
        (0, 11, Side::Right),
    );
    assert_eq!(session.0.backend.selectable_content(), Some(String::new()));
    assert_eq!(
        session.0.copy_selection(|_| panic!(
            "empty extracted text overwrote clipboard"
        )),
        None::<()>
    );
}

#[test]
fn unavailable_writer_preserves_both_sessions_and_runs_outside_model_lock() {
    let first = Session::new(151003);
    let other = Session::new(151004);
    for session in [&first, &other] {
        session.model(12, 4, "selected");
        session.select(
            SelectionType::Simple,
            (0, 0, Side::Left),
            (0, 7, Side::Right),
        );
    }
    let selections = (
        first.0.backend.term.lock().selection.clone(),
        other.0.backend.term.lock().selection.clone(),
    );
    let work = first.0.work_stats();
    let snapshots = first.0.snapshot_stats().snapshots;
    let mut writes = 0;
    let result = first.0.copy_selection(|_| {
        assert!(
            first.0.backend.term.try_lock_unfair().is_some(),
            "clipboard callback held the terminal lock"
        );
        writes += 1;
        Err::<(), _>(std::io::ErrorKind::PermissionDenied)
    });
    assert_eq!(result, Some(Err(std::io::ErrorKind::PermissionDenied)));
    assert_eq!(writes, 1);
    assert_eq!(first.0.backend.term.lock().selection, selections.0);
    assert_eq!(other.0.backend.term.lock().selection, selections.1);
    assert_eq!(first.0.work_stats(), work);
    assert_eq!(first.0.snapshot_stats().snapshots, snapshots);
    assert_eq!(first.0.input_status().outstanding_bytes, 0);
}

#[test]
fn hidden_history_copy_is_not_limited_to_the_visible_snapshot() {
    let mut session = Session::new(151005);
    session.model(12, 3, "zero\r\none\r\ntwo\r\nthree\r\nfour");
    session.select(
        SelectionType::Lines,
        (-2, 0, Side::Left),
        (1, 0, Side::Right),
    );
    session.0.set_visible(false);
    let before = session.0.snapshot_stats();
    assert!(session.0.renderable_content().is_none());
    assert_eq!(
        session.0.copy_selection(|text| text).as_deref(),
        Some("zero\none\ntwo\nthree\n")
    );
    assert_eq!(session.0.snapshot_stats().snapshots, before.snapshots);
    assert_eq!(session.0.snapshot_stats().viewport_cells, 0);
}

#[test]
fn no_op_and_height_resize_preserve_selection_but_column_reflow_invalidates_it()
{
    let session = Session::new(151006);
    session.model(12, 4, "alpha\r\nbeta");
    session.select(
        SelectionType::Simple,
        (0, 0, Side::Left),
        (1, 3, Side::Right),
    );
    for lines in [4, 5, 4] {
        session
            .0
            .backend
            .term
            .lock()
            .resize(TermSize::new(12, lines));
        assert_eq!(
            session.0.copy_selection(|text| text).as_deref(),
            Some("alpha\nbeta")
        );
    }
    session.0.backend.term.lock().resize(TermSize::new(8, 4));
    assert!(session.0.backend.term.lock().selection.is_none());
    assert_eq!(
        session.0.copy_selection(|_| panic!(
            "invalidated selection overwrote clipboard"
        )),
        None::<()>
    );
}
