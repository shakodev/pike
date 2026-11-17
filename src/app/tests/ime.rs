//! Native ownership gates with tokens obtained from the real public widget.
use super::*;
use iced::advanced::renderer::Headless;
use iced::advanced::widget::{Tree, operation};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced_term::{BackendCommand as B, ImeCommit, ImeError, TerminalView};

#[test]
fn ime_errors_are_visible_only_on_their_origin_without_writing_input() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    let (ids, ready) = launch(&mut app, &executor);
    drop(app.launched(ids, Ok(ready)));
    let before = app.terminal(ids.session).unwrap().input_status();
    for error in [
        ImeError::PreeditTooLarge {
            bytes: iced_term::PREEDIT_BYTE_LIMIT + 1,
        },
        ImeError::CommitQueueFull {
            bytes: 7,
            available: 0,
        },
        ImeError::WrongSession,
        ImeError::Input(iced_term::InputError::Full {
            requested: 7,
            available: 0,
        }),
        ImeError::Input(iced_term::InputError::Closed),
    ] {
        drop(app.update(Message::Terminal((
            ids.session,
            Event::BackendCall(ids.session.get(), B::ImeRejected(error)),
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
            B::ImeRejected(ImeError::WrongSession),
        ),
    ))));
    assert_eq!(app.sessions.entries[&ids.session].notice, notice);
    shutdown(&mut app, &executor);
}

pub(super) fn request(terminal: &Terminal, renderer: &iced::Renderer) -> ImeCommit {
    use iced::advanced::input_method::Event as Ime;
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
    let mut events = Vec::new();
    for event in [
        iced::Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        )),
        iced::Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        )),
        iced::Event::InputMethod(Ime::Opened),
        iced::Event::InputMethod(Ime::Preedit("ni".into(), Some(0..2))),
        iced::Event::InputMethod(Ime::Preedit(String::new(), None)),
        iced::Event::InputMethod(Ime::Commit("你好".into())),
        iced::Event::InputMethod(Ime::Closed),
    ] {
        let mut shell = Shell::new(&mut events);
        view.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut clipboard::Null,
            &mut shell,
            &iced::Rectangle::with_size(size),
        );
    }
    events
        .into_iter()
        .find_map(|event| match event {
            Event::BackendCall(_, B::ImeCommit(commit)) => Some(commit),
            _ => None,
        })
        .expect("public widget must stage a commit")
}

fn admitted(terminal: &Terminal) -> usize {
    let status = terminal.input_status();
    status.written_bytes + status.outstanding_bytes + status.cancelled_bytes
}

#[test]
#[ignore = "requires a local graphics device; run explicitly for host IME integration"]
fn ime_rejected_origin_cancels_held_clones_and_valid_commit_is_once_only() {
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
    for reason in [
        "stale",
        "hidden",
        "ended",
        "unfocused",
        "blurred",
        "unowned",
        "reservation",
    ] {
        let token = request(app.terminal(ids.session).unwrap(), &renderer);
        let clone = token.clone();
        assert_eq!(
            admitted(app.terminal(ids.session).unwrap()),
            initial,
            "preedit/commit staging wrote input"
        );
        match reason {
            "hidden" => app
                .sessions
                .synchronize(&app.model, None, &app.window_focus),
            "ended" => app.sessions.entries.get_mut(&ids.session).unwrap().ended = true,
            "unfocused" => drop(app.update(Message::Terminal((
                ids.session,
                Event::BackendCall(ids.session.get(), B::FocusChanged(false)),
            )))),
            "blurred" => app.window_focus.set_focused(false),
            _ => {}
        }
        let mut foreign = Model::default();
        let (_, Effect::Spawn(other)) = foreign.create_tab().unwrap() else {
            unreachable!()
        };
        if reason == "reservation" {
            app.sessions
                .entries
                .get_mut(&ids.session)
                .unwrap()
                .reservation = other;
        }
        let removed =
            (reason == "unowned").then(|| app.sessions.entries.remove(&ids.session).unwrap());
        drop(app.update(Message::Terminal((
            ids.session,
            Event::BackendCall(
                ids.session.get() + u64::from(reason == "stale"),
                B::ImeCommit(token),
            ),
        ))));
        if let Some(entry) = removed {
            app.sessions.entries.insert(ids.session, entry);
        }
        let entry = app.sessions.entries.get_mut(&ids.session).unwrap();
        entry.reservation = ids;
        entry.ended = false;
        let Owner::Attached(terminal) = &mut entry.owner else {
            unreachable!()
        };
        assert_eq!(
            terminal.handle(iced_term::Command::ProxyToBackend(B::ImeCommit(clone))),
            iced_term::actions::Action::Ignore,
            "rejected {reason} clone remained live"
        );
        assert_eq!(admitted(terminal), initial, "rejected {reason} wrote input");
        app.window_focus.set_focused(true);
        terminal.set_visible(true);
        terminal.set_focused(true);
    }
    let token = request(app.terminal(ids.session).unwrap(), &renderer);
    for commit in [token.clone(), token] {
        drop(app.update(Message::Terminal((
            ids.session,
            Event::BackendCall(ids.session.get(), B::ImeCommit(commit)),
        ))));
    }
    assert_eq!(
        admitted(app.terminal(ids.session).unwrap()),
        initial + "你好".len()
    );
    // Finish the controlled tty line before awaiting native cleanup.
    drop(app.update(Message::Terminal((
        ids.session,
        Event::BackendCall(ids.session.get(), B::Write(b"\n".to_vec())),
    ))));
    let handle = app.terminal(ids.session).unwrap().shutdown_handle();
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap();
    });
    shutdown(&mut app, &executor);
}

#[test]
#[ignore = "requires a local graphics device; run explicitly for split IME ownership"]
fn ime_visible_sibling_requires_model_authority_and_cancels_every_held_clone() {
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
    let (first, ready) = launch(&mut app, &executor);
    drop(app.launched(first, Ok(ready)));
    let target = app.active_target().unwrap();
    drop(app.update(Message::GridSize(target.tab, iced::Size::new(960.0, 560.0))));
    let prior = request(app.terminal(first.session).unwrap(), &renderer);
    let launch = executor
        .enter(|| app.begin_split(target, super::super::split::Direction::Right))
        .unwrap();
    let next = Reservation {
        session: launch.session,
        request: launch.request,
    };
    let ready = executor.block_on(launch.receive()).unwrap();
    drop(app.launched(next, Ok(ready)));
    assert_eq!(app.active_session(), Some(next.session));
    for id in [first.session, next.session] {
        assert!(app.terminal(id).unwrap().is_visible());
        assert_eq!(admitted(app.terminal(id).unwrap()), 0);
    }
    let Owner::Attached(terminal) =
        &mut app.sessions.entries.get_mut(&first.session).unwrap().owner
    else {
        unreachable!()
    };
    // The real split focus transition invalidates the preceding composition.
    terminal.handle(iced_term::Command::ProxyToBackend(B::ImeCommit(prior)));
    assert_eq!(admitted(terminal), 0);
    // Model authority must also reject a newly staged real-widget token when
    // stale backend/widget state claims the inactive visible sibling is focused.
    terminal.set_focused(true);
    let token = request(terminal, &renderer);
    let held = token.clone();
    app.sessions.route(
        &mut app.model,
        &app.window_focus,
        first.session,
        Event::BackendCall(first.session.get(), B::ImeCommit(token)),
    );
    let Owner::Attached(terminal) =
        &mut app.sessions.entries.get_mut(&first.session).unwrap().owner
    else {
        unreachable!()
    };
    assert!(terminal.renderable_content().unwrap().focused);
    terminal.handle(iced_term::Command::ProxyToBackend(B::ImeCommit(held)));
    assert_eq!(
        admitted(terminal),
        0,
        "inactive visible sibling admitted IME bytes"
    );
    assert_eq!(app.active_session(), Some(next.session));
    app.synchronize();
    let token = request(app.terminal(next.session).unwrap(), &renderer);
    for commit in [token.clone(), token] {
        drop(app.update(Message::Terminal((
            next.session,
            Event::BackendCall(next.session.get(), B::ImeCommit(commit)),
        ))));
    }
    assert_eq!(admitted(app.terminal(first.session).unwrap()), 0);
    assert_eq!(admitted(app.terminal(next.session).unwrap()), "你好".len());
    // Finish both controlled lines through the origin-bound protocol path and
    // await real completion before aggregate shutdown can cancel queued input.
    for id in [first.session, next.session] {
        drop(app.update(Message::Terminal((
            id,
            Event::BackendCall(
                id.get(),
                B::ProcessAlacrittyEvent(iced_term::AlacrittyEvent::PtyWrite("\n".into())),
            ),
        ))));
        let completion = app.terminal(id).unwrap().shutdown_handle();
        executor.block_on(async {
            tokio::time::timeout(DEADLINE, completion.wait())
                .await
                .unwrap();
        });
    }
    shutdown(&mut app, &executor);
}
