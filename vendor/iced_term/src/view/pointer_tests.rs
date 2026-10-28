use super::*;
use crate::{settings::BackendSettings, PreparedTerminal};
use iced::advanced::renderer::Headless;
use std::time::Duration;

struct Session(Terminal);
impl Session {
    fn new(id: u64, vt: &str) -> Self {
        #[cfg(windows)]
        let (program, args) =
            ("cmd.exe", vec!["/D", "/Q", "/C", "set /p PIKE_WAIT="]);
        #[cfg(not(windows))]
        let (program, args) = ("/bin/sh", vec!["-c", "read pike_wait"]);
        let mut terminal = PreparedTerminal::new(
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
        .attach(Default::default(), Default::default());
        let window = crate::WindowFocus::default();
        window.set_focused(true);
        terminal.set_window_focus(window);
        let c = terminal.renderable_content().unwrap().terminal_size;
        terminal.handle(crate::Command::ProxyToBackend(Command::Resize(
            Some(Size::new(
                c.cell_width * 12.0 + 0.01,
                c.cell_height * 5.0 + 0.01,
            )),
            None,
        )));
        terminal.backend.fixture_vt(vt.as_bytes());
        Self(terminal)
    }
    fn bounds(&self, origin: Point) -> Rectangle {
        let c = self.0.renderable_content().unwrap().terminal_size;
        Rectangle::new(
            origin,
            Size::new(c.cell_width * 12.0 + 0.01, c.cell_height * 5.0 + 0.01),
        )
    }
    fn point(&self, b: Rectangle, col: f32, row: f32) -> Point {
        let c = self.0.renderable_content().unwrap().terminal_size;
        Point::new(b.x + c.cell_width * col, b.y + c.cell_height * row)
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
fn renderer() -> iced::Renderer {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .expect("offscreen test renderer")
}
fn tree(term: &Terminal) -> Tree {
    Tree::new(
        &TerminalView { term } as &dyn Widget<Event, Theme, iced::Renderer>
    )
}
fn dispatch(
    term: &Terminal,
    tree: &mut Tree,
    renderer: &iced::Renderer,
    bounds: Rectangle,
    event: iced::Event,
    cursor: Cursor,
) -> (Vec<Command>, bool) {
    let node =
        iced_core::layout::Node::new(bounds.size()).move_to(bounds.position());
    let mut out = Vec::new();
    let mut shell = iced_core::Shell::new(&mut out);
    TerminalView { term }.update(
        tree,
        &event,
        iced_core::Layout::new(&node),
        cursor,
        renderer,
        &mut iced_core::clipboard::Null,
        &mut shell,
        &bounds,
    );
    let captured = shell.is_event_captured();
    (
        out.into_iter()
            .map(|Event::BackendCall(id, c)| {
                assert_eq!(id, term.id);
                c
            })
            .collect(),
        captured,
    )
}
fn warm(
    term: &Terminal,
    tree: &mut Tree,
    renderer: &iced::Renderer,
    b: Rectangle,
) {
    let _ = dispatch(
        term,
        tree,
        renderer,
        b,
        iced::Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        )),
        Cursor::Unavailable,
    );
}
fn mouse(
    term: &Terminal,
    tree: &mut Tree,
    r: &iced::Renderer,
    b: Rectangle,
    e: mouse::Event,
    p: Point,
) -> Vec<Command> {
    dispatch(
        term,
        tree,
        r,
        b,
        iced::Event::Mouse(e),
        Cursor::Available(p),
    )
    .0
    .into_iter()
    .filter(|c| !matches!(c, Command::FocusChanged(_) | Command::Resize(..)))
    .collect()
}
fn modifiers(
    term: &Terminal,
    tree: &mut Tree,
    r: &iced::Renderer,
    b: Rectangle,
    m: Modifiers,
) {
    let _ = dispatch(
        term,
        tree,
        r,
        b,
        iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(m)),
        Cursor::Unavailable,
    );
}
#[test]
fn all_three_buttons_pair_motion_and_clamped_outside_releases_without_orphans()
{
    let r = renderer();
    for (index, mode) in [(0, "1000"), (1, "1002"), (2, "1003")] {
        let session =
            Session::new(17001 + index, &format!("\x1b[?{mode}h\x1b[?1006h"));
        let term = &session.0;
        let b = session.bounds(Point::new(13.25, 31.5));
        for (button, code, moved) in [
            (
                mouse::Button::Left,
                MouseButton::LeftButton,
                MouseButton::LeftMove,
            ),
            (
                mouse::Button::Middle,
                MouseButton::MiddleButton,
                MouseButton::MiddleMove,
            ),
            (
                mouse::Button::Right,
                MouseButton::RightButton,
                MouseButton::RightMove,
            ),
        ] {
            let mut t = tree(term);
            warm(term, &mut t, &r, b);
            let p = session.point(b, 3.2, 2.2);
            let press = mouse(
                term,
                &mut t,
                &r,
                b,
                mouse::Event::ButtonPressed(button),
                p,
            );
            assert!(
                matches!(press.as_slice(),[Command::MouseReport(c,_,point,true)] if *c==code && point.line.0==2 && point.column.0==3)
            );
            assert!(mouse(
                term,
                &mut t,
                &r,
                b,
                mouse::Event::CursorMoved { position: p },
                p
            )
            .is_empty());
            let p = session.point(b, 4.2, 3.2);
            let movement = mouse(
                term,
                &mut t,
                &r,
                b,
                mouse::Event::CursorMoved { position: p },
                p,
            );
            if mode == "1000" {
                assert!(movement.is_empty());
            } else {
                assert!(
                    matches!(movement.as_slice(),[Command::MouseReport(c,_,point,true)] if *c==moved && point.line.0==3 && point.column.0==4)
                );
            }
            let p = session.point(b, 100.0, 100.0);
            let release = mouse(
                term,
                &mut t,
                &r,
                b,
                mouse::Event::ButtonReleased(button),
                p,
            );
            assert!(
                matches!(release.as_slice(),[Command::MouseReport(c,_,point,false)] if *c==code && point.line.0==4 && point.column.0==11)
            );
            assert!(mouse(
                term,
                &mut t,
                &r,
                b,
                mouse::Event::ButtonReleased(button),
                p
            )
            .is_empty());
            assert!(!TerminalView::has_pointer_capture(t.state.downcast_ref()));
        }
    }
}
#[test]
fn shift_drag_owns_local_selection_through_modifier_change_and_release_outside()
{
    let r = renderer();
    let session = Session::new(17004, "alpha\r\nbeta\x1b[?1002h\x1b[?1006h");
    let term = &session.0;
    let b = session.bounds(Point::new(300.25, 45.5));
    let mut t = tree(term);
    warm(term, &mut t, &r, b);
    modifiers(term, &mut t, &r, b, Modifiers::SHIFT);
    let p = session.point(b, 1.2, 0.5);
    let press = mouse(
        term,
        &mut t,
        &r,
        b,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        p,
    );
    assert!(matches!(
        press.as_slice(),
        [Command::SelectStart(SelectionType::Simple, _)]
    ));
    modifiers(term, &mut t, &r, b, Modifiers::empty());
    let p = session.point(b, 14.0, 7.0);
    let moved = mouse(
        term,
        &mut t,
        &r,
        b,
        mouse::Event::CursorMoved { position: p },
        p,
    );
    assert!(matches!(moved.as_slice(), [Command::SelectUpdate(_)]));
    let release = dispatch(
        term,
        &mut t,
        &r,
        b,
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        Cursor::Unavailable,
    )
    .0;
    assert!(release
        .iter()
        .any(|c| matches!(c, Command::SelectUpdate(_))));
    assert!(!release
        .iter()
        .any(|c| matches!(c, Command::MouseReport(..))));
    assert!(!TerminalView::has_pointer_capture(t.state.downcast_ref()));
    let p = session.point(b, 2.0, 1.0);
    assert!(mouse(
        term,
        &mut t,
        &r,
        b,
        mouse::Event::CursorMoved { position: p },
        p
    )
    .is_empty());
}
#[test]
fn click_only_never_selects_locally_and_any_motion_reports_without_button() {
    let r = renderer();
    let session = Session::new(17005, "\x1b[?1003h\x1b[?1006h");
    let term = &session.0;
    let b = session.bounds(Point::new(8.0, 32.0));
    let mut t = tree(term);
    warm(term, &mut t, &r, b);
    let p = session.point(b, 3.2, 1.2);
    assert!(
        matches!(mouse(term,&mut t,&r,b,mouse::Event::CursorMoved{position:p},p).as_slice(),[Command::MouseReport(MouseButton::NoneMove,_,point,true)] if point.line.0==1 && point.column.0==3)
    );
    assert!(mouse(
        term,
        &mut t,
        &r,
        b,
        mouse::Event::CursorMoved { position: p },
        p
    )
    .is_empty());
}
#[test]
fn blur_window_leave_and_move_cleanup_are_single_release_and_no_stale_drag() {
    let r = renderer();
    let session = Session::new(17006, "\x1b[?1002h\x1b[?1006h");
    let term = &session.0;
    let b = session.bounds(Point::new(8.5, 32.25));
    for kind in 0..3 {
        let mut t = tree(term);
        warm(term, &mut t, &r, b);
        let p = session.point(b, 2.5, 1.5);
        mouse(
            term,
            &mut t,
            &r,
            b,
            mouse::Event::ButtonPressed(mouse::Button::Left),
            p,
        );
        let event = if kind == 0 {
            iced::Event::Window(iced::window::Event::Unfocused)
        } else if kind == 1 {
            iced::Event::Mouse(mouse::Event::CursorLeft)
        } else {
            iced::Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            ))
        };
        let bounds = if kind == 2 {
            Rectangle::new(Point::new(250.0, 90.0), b.size())
        } else {
            b
        };
        let events =
            dispatch(term, &mut t, &r, bounds, event, Cursor::Unavailable).0;
        assert_eq!(
            events
                .iter()
                .filter(|c| matches!(
                    c,
                    Command::MouseReport(MouseButton::LeftButton, _, _, false)
                ))
                .count(),
            1
        );
        assert!(!TerminalView::has_pointer_capture(t.state.downcast_ref()));
    }
}
#[test]
fn fractional_wheel_is_captured_and_hover_does_not_change_keyboard_focus() {
    let r = renderer();
    let a = Session::new(17007, "");
    let b = Session::new(17008, "");
    let ba = a.bounds(Point::new(0.0, 20.0));
    let bb = b.bounds(Point::new(300.0, 20.0));
    let mut ta = tree(&a.0);
    let mut tb = tree(&b.0);
    warm(&a.0, &mut ta, &r, ba);
    warm(&b.0, &mut tb, &r, bb);
    ta.state.downcast_mut::<TerminalViewState>().focus();
    let p = b.point(bb, 2.3, 1.3);
    for n in 0..4 {
        let event = iced::Event::Mouse(mouse::Event::WheelScrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: 0.25 },
        });
        let (out, captured) =
            dispatch(&b.0, &mut tb, &r, bb, event, Cursor::Available(p));
        assert!(captured);
        let scrolls: Vec<_> = out
            .iter()
            .filter(|c| matches!(c, Command::Scroll(_)))
            .collect();
        assert_eq!(scrolls.len(), usize::from(n == 3));
    }
    assert!(ta.state.downcast_ref::<TerminalViewState>().is_focused());
    assert!(!tb.state.downcast_ref::<TerminalViewState>().is_focused());
}
#[test]
fn wheel_reports_use_viewport_coordinates_and_shift_bypasses_alt_and_mouse() {
    let r = renderer();
    let mut session=Session::new(17009,"line\r\nline\r\nline\r\nline\r\nline\r\nline\r\n\x1b[?1000h\x1b[?1006h");
    session
        .0
        .handle(crate::Command::ProxyToBackend(Command::ScrollHistory(
            crate::HistoryScroll::Top,
        )));
    let term = &session.0;
    let b = session.bounds(Point::new(10.25, 19.5));
    let mut t = tree(term);
    warm(term, &mut t, &r, b);
    let p = session.point(b, 3.3, 2.2);
    let out = mouse(
        term,
        &mut t,
        &r,
        b,
        mouse::Event::WheelScrolled {
            delta: ScrollDelta::Lines { x: -1.0, y: 1.0 },
        },
        p,
    );
    assert!(
        matches!(out.as_slice(),[Command::MouseReport(MouseButton::ScrollUp,_,point,true),Command::MouseReport(MouseButton::ScrollRight,_,_,true)] if point.line.0==2 && point.column.0==3)
    );
    modifiers(term, &mut t, &r, b, Modifiers::SHIFT);
    let out = mouse(
        term,
        &mut t,
        &r,
        b,
        mouse::Event::WheelScrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: 1.0 },
        },
        p,
    );
    assert!(matches!(
        out.as_slice(),
        [Command::ScrollHistory(crate::HistoryScroll::Lines(1))]
    ));
}

#[test]
fn history_shortcut_is_captured_locally_and_ignored_by_unfocused_pane() {
    let r = renderer();
    let session = Session::new(17010, "alpha\r\nbeta");
    let b = session.bounds(Point::new(13.25, 31.5));
    let term = &session.0;
    let mut t = tree(term);
    warm(term, &mut t, &r, b);
    let event = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Named(iced::keyboard::key::Named::PageUp),
        modified_key: Key::Named(iced::keyboard::key::Named::PageUp),
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers: Modifiers::SHIFT,
        text: Some("unexpected text".into()),
        repeat: true,
    });
    let (out, captured) =
        dispatch(term, &mut t, &r, b, event.clone(), Cursor::Unavailable);
    assert!(!captured);
    assert!(!out
        .iter()
        .any(|c| matches!(c, Command::Write(_) | Command::ScrollHistory(_))));
    t.state.downcast_mut::<TerminalViewState>().focus();
    let (out, captured) =
        dispatch(term, &mut t, &r, b, event, Cursor::Unavailable);
    assert!(captured);
    assert!(out.iter().any(|c| matches!(
        c,
        Command::ScrollHistory(crate::HistoryScroll::PageUp)
    )));
    assert!(!out.iter().any(|c| matches!(c, Command::Write(_))));
}
