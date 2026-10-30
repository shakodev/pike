//! Public host commands against real prepared sessions; no vendor test hooks.
use super::*;
use iced_term::BackendCommand as B;

fn send(app: &mut App, id: SessionId, command: B) {
    drop(app.update(Message::Terminal((
        id,
        Event::BackendCall(id.get(), command),
    ))));
}

fn admitted(app: &App, id: SessionId) -> usize {
    let status = app.terminal(id).unwrap().input_status();
    status.written_bytes + status.outstanding_bytes + status.cancelled_bytes
}

#[test]
fn queued_keyboard_requires_current_pane_and_window_focus() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    drop(app.update(Message::WindowFocused(true)));
    let (ids, ready) = launch(&mut app, &executor);
    drop(app.launched(ids, Ok(ready)));
    let initial = admitted(&app, ids.session);
    send(&mut app, ids.session, B::FocusChanged(false));
    send(&mut app, ids.session, B::Write(b"unfocused".to_vec()));
    let pane_rejected = admitted(&app, ids.session) == initial;
    send(&mut app, ids.session, B::FocusChanged(true));
    // Window authority can change before a queued widget focus receipt arrives.
    app.window_focus.set_focused(false);
    send(&mut app, ids.session, B::Write(b"blurred".to_vec()));
    let window_rejected = admitted(&app, ids.session) == initial;
    drop(app.update(Message::WindowFocused(true)));
    let before = admitted(&app, ids.session);
    send(&mut app, ids.session, B::Write(b"accepted\n".to_vec()));
    let accepted = admitted(&app, ids.session) - before;
    let handle = app.terminal(ids.session).unwrap().shutdown_handle();
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap();
    });
    shutdown(&mut app, &executor);
    assert!(
        pane_rejected,
        "queued keyboard wrote after pane focus changed"
    );
    assert!(window_rejected, "queued keyboard wrote after window blur");
    assert_eq!(accepted, b"accepted\n".len());
}

#[cfg(unix)]
mod native {
    use super::*;
    use iced::keyboard::Modifiers;
    use iced_term::{
        AlacrittyEvent, AlacrittyPoint, HistoryScroll, MouseButton, SelectionType, TermMode,
    };

    fn attached(script: &str) -> (SharedExecutor, App, SessionId) {
        let executor = SharedExecutor::new().unwrap();
        let mut app = App {
            profile: profile(script),
            ..App::default()
        };
        drop(app.update(Message::WindowFocused(true)));
        let (ids, ready) = launch(&mut app, &executor);
        drop(app.launched(ids, Ok(ready)));
        (executor, app, ids.session)
    }

    fn until(executor: &SharedExecutor, app: &mut App, ready: impl Fn(&App) -> bool) {
        let mut events = into_recipes(app.sessions.subscription())
            .pop()
            .unwrap()
            .stream(stream::pending().boxed());
        executor.block_on(async {
            tokio::time::timeout(DEADLINE, async {
                while !ready(app) {
                    let event = events.next().await.expect("missing fixture event");
                    drop(app.update(Message::Terminal(event)));
                }
            })
            .await
            .unwrap();
        });
    }

    fn offset(app: &App, id: SessionId) -> usize {
        app.terminal(id)
            .unwrap()
            .renderable_content()
            .unwrap()
            .viewport
            .display_offset()
    }

    fn mouse() -> B {
        B::MouseReport(
            MouseButton::ScrollUp,
            Modifiers::empty(),
            AlacrittyPoint::default(),
            true,
        )
    }

    #[test]
    fn hovered_mouse_and_alternate_wheel_are_owned_visible_and_do_not_focus() {
        let (executor, mut app, id) =
            attached("printf '\\033[?1049h\\033[?1007h\\033[?1000h\\033[?1006h'; read pike_wait");
        until(&executor, &mut app, |app| {
            app.terminal(id)
                .unwrap()
                .renderable_content()
                .unwrap()
                .terminal_mode
                .contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL | TermMode::SGR_MOUSE)
        });
        send(&mut app, id, B::FocusChanged(false));
        let initial = admitted(&app, id);
        send(&mut app, id, mouse());
        send(&mut app, id, B::Scroll(2));
        assert_eq!(admitted(&app, id) - initial, b"\x1b[<64;1;1M".len() + 6);
        assert!(
            !app.terminal(id)
                .unwrap()
                .renderable_content()
                .unwrap()
                .focused
        );
        assert_eq!(app.active_session(), Some(id));
        let before = admitted(&app, id);
        for reason in ["hidden", "ended", "raw", "foreign"] {
            if reason == "hidden" {
                app.sessions
                    .synchronize(&app.model, None, &app.window_focus);
            }
            if reason == "ended" {
                app.sessions.entries.get_mut(&id).unwrap().ended = true;
            }
            let mut foreign = Model::default();
            let (_, Effect::Spawn(other)) = foreign.create_tab().unwrap() else {
                unreachable!()
            };
            for command in [mouse(), B::Scroll(2), B::Write(b"stale".to_vec())] {
                drop(app.update(Message::Terminal((
                    if reason == "foreign" {
                        other.session
                    } else {
                        id
                    },
                    Event::BackendCall(
                        if reason == "foreign" {
                            other.session.get()
                        } else {
                            id.get() + u64::from(reason == "raw")
                        },
                        command,
                    ),
                ))));
            }
            assert_eq!(admitted(&app, id), before, "rejected {reason} input");
            app.sessions.entries.get_mut(&id).unwrap().ended = false;
            app.synchronize();
            send(&mut app, id, B::FocusChanged(false));
        }
        // Local history never becomes alternate-screen arrows, even unfocused.
        send(&mut app, id, B::ScrollHistory(HistoryScroll::Top));
        assert_eq!(admitted(&app, id), before);
        // Protocol replies still reach this owner without keyboard focus.
        send(
            &mut app,
            id,
            B::ProcessAlacrittyEvent(AlacrittyEvent::PtyWrite("\n".into())),
        );
        until(&executor, &mut app, |app| {
            matches!(
                app.model.sessions().get(id).unwrap().lifecycle(),
                Lifecycle::Exited { .. }
            )
        });
        let before = app.terminal(id).unwrap().input_status();
        send(&mut app, id, mouse());
        send(&mut app, id, B::Scroll(2));
        send(&mut app, id, B::ScrollHistory(HistoryScroll::PageUp));
        assert_eq!(app.terminal(id).unwrap().input_status(), before);
        assert!(app.sessions.entries[&id].notice.is_none());
        shutdown(&mut app, &executor);
    }

    #[test]
    fn hidden_history_is_ignored_and_exited_history_selection_copy_stay_local() {
        let (executor, mut app, id) = attached(
            "i=0; while [ $i -lt 100 ]; do printf 'PIKE23_HISTORY_%s\\r\\n' $i; i=$((i+1)); done; exit 7",
        );
        until(&executor, &mut app, |app| {
            matches!(
                app.model.sessions().get(id).unwrap().lifecycle(),
                Lifecycle::Exited { code: Some(7) }
            )
        });
        let before = app.terminal(id).unwrap().input_status();
        assert_eq!(offset(&app, id), 0);
        app.sessions
            .synchronize(&app.model, None, &app.window_focus);
        send(&mut app, id, B::ScrollHistory(HistoryScroll::Top));
        app.synchronize();
        let hidden_offset = offset(&app, id);
        send(&mut app, id, B::Scroll(5));
        assert_eq!(offset(&app, id), hidden_offset + 5);
        send(&mut app, id, B::ScrollHistory(HistoryScroll::Top));
        assert!(offset(&app, id) > 5);
        send(
            &mut app,
            id,
            B::SelectStart(SelectionType::Lines, (1.0, 1.0)),
        );
        let copied = app
            .terminal(id)
            .unwrap()
            .copy_selection(|text| text)
            .unwrap();
        assert!(copied.contains("PIKE23_HISTORY_"));
        assert!(app.update(Message::Control(Control::Copy(id))).units() > 0);
        send(&mut app, id, B::ScrollHistory(HistoryScroll::Bottom));
        assert_eq!(offset(&app, id), 0);
        send(&mut app, id, B::Write(b"closed".to_vec()));
        send(&mut app, id, mouse());
        assert_eq!(app.terminal(id).unwrap().input_status(), before);
        assert_eq!(into_recipes(app.sessions.subscription()).len(), 0);
        shutdown(&mut app, &executor);
        assert_eq!(hidden_offset, 0, "hidden history changed the viewport");
    }
}
