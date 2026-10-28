use super::*;
use iced::advanced::renderer::Headless;
use iced::advanced::widget::{Tree, operation};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::keyboard::{self, Modifiers};
use iced_term::{BackendCommand, PasteError, PasteRequest, TerminalView};

#[test]
fn paste_errors_remain_visible_on_their_exact_origin_without_sending_input() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    let (ids, ready) = launch(&mut app, &executor);
    drop(app.launched(ids, Ok(ready)));
    let before = app.terminal(ids.session).unwrap().input_status();
    for error in [
        PasteError::ClipboardUnavailable,
        PasteError::TooLarge {
            bytes: iced_term::PASTE_BYTE_LIMIT + 1,
            limit: iced_term::PASTE_BYTE_LIMIT,
        },
        PasteError::Pending,
        PasteError::Input(iced_term::InputError::Full {
            requested: 1024,
            available: 0,
        }),
        PasteError::Input(iced_term::InputError::Closed),
    ] {
        drop(app.update(Message::Terminal((
            ids.session,
            Event::BackendCall(ids.session.get(), BackendCommand::PasteRejected(error)),
        ))));
        assert_eq!(
            app.sessions.entries[&ids.session].notice.as_deref(),
            Some(error.to_string().as_str())
        );
        assert_eq!(app.terminal(ids.session).unwrap().input_status(), before);
    }
    let notice = app.sessions.entries[&ids.session].notice.clone();
    drop(app.update(Message::Terminal((
        ids.session,
        Event::BackendCall(
            ids.session.get() + 1,
            BackendCommand::PasteRejected(PasteError::ClipboardUnavailable),
        ),
    ))));
    assert_eq!(app.sessions.entries[&ids.session].notice, notice);
    shutdown(&mut app, &executor);
}

struct Clipboard;
impl clipboard::Clipboard for Clipboard {
    fn read(&self, _: clipboard::Kind) -> Option<String> {
        Some("synthetic paste".into())
    }
    fn write(&mut self, _: clipboard::Kind, _: String) {
        panic!("unexpected clipboard write")
    }
}

/// Obtain the real public widget's deferred token, without a test-only constructor.
fn request(terminal: &Terminal, renderer: &iced::Renderer) -> PasteRequest {
    let mut view = TerminalView::show(terminal);
    let mut tree = Tree::new(view.as_widget());
    let size = iced::Size::new(640.0, 480.0);
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &layout::Limits::new(size, size));
    view.as_widget_mut().operate(
        &mut tree,
        Layout::new(&node),
        renderer,
        &mut operation::focusable::focus::<()>(terminal.widget_id().clone()),
    );
    let event = iced::Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Character("V".into()),
        modified_key: keyboard::Key::Character("V".into()),
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers: Modifiers::CTRL | Modifiers::SHIFT,
        text: Some("v".into()),
        repeat: false,
    });
    let mut events = Vec::new();
    let mut shell = Shell::new(&mut events);
    view.as_widget_mut().update(
        &mut tree,
        &event,
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        renderer,
        &mut Clipboard,
        &mut shell,
        &iced::Rectangle::with_size(size),
    );
    assert!(shell.is_event_captured());
    events
        .into_iter()
        .find_map(|event| match event {
            Event::BackendCall(_, BackendCommand::Paste(request)) => Some(request),
            _ => None,
        })
        .expect("widget must stage a paste, with its pending slot available")
}

fn admitted(terminal: &Terminal) -> usize {
    let status = terminal.input_status();
    status.written_bytes + status.outstanding_bytes + status.cancelled_bytes
}

#[test]
#[ignore = "requires a local graphics device; run explicitly for host paste integration"]
fn real_widget_paste_tokens_are_canceled_for_stale_hidden_and_ended_origins() {
    let executor = SharedExecutor::new().unwrap();
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .expect("headless renderer");
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    drop(app.update(Message::WindowFocused(true)));
    let (ids, ready) = launch(&mut app, &executor);
    drop(app.launched(ids, Ok(ready)));
    let initial = admitted(app.terminal(ids.session).unwrap());
    for reason in ["stale", "hidden", "ended"] {
        let token = request(app.terminal(ids.session).unwrap(), &renderer);
        let held_clone = token.clone();
        match reason {
            "hidden" => app
                .sessions
                .synchronize(&app.model, None, &app.window_focus),
            "ended" => app.sessions.entries.get_mut(&ids.session).unwrap().ended = true,
            _ => {}
        }
        let raw = ids.session.get() + u64::from(reason == "stale");
        drop(app.update(Message::Terminal((
            ids.session,
            Event::BackendCall(raw, BackendCommand::Paste(token)),
        ))));
        let terminal = match &mut app.sessions.entries.get_mut(&ids.session).unwrap().owner {
            Owner::Attached(terminal) => terminal,
            _ => unreachable!(),
        };
        // Even a later replay of a held clone cannot write the discarded data.
        assert_eq!(
            terminal.handle(iced_term::Command::ProxyToBackend(BackendCommand::Paste(
                held_clone
            ))),
            iced_term::actions::Action::Ignore
        );
        assert_eq!(admitted(terminal), initial);
        terminal.set_visible(true);
        let next = request(terminal, &renderer); // Cancellation freed its bounded slot.
        next.cancel();
        app.sessions.entries.get_mut(&ids.session).unwrap().ended = false;
    }
    let token = request(app.terminal(ids.session).unwrap(), &renderer);
    drop(app.update(Message::Terminal((
        ids.session,
        Event::BackendCall(ids.session.get(), BackendCommand::Paste(token)),
    ))));
    assert_eq!(
        admitted(app.terminal(ids.session).unwrap()),
        initial + "synthetic paste".len()
    );
    // Finish the controlled child's line before awaiting its real completion;
    // this fixture does not exercise Unix cancellation of a partial tty line.
    drop(app.update(Message::Terminal((
        ids.session,
        Event::BackendCall(ids.session.get(), BackendCommand::Write(b"\n".to_vec())),
    ))));
    let handle = app.terminal(ids.session).unwrap().shutdown_handle();
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap();
    });
    shutdown(&mut app, &executor);
}
