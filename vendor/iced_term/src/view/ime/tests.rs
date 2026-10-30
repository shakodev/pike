use super::*;
use crate::{settings::BackendSettings, PreparedTerminal};
use iced::keyboard::{self, key};
use std::time::Duration;

struct Live(Terminal);
impl Live {
    fn new(id: u64) -> Self {
        #[cfg(windows)]
        let (program, args) = ("cmd.exe", vec!["/D", "/Q", "/K"]);
        #[cfg(not(windows))]
        let (program, args) = ("/bin/sh", vec!["-c", "exec sleep 30"]);
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
        terminal.set_focused(true);
        Self(terminal)
    }
}
impl Drop for Live {
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

fn dispatch(
    term: &Terminal,
    state: &TerminalViewState,
    event: iced::Event,
) -> (Option<Command>, InputMethod) {
    let node = iced::advanced::layout::Node::new(Size::new(640.0, 480.0));
    let mut events = Vec::new();
    let mut shell = iced_core::Shell::new(&mut events);
    let command = TerminalView { term }.update_ime(
        state,
        &event,
        Layout::new(&node),
        &Rectangle::with_size(Size::new(640.0, 480.0)),
        &mut shell,
    );
    (command, shell.input_method().clone())
}
fn redraw() -> iced::Event {
    iced::Event::Window(iced::window::Event::RedrawRequested(
        std::time::Instant::now(),
    ))
}
fn ime(event: input_method::Event) -> iced::Event {
    iced::Event::InputMethod(event)
}
fn accepted(term: &Terminal) -> usize {
    let s = term.input_status();
    s.written_bytes + s.outstanding_bytes + s.cancelled_bytes
}

#[test]
fn ime_public_row_named_space_altgr_text_and_nontext_binding_priority() {
    use iced::advanced::{renderer::Headless, widget::operation};

    let mut a = Live::new(19004);
    let mut b = Live::new(19005);
    b.0.set_window_focus(a.0.backend.window_focus.clone());
    b.0.set_focused(false);
    let renderer = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .expect("offscreen test renderer");
    let size = Size::new(800.0, 240.0);
    fn row<'a>(a: &'a Terminal, b: &'a Terminal) -> iced::Element<'a, Event> {
        iced::widget::row![TerminalView::show(a), TerminalView::show(b)].into()
    }
    let mut tree = Tree::empty();
    {
        let mut view = row(&a.0, &b.0);
        tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &iced_core::layout::Limits::new(size, size),
        );
        view.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &mut operation::focusable::focus::<()>(a.0.widget_id().clone()),
        );
    }
    let mut dispatch = |a: &Terminal, b: &Terminal, event| {
        let mut view = row(a, b);
        tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &iced_core::layout::Limits::new(size, size),
        );
        let mut events = Vec::new();
        let mut shell = iced_core::Shell::new(&mut events);
        view.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            Cursor::Unavailable,
            &renderer,
            &mut iced_core::clipboard::Null,
            &mut shell,
            &Rectangle::with_size(size),
        );
        let captured = shell.is_event_captured();
        (events, captured)
    };
    dispatch(&a.0, &b.0, redraw());
    for (logical_key, modifiers, text, expected, captured) in [
        (
            Key::Named(key::Named::Space),
            Modifiers::CTRL | Modifiers::ALT,
            Some("\u{a0}"),
            Some("\u{a0}"),
            true,
        ),
        (
            Key::Named(key::Named::Space),
            Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT,
            Some("\u{a0}"),
            Some("\u{a0}"),
            true,
        ),
        (
            Key::Character("q".into()),
            Modifiers::CTRL | Modifiers::ALT,
            Some("@"),
            Some("@"),
            true,
        ),
        (
            Key::Named(key::Named::Space),
            Modifiers::empty(),
            Some(" "),
            Some(" "),
            true,
        ),
        (
            Key::Named(key::Named::Space),
            Modifiers::empty(),
            Some("´"),
            Some("´"),
            true,
        ),
        (Key::Unidentified, Modifiers::empty(), None, None, false),
        (Key::Unidentified, Modifiers::empty(), Some(""), None, false),
        (
            Key::Character("e".into()),
            Modifiers::empty(),
            Some("é"),
            Some("é"),
            true,
        ),
        (
            Key::Character("c".into()),
            Modifiers::CTRL,
            Some("c"),
            Some("\x03"),
            true,
        ),
        (
            Key::Character("c".into()),
            Modifiers::CTRL | Modifiers::SHIFT,
            Some("c"),
            None,
            true,
        ),
        (
            Key::Named(key::Named::Space),
            Modifiers::CTRL,
            Some("\u{a0}"),
            None,
            false,
        ),
        (
            Key::Named(key::Named::Space),
            Modifiers::ALT,
            Some("\u{a0}"),
            None,
            false,
        ),
        (
            Key::Named(key::Named::Space),
            Modifiers::CTRL | Modifiers::ALT | Modifiers::LOGO,
            Some("\u{a0}"),
            None,
            false,
        ),
        (
            Key::Named(key::Named::Space),
            Modifiers::CTRL | Modifiers::ALT,
            None,
            None,
            false,
        ),
        (
            Key::Named(key::Named::Space),
            Modifiers::CTRL | Modifiers::ALT,
            Some(""),
            None,
            false,
        ),
        (
            Key::Named(key::Named::Space),
            Modifiers::CTRL | Modifiers::ALT,
            Some("\0"),
            None,
            false,
        ),
        (
            Key::Named(key::Named::ArrowLeft),
            Modifiers::CTRL | Modifiers::ALT,
            Some("x"),
            Some("\x1b[1;7D"),
            true,
        ),
    ] {
        let event = iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: logical_key.clone(),
            modified_key: logical_key,
            physical_key: key::Physical::Code(key::Code::Space),
            location: keyboard::Location::Standard,
            modifiers,
            text: text.map(Into::into),
            repeat: false,
        });
        let (events, actual_captured) = dispatch(&a.0, &b.0, event);
        let writes: Vec<_> = events
            .into_iter()
            .map(|event| {
                let Event::BackendCall(id, Command::Write(bytes)) = event
                else {
                    panic!("unexpected keyboard event: {event:?}");
                };
                assert_eq!(id, a.0.id, "input reached the unfocused sibling");
                bytes
            })
            .collect();
        assert_eq!(
            writes,
            expected
                .into_iter()
                .map(|s| s.as_bytes().to_vec())
                .collect::<Vec<_>>(),
            "modifiers={modifiers:?}, text={text:?}"
        );
        assert_eq!(actual_captured, captured);
    }
    // Explicit nontext bindings still own Space, Ctrl+Space and Alt+Space.
    // Ctrl+Alt uses bindings too when the OS supplies no printable text.
    for modifiers in [
        Modifiers::empty(),
        Modifiers::CTRL,
        Modifiers::ALT,
        Modifiers::CTRL | Modifiers::ALT,
        Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT,
        Modifiers::CTRL | Modifiers::ALT | Modifiers::LOGO,
    ] {
        a.0.handle(crate::Command::AddBindings(vec![(
            crate::bindings::Binding {
                target: InputKind::KeyCode(key::Named::Space),
                modifiers,
                terminal_mode_include: TermMode::empty(),
                terminal_mode_exclude: TermMode::empty(),
            },
            BindingAction::Esc("space-binding".into()),
        )]));
        for text in [None, Some(""), Some("\0")] {
            let (events, captured) = dispatch(
                &a.0,
                &b.0,
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Named(key::Named::Space),
                    modified_key: Key::Named(key::Named::Space),
                    physical_key: key::Physical::Code(key::Code::Space),
                    location: keyboard::Location::Standard,
                    modifiers,
                    text: text.map(Into::into),
                    repeat: false,
                }),
            );
            assert!(captured);
            assert!(
                matches!(events.as_slice(),
                [Event::BackendCall(id, Command::Write(bytes))]
                    if *id == a.0.id && bytes == b"space-binding"),
                "modifiers={modifiers:?}, text={text:?}, events={events:?}"
            );
        }
    }
}

#[test]
fn ime_widget_preedit_zero_writes_capture_all_composing_keys_and_commit_once() {
    let mut live = Live::new(19001);
    let term = &mut live.0;
    let mut state = TerminalViewState::new(term.id);
    state.focus();
    let (_, strategy) = dispatch(term, &state, redraw());
    assert!(strategy.is_enabled());
    let before = accepted(term);
    let work = term.work_stats();
    assert!(dispatch(term, &state, ime(input_method::Event::Opened))
        .0
        .is_none());
    assert!(dispatch(
        term,
        &state,
        ime(input_method::Event::Preedit("ni".into(), Some(0..2)))
    )
    .0
    .is_none());
    let (
        _,
        InputMethod::Enabled {
            preedit: Some(preedit),
            purpose,
            ..
        },
    ) = dispatch(term, &state, redraw())
    else {
        panic!("missing preedit")
    };
    assert_eq!(preedit.content, "ni");
    assert_eq!(purpose, Purpose::Terminal);
    term.set_window_focus(term.backend.window_focus.clone());
    assert!(
        term.is_composing(),
        "idempotent window binding cancelled composition"
    );
    assert_eq!(accepted(term), before);
    assert_eq!(term.work_stats(), work);
    for key in [
        Key::Character("n".into()),
        Key::Named(key::Named::Enter),
        Key::Named(key::Named::ArrowLeft),
        Key::Named(key::Named::Space),
        Key::Character("c".into()),
    ] {
        for modifiers in [
            Modifiers::CTRL | Modifiers::SHIFT,
            Modifiers::CTRL | Modifiers::ALT,
            Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT,
        ] {
            let event = keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key.clone(),
                physical_key: key::Physical::Unidentified(
                    key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                modifiers,
                text: Some("duplicate".into()),
                repeat: false,
            };
            let mut events = Vec::new();
            let mut shell = iced_core::Shell::new(&mut events);
            assert!(TerminalView { term }
                .handle_keyboard_event(
                    &mut state,
                    &mut iced_core::clipboard::Null,
                    &event,
                    &mut shell
                )
                .is_none());
            assert!(shell.is_event_captured());
        }
    }
    dispatch(
        term,
        &state,
        ime(input_method::Event::Preedit(String::new(), None)),
    );
    let (Some(command), _) = dispatch(
        term,
        &state,
        ime(input_method::Event::Commit("你好".into())),
    ) else {
        panic!("missing commit")
    };
    dispatch(term, &state, ime(input_method::Event::Closed));
    assert_eq!(accepted(term), before);
    assert_eq!(
        term.handle(crate::Command::ProxyToBackend(command.clone())),
        crate::actions::Action::Ignore
    );
    term.handle(crate::Command::ProxyToBackend(command));
    assert_eq!(accepted(term), before + "你好".len());
}

#[test]
fn ime_widget_blur_close_modal_and_replacement_reject_stale_commits() {
    let mut a = Live::new(19002);
    let mut b = Live::new(19003);
    b.0.set_window_focus(a.0.backend.window_focus.clone());
    let mut state = TerminalViewState::new(a.0.id);
    state.focus();
    dispatch(&a.0, &state, redraw());
    dispatch(&a.0, &state, ime(input_method::Event::Opened));
    let (Some(commit), _) =
        dispatch(&a.0, &state, ime(input_method::Event::Commit("A".into())))
    else {
        panic!()
    };
    a.0.set_focused(false);
    let (_, strategy) = dispatch(&b.0, &state, redraw());
    assert!(!strategy.is_enabled());
    let (_, strategy) = dispatch(&b.0, &state, redraw());
    assert!(strategy.is_enabled());
    assert!(dispatch(
        &b.0,
        &state,
        ime(input_method::Event::Commit("late A".into()))
    )
    .0
    .is_none());
    assert_eq!(
        a.0.handle(crate::Command::ProxyToBackend(commit)),
        crate::actions::Action::Ignore
    );
    assert_eq!(accepted(&a.0), 0);
    assert_eq!(accepted(&b.0), 0);
    dispatch(&b.0, &state, ime(input_method::Event::Opened));
    b.0.set_input_method_enabled(false);
    assert!(!b.0.is_composing());
    assert!(!dispatch(&b.0, &state, redraw()).1.is_enabled());
    b.0.set_input_method_enabled(true);
    dispatch(&b.0, &state, redraw());
    dispatch(&b.0, &state, redraw());
    dispatch(&b.0, &state, ime(input_method::Event::Opened));
    let (Some(commit), _) = dispatch(
        &b.0,
        &state,
        ime(input_method::Event::Commit("closed".into())),
    ) else {
        panic!()
    };
    b.0.request_close();
    b.0.handle(crate::Command::ProxyToBackend(commit));
    assert_eq!(accepted(&b.0), 0);
}

#[test]
fn ime_anchor_is_logical_current_fractional_and_clipped() {
    use alacritty_terminal::{
        event::VoidListener,
        term::{test::TermSize, Term},
    };
    let mut term =
        Term::new(Default::default(), &TermSize::new(12, 4), VoidListener);
    let mut parser: alacritty_terminal::vte::ansi::Processor =
        Default::default();
    parser.advance(&mut term, b"\x1b[3;5H");
    let mut content = RenderableContent::default();
    content.capture(
        &term,
        crate::geometry::TerminalSize {
            cell_width: 8.5,
            cell_height: 16.25,
            ..Default::default()
        },
    );
    let bounds =
        Rectangle::new(Point::new(10.5, 20.25), Size::new(102.0, 65.0));
    let first = anchor(&content, bounds, bounds).unwrap();
    assert_eq!(
        first,
        Rectangle::new(Point::new(44.5, 52.75), Size::new(8.5, 16.25))
    );
    let moved = Rectangle {
        x: bounds.x + 30.0,
        y: bounds.y + 40.0,
        ..bounds
    };
    assert_eq!(
        anchor(&content, moved, moved).unwrap().position(),
        first.position() + Vector::new(30.0, 40.0)
    );
    let tiny = Rectangle::new(bounds.position(), Size::new(12.0, 10.0));
    let clipped = anchor(&content, bounds, tiny).unwrap();
    assert!(tiny.contains(clipped.position()));
    assert!(clipped.x + clipped.width <= tiny.x + tiny.width);
    assert_eq!(clipped.height, 10.0);
    parser.advance(&mut term, "\x1b[2;2H界\x1b[2;2H".as_bytes());
    let size = crate::geometry::TerminalSize {
        cell_width: 8.5,
        cell_height: 16.25,
        ..Default::default()
    };
    content.capture(&term, size);
    let wide = anchor(&content, bounds, bounds).unwrap();
    assert_eq!(wide.width, 17.0);
    parser.advance(&mut term, b"\x1b[2;3H");
    content.capture(&term, size);
    assert_eq!(anchor(&content, bounds, bounds).unwrap(), wide);
    assert!(
        anchor(&content, bounds, Rectangle::with_size(Size::ZERO)).is_none()
    );
}
