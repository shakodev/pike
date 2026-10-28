use super::*;
use std::{
    ops::{Deref, DerefMut},
    time::Duration,
};
struct Session(Option<Backend>);
impl Session {
    fn new(vt: &str) -> Self {
        #[cfg(windows)]
        let (program, args) =
            ("cmd.exe", vec!["/D", "/Q", "/C", "set /p PIKE_WAIT="]);
        #[cfg(not(windows))]
        let (program, args) = ("/bin/sh", vec!["-c", "read pike_wait"]);
        let (sender, _receiver) = crate::event_bridge::channel();
        let mut b = Backend::new(
            17100,
            sender,
            BackendSettings {
                program: program.into(),
                args: args.into_iter().map(str::to_owned).collect(),
                #[cfg(windows)]
                escape_args: false,
                ..Default::default()
            },
        )
        .unwrap();
        b.handle(Command::Resize(
            Some(Size::new(12.0, 4.0)),
            Some(Size::new(1.0, 1.0)),
        ));
        b.set_visible(true);
        b.fixture_vt(vt.as_bytes());
        Self(Some(b))
    }
    fn accepted_bytes(&self) -> usize {
        let status = self.input_status();
        status.written_bytes + status.outstanding_bytes
    }
    fn first_row(&self) -> String {
        self.term
            .lock()
            .grid()
            .display_iter()
            .take(12)
            .map(|x| x.c)
            .collect()
    }
}
impl Deref for Session {
    type Target = Backend;
    fn deref(&self) -> &Backend {
        self.0.as_ref().unwrap()
    }
}
impl DerefMut for Session {
    fn deref_mut(&mut self) -> &mut Backend {
        self.0.as_mut().unwrap()
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let b = self.0.take().unwrap();
        let handle = b.shutdown_handle();
        drop(b);
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(5), handle.wait())
                    .await
                    .unwrap();
            });
    }
}
#[test]
fn history_pages_bounds_and_extreme_lines_use_the_authoritative_model() {
    let mut b = Session::new("");
    for i in 0..20 {
        b.fixture_vt(format!("{i:02} history\r\n").as_bytes());
    }
    assert_eq!(b.term.lock().grid().display_offset(), 0);
    b.handle(Command::ScrollHistory(HistoryScroll::PageUp));
    assert_eq!(b.term.lock().grid().display_offset(), 4);
    b.handle(Command::ScrollHistory(HistoryScroll::PageDown));
    assert_eq!(b.term.lock().grid().display_offset(), 0);
    b.handle(Command::ScrollHistory(HistoryScroll::Top));
    let max = b.term.lock().history_size();
    assert_eq!(b.term.lock().grid().display_offset(), max);
    b.sync_if_dirty();
    let snapshots = b.snapshot_stats().snapshots;
    for _ in 0..16 {
        b.handle(Command::ScrollHistory(HistoryScroll::Top));
        b.sync_if_dirty();
    }
    assert_eq!(b.snapshot_stats().snapshots, snapshots);
    b.handle(Command::ScrollHistory(HistoryScroll::Lines(i32::MAX)));
    assert_eq!(b.term.lock().grid().display_offset(), max);
    b.handle(Command::ScrollHistory(HistoryScroll::Lines(i32::MIN)));
    assert_eq!(b.term.lock().grid().display_offset(), 0);
    b.handle(Command::SetHistoryLimit(HistoryLimit::try_from(0).unwrap()));
    b.handle(Command::ScrollHistory(HistoryScroll::Top));
    assert_eq!(b.term.lock().grid().display_offset(), 0);
}
#[test]
fn output_retains_scrolled_content_and_accepted_typing_returns_to_bottom() {
    let mut b = Session::new("zero\r\none\r\ntwo\r\nthree\r\nfour\r\nfive\r\n");
    b.handle(Command::ScrollHistory(HistoryScroll::Top));
    let row = b.first_row();
    let offset = b.term.lock().grid().display_offset();
    b.fixture_vt(b"six\r\n");
    assert_eq!(b.first_row(), row);
    assert_eq!(b.term.lock().grid().display_offset(), offset + 1);
    assert_eq!(b.handle(Command::Write(vec![b'x'])), Action::Ignore);
    assert_eq!(b.term.lock().grid().display_offset(), 0);
}
#[test]
fn local_history_is_no_input_in_alternate_screen_and_wheel_arrows_are_bounded()
{
    let mut b = Session::new("\x1b[?1049h\x1b[?1007h");
    let queued = b.accepted_bytes();
    for scroll in [
        HistoryScroll::Lines(20),
        HistoryScroll::PageUp,
        HistoryScroll::PageDown,
        HistoryScroll::Top,
        HistoryScroll::Bottom,
    ] {
        b.handle(Command::ScrollHistory(scroll));
    }
    assert_eq!(b.accepted_bytes(), queued);
    assert_eq!(b.term.lock().grid().display_offset(), 0);
    b.handle(Command::Scroll(i32::MAX));
    assert_eq!(b.accepted_bytes() - queued, 256 * 3);
}
#[test]
fn paired_releases_and_focus_hide_cleanup_are_idempotent_and_session_owned() {
    let mut b = Session::new("\x1b[?1002h\x1b[?1006h");
    let mut other = Session::new("\x1b[?1002h\x1b[?1006h");
    let point = Point::new(Line(2), Column(3));
    b.handle(Command::MouseReport(
        MouseButton::RightButton,
        Modifiers::CTRL,
        point,
        true,
    ));
    let before = b.accepted_bytes();
    assert_eq!(b.handle(Command::FocusChanged(false)), Action::Ignore);
    let after = b.accepted_bytes();
    assert_eq!(after - before, b"\x1b[<18;4;3m".len());
    b.handle(Command::FocusChanged(false));
    b.handle(Command::MouseReport(
        MouseButton::RightButton,
        Modifiers::empty(),
        point,
        false,
    ));
    assert_eq!(b.accepted_bytes(), after);
    assert_eq!(other.accepted_bytes(), 0);
    other.handle(Command::FocusChanged(false));
    assert_eq!(other.accepted_bytes(), 0);
}
#[test]
fn mouse_tracking_enablement_and_invalid_coordinates_cannot_write_or_wrap() {
    let mut b = Session::new("\x1b[?1006h");
    let p = Point::new(Line(0), Column(0));
    b.handle(Command::MouseReport(
        MouseButton::LeftButton,
        Modifiers::empty(),
        p,
        true,
    ));
    assert_eq!(b.accepted_bytes(), 0);
    b.fixture_vt(b"\x1b[?1000h");
    b.handle(Command::MouseReport(
        MouseButton::LeftButton,
        Modifiers::empty(),
        Point::new(Line(-1), Column(0)),
        true,
    ));
    assert_eq!(b.accepted_bytes(), 0);
    b.handle(Command::MouseReport(
        MouseButton::LeftButton,
        Modifiers::empty(),
        Point::new(Line(i32::MAX), Column(usize::MAX)),
        true,
    ));
    assert_eq!(b.accepted_bytes(), b"\x1b[<0;12;4M".len());
}

#[test]
fn legacy_wide_release_and_encoding_transition_cleanup_are_exactly_once() {
    let mut b = Session::new("\x1b[?1000h");
    b.handle(Command::Resize(Some(Size::new(300.0, 230.0)), None));
    let inside = Point::new(Line(2), Column(3));
    let outside = Point::new(Line(229), Column(299));
    let report = |point, pressed| {
        Command::MouseReport(
            MouseButton::LeftButton,
            Modifiers::empty(),
            point,
            pressed,
        )
    };
    // Unsupported presses must not synthesize an orphan release at the edge.
    b.handle(report(outside, true));
    b.handle(report(outside, false));
    assert_eq!(b.accepted_bytes(), 0);
    b.handle(report(inside, true));
    b.handle(report(outside, false));
    assert_eq!(b.accepted_bytes(), 12);
    b.handle(report(outside, false));
    b.handle(Command::FocusChanged(false));
    assert_eq!(b.accepted_bytes(), 12);
    // A valid SGR press may be outside the subsequently selected encoding.
    b.fixture_vt(b"\x1b[?1006h");
    b.handle(report(outside, true));
    let before = b.accepted_bytes();
    b.fixture_vt(b"\x1b[?1006l");
    b.handle(Command::FocusChanged(false));
    assert_eq!(b.accepted_bytes(), before + 6);
    b.handle(report(outside, false));
    b.handle(Command::FocusChanged(false));
    assert_eq!(b.accepted_bytes(), before + 6);
}
