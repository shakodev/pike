//! Native controlled shells; synthetic data only, no user terminal logging.
use super::*;
use iced_term::BackendCommand as B;
use std::time::Instant;

mod ime;
#[cfg(windows)]
mod windows;

fn drain_exit(app: &mut App, executor: &SharedExecutor, id: SessionId) {
    let mut events = into_recipes(app.sessions.subscription())
        .pop()
        .unwrap()
        .stream(stream::pending().boxed());
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(event) = events.next().await {
                drop(app.update(Message::Terminal(event)));
                if matches!(
                    app.model.sessions().get(id).map(|s| s.lifecycle()),
                    Some(Lifecycle::Exited { .. })
                ) {
                    return;
                }
            }
            panic!("missing native completion");
        })
        .await
        .unwrap();
    });
}

#[test]
fn success_error_exit_keeps_final_sentinel_and_bounded_read_only_model() {
    let executor = SharedExecutor::new().unwrap();
    for code in [0, 7] {
        let script = if cfg!(windows) {
            format!(
                "for /L %i in (1,1,120) do @echo PIKE28_line_%i&echo PIKE28_FINAL&exit /b {code}"
            )
        } else {
            format!(
                "i=0; while [ $i -lt 120 ]; do echo PIKE28_line_$i; i=$((i+1)); done; printf '\\r\\nPIKE28_FINAL\\r\\n'; exit {code}"
            )
        };
        let mut settings = profile(&script);
        settings.history = 8.try_into().unwrap();
        let mut app = App {
            profile: settings,
            ..App::default()
        };
        let (ids, ready) = launch(&mut app, &executor);
        let handle = ready.shutdown_handle();
        drop(app.launched(ids, Ok(ready)));
        drain_exit(&mut app, &executor, ids.session);
        assert_eq!(
            app.model.sessions().get(ids.session).unwrap().lifecycle(),
            Lifecycle::Exited { code: Some(code) }
        );
        let terminal = app.terminal(ids.session).unwrap();
        let content = terminal.renderable_content().unwrap();
        let text: String = content.viewport.cells().iter().map(|cell| cell.c).collect();
        assert!(text.contains("PIKE28_FINAL"));
        assert!(terminal.snapshot_stats().history_lines <= 8);
        assert_eq!(terminal.snapshot_stats().history_limit.lines(), 8);
        assert!(terminal.input_status().closed);
        let before = terminal.input_status();
        assert!(handle.completion().unwrap().output_drained);
        drop(app.update(Message::Terminal((
            ids.session,
            Event::BackendCall(ids.session.get(), B::Write(b"stale\n".to_vec())),
        ))));
        assert_eq!(app.terminal(ids.session).unwrap().input_status(), before);
        assert_eq!(into_recipes(app.sessions.subscription()).len(), 0);
        drop(app.update(Message::Control(Control::CloseSession(ids.session))));
        assert!(app.close_confirmation().is_none());
        assert!(app.model.sessions().is_empty());
        assert!(app.active_session().is_none());
        assert!(!app.exiting);
        shutdown(&mut app, &executor);
    }
}

#[test]
fn modal_accepts_launch_and_natural_completion_without_closing_or_focusing_a_target() {
    let executor = SharedExecutor::new().unwrap();
    let settings = profile(if cfg!(windows) {
        "echo PIKE28_MODAL_FINAL&exit /b 9"
    } else {
        "printf 'PIKE28_MODAL_FINAL\\r\\n'; exit 9"
    });
    let mut app = App {
        profile: settings,
        ..App::default()
    };
    drop(app.update(Message::WindowFocused(true)));
    let request = executor.enter(|| app.begin_open()).unwrap();
    let ids = Reservation {
        session: request.session,
        request: request.request,
    };
    drop(app.update(Message::Control(Control::CloseSession(ids.session))));
    let (decision, _) = app.close_confirmation().unwrap();
    let ready = executor.block_on(async {
        tokio::time::timeout(DEADLINE, request.receive())
            .await
            .unwrap()
            .unwrap()
    });
    drop(app.launched(ids, Ok(ready)));
    assert!(app.close_confirmation().is_some());
    assert!(
        !app.terminal(ids.session)
            .unwrap()
            .renderable_content()
            .is_some_and(|c| c.focused)
    );
    drain_exit(&mut app, &executor, ids.session);
    assert_eq!(
        app.model.sessions().get(ids.session).unwrap().lifecycle(),
        Lifecycle::Exited { code: Some(9) }
    );
    assert_eq!(app.active_session(), Some(ids.session));
    drop(
        app.update(Message::Control(Control::CloseAction(CloseAction::Cancel(
            decision,
        )))),
    );
    let final_text: String = app
        .terminal(ids.session)
        .unwrap()
        .renderable_content()
        .unwrap()
        .viewport
        .cells()
        .iter()
        .map(|cell| cell.c)
        .collect();
    assert!(final_text.contains("PIKE28_MODAL_FINAL"));
    assert!(!app.exiting);
    assert_eq!(app.active_session(), Some(ids.session));
    shutdown(&mut app, &executor);
}

#[test]
fn in_place_restart_rejects_old_close_launch_and_terminal_events_and_keeps_sibling() {
    use iced::widget::pane_grid::Axis;
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: profile(if cfg!(windows) {
            "echo PIKE28_RESTART_FINAL&exit /b 4"
        } else {
            "printf 'PIKE28_RESTART_FINAL\\r\\n'; exit 4"
        }),
        ..App::default()
    };
    let (old, ready) = launch(&mut app, &executor);
    drop(app.launched(old, Ok(ready)));
    drain_exit(&mut app, &executor, old.session);
    let target = app.model.target(old.session).unwrap();
    let (sibling, Effect::Spawn(other)) = app.model.split(target, Axis::Vertical).unwrap() else {
        unreachable!()
    };
    let request = executor
        .enter(|| app.submit_profile(other, waiting()))
        .unwrap();
    let ready = executor.block_on(request.receive()).unwrap();
    drop(app.launched(other, Ok(ready)));
    // Keep the restarted child live so stale shutdown would be observable.
    app.profiles.insert(old.session, waiting());
    let request = executor
        .enter(|| app.begin_retry(old.session, false))
        .unwrap();
    let next = Reservation {
        session: request.session,
        request: request.request,
    };
    assert_ne!(next.session, old.session);
    assert_ne!(next.request, old.request);
    assert_eq!(app.model.target(next.session), Some(target));
    let ready = executor.block_on(request.receive()).unwrap();
    drop(app.launched(next, Ok(ready)));
    let before = app.terminal(next.session).unwrap().input_status();
    drop(app.update(Message::Control(Control::CloseSession(old.session))));
    drop(app.update(Message::Control(Control::Retry(old.session))));
    drop(app.update(Message::Disposed(old)));
    drop(app.update(Message::Launched(old, Err(LaunchError::Canceled))));
    for raw in [old.session.get(), next.session.get()] {
        drop(app.update(Message::Terminal((
            old.session,
            Event::BackendCall(raw, B::RequestShutdown),
        ))));
    }
    assert!(app.close_confirmation().is_none());
    assert_eq!(
        app.model.workspace().tab(target.tab).unwrap().grid().len(),
        2
    );
    assert_eq!(
        app.model.workspace().pane(target).unwrap().session,
        next.session
    );
    assert_eq!(
        app.model.workspace().pane(sibling).unwrap().session,
        other.session
    );
    assert_eq!(app.terminal(next.session).unwrap().input_status(), before);
    assert!(!app.terminal(other.session).unwrap().input_status().closed);
    let handles = [
        app.terminal(next.session).unwrap().shutdown_handle(),
        app.terminal(other.session).unwrap().shutdown_handle(),
    ];
    shutdown(&mut app, &executor);
    assert!(handles.iter().all(|handle| handle.completion().is_some()));
}

fn click_dialog(app: &App, renderer: &iced::Renderer, confirm: bool) -> Vec<Message> {
    use iced::advanced::widget::Tree;
    use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
    let mut view = app.view();
    let mut tree = Tree::new(view.as_widget());
    let size = iced::Size::new(900.0, 600.0);
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &layout::Limits::new(size, size));
    let buttons = Layout::new(&node)
        .children()
        .next()
        .unwrap()
        .children()
        .last()
        .unwrap();
    let button = buttons
        .children()
        .nth(usize::from(confirm))
        .unwrap()
        .bounds();
    let mut messages = Vec::new();
    for event in [
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ] {
        view.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Available(button.center()),
            renderer,
            &mut clipboard::Null,
            &mut Shell::new(&mut messages),
            &iced::Rectangle::with_size(size),
        );
    }
    messages
}

#[test]
#[ignore = "requires a local graphics device; run explicitly for close controls"]
fn actual_modal_buttons_cancel_restore_focus_and_confirm_only_the_owned_session() {
    use iced::advanced::renderer::Headless;
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
    let original = app.active_target();
    let before = app.terminal(ids.session).unwrap().input_status();
    for confirm in [false, true] {
        drop(app.update(Message::Control(Control::CloseSession(ids.session))));
        assert!(
            !app.terminal(ids.session)
                .unwrap()
                .renderable_content()
                .is_some_and(|c| c.focused)
        );
        drop(app.update(Message::Terminal((
            ids.session,
            Event::BackendCall(ids.session.get(), B::Write(b"never\n".to_vec())),
        ))));
        assert_eq!(app.terminal(ids.session).unwrap().input_status(), before);
        let messages = click_dialog(&app, &renderer, confirm);
        assert_eq!(messages.len(), 1);
        assert!(
            matches!(
                &messages[0],
                Message::Control(Control::CloseAction(CloseAction::Confirm(_)))
            ) == confirm
        );
        for message in messages {
            drop(app.update(message));
        }
        if !confirm {
            assert_eq!(app.active_target(), original);
            assert!(
                app.terminal(ids.session)
                    .unwrap()
                    .renderable_content()
                    .unwrap()
                    .focused
            );
            assert_eq!(
                app.model.sessions().get(ids.session).unwrap().lifecycle(),
                Lifecycle::Running
            );
        }
    }
    assert!(app.model.workspace().order().is_empty());
    shutdown(&mut app, &executor);
    // The same native widget event path selects both timeout controls. The
    // force effect is inspected here; its process exit is tested in a child.
    for force in [false, true] {
        let token = app.close_policy.timer.unwrap();
        drop(app.update(Message::CloseTimeout(token)));
        let messages = click_dialog(&app, &renderer, force);
        assert_eq!(messages.len(), 1);
        assert_eq!(
            matches!(
                &messages[0],
                Message::Control(Control::CloseAction(CloseAction::Force(_)))
            ),
            force
        );
        for message in messages {
            let task = app.update(message);
            assert!(task.units() > 0);
            drop(task);
        }
    }
}

#[test]
fn close_during_output_is_nonblocking_idempotent_and_waits_for_native_receipt() {
    let executor = SharedExecutor::new().unwrap();
    let script = if cfg!(windows) {
        "for /L %i in (1,1,999999) do @echo PIKE28_FLOOD"
    } else {
        "while :; do printf 'PIKE28_FLOOD\\r\\n'; done"
    };
    let mut app = App {
        profile: profile(script),
        ..App::default()
    };
    let (ids, ready) = launch(&mut app, &executor);
    let handle = ready.shutdown_handle();
    drop(app.launched(ids, Ok(ready)));
    let mut events = into_recipes(app.sessions.subscription())
        .pop()
        .unwrap()
        .stream(stream::pending().boxed());
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, events.next())
            .await
            .unwrap()
            .unwrap();
    });
    let began = Instant::now();
    drop(app.update(Message::Control(Control::CloseSession(ids.session))));
    confirm_close(&mut app);
    let dispatch = began.elapsed();
    assert!(dispatch < Duration::from_millis(100));
    assert!(app.model.workspace().order().is_empty());
    assert!(matches!(
        app.model.sessions().get(ids.session).unwrap().lifecycle(),
        Lifecycle::Closing { .. }
    ));
    for _ in 0..16 {
        drop(app.update(Message::Control(Control::CloseSession(ids.session))));
    }
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(event) = events.next().await {
                drop(app.update(Message::Terminal(event)));
                if app.model.sessions().get(ids.session).is_none() {
                    return;
                }
            }
            panic!("missing cleanup receipt");
        })
        .await
        .unwrap();
    });
    assert!(handle.completion().is_some());
    assert!(app.sessions.entries.is_empty());
    assert!(app.model.sessions().is_empty());
    eprintln!(
        "pike28 native output close dispatch_us={} cleanup_ms={}",
        dispatch.as_micros(),
        began.elapsed().as_millis()
    );
    shutdown(&mut app, &executor);
}

#[cfg(unix)]
#[test]
fn stalled_reader_window_timeout_wait_remains_responsive_until_actual_cleanup() {
    use super::cancellation::CleanupGate;
    let executor = SharedExecutor::new().unwrap();
    let mut app = App::default();
    let (_, Effect::Spawn(ids)) = app.model.create_tab().unwrap() else {
        unreachable!()
    };
    let mut cleanup = CleanupGate::new(ids.session);
    let marker = cleanup.directory.join("ready");
    let fifo = cleanup.directory.join("release");
    let settings = BackendSettings {
        program: "/bin/sh".into(),
        args: vec![
            "-c".into(),
            "exec 3< \"$2\"; trap '' HUP; echo ready > \"$1\"; read pike_release <&3".into(),
            "pike28-stalled".into(),
            marker.to_string_lossy().into_owned(),
            fifo.to_string_lossy().into_owned(),
        ],
        ..Default::default()
    };
    let token = app.model.claim_launch(ids).unwrap();
    let request = executor
        .enter(|| app.launches.start_reserved(token, settings))
        .unwrap();
    app.sessions.entries.insert(
        ids.session,
        crate::terminal::Entry::new(ids, Owner::Starting(request.cancellation())),
    );
    let ready = executor.block_on(async {
        tokio::time::timeout(DEADLINE, request.receive())
            .await
            .unwrap()
            .unwrap()
    });
    let handle = ready.shutdown_handle();
    drop(app.launched(ids, Ok(ready)));
    let ready_at = Instant::now();
    while !marker.exists() {
        assert!(ready_at.elapsed() < DEADLINE);
        std::thread::sleep(Duration::from_millis(5)); // Controlled fixture readiness.
    }
    drop(app.update(Message::WindowFocused(true)));
    let original = app.active_target();
    drop(app.update(Message::CloseWindow));
    let (cancel, count) = app.close_confirmation().unwrap();
    assert_eq!(count, 1);
    drop(
        app.update(Message::Control(Control::CloseAction(CloseAction::Cancel(
            cancel,
        )))),
    );
    assert_eq!(app.active_target(), original);
    assert!(!app.exiting);
    assert!(handle.completion().is_none());
    let began = Instant::now();
    drop(app.update(Message::CloseWindow));
    confirm_close(&mut app);
    let dispatch = began.elapsed();
    let timer = app.close_policy.timer.unwrap();
    assert!(app.update(Message::ShutdownComplete).units() == 0); // Premature receipt cannot exit.
    assert!(!app.launches.shutdown_complete());
    assert!(handle.completion().is_none());
    let elapsed = Instant::now();
    let timed = executor.block_on(crate::app::close::wait_deadline(timer));
    let timer_elapsed = elapsed.elapsed();
    drop(app.update(Message::CloseTimeout(timed)));
    assert!(timer_elapsed >= Duration::from_secs(3));
    assert!(app.close_policy.timed_out);
    assert!(app.close_view().is_some());
    let wait = Instant::now();
    drop(
        app.update(Message::Control(Control::CloseAction(CloseAction::Wait(
            timer,
        )))),
    );
    let wait_elapsed = wait.elapsed();
    assert!(dispatch < Duration::from_millis(100));
    assert!(wait_elapsed < Duration::from_millis(100));
    assert!(!app.close_policy.timed_out);
    drop(app.update(Message::CloseTimeout(timer))); // Old timer cannot re-open choices.
    assert!(!app.close_policy.timed_out);
    assert!(
        app.update(Message::Control(Control::CloseAction(CloseAction::Force(
            timer
        ))))
        .units()
            == 0
    );
    for _ in 0..16 {
        assert_eq!(app.update(Message::CloseWindow).units(), 0);
        assert_eq!(app.update(Message::Control(Control::New)).units(), 0);
    }
    assert!(app.model.sessions().get(ids.session).is_some());
    assert!(handle.completion().is_none());
    cleanup.writer.take();
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, app.launches.shutdown())
            .await
            .unwrap()
    });
    assert!(handle.completion().is_some());
    assert!(app.update(Message::ShutdownComplete).units() > 0);
    assert!(app.model.sessions().is_empty());
    assert!(app.sessions.entries.is_empty());
    eprintln!(
        "pike28 native stalled close_us={} deadline_ms={} wait_us={} total_ms={}",
        dispatch.as_micros(),
        timer_elapsed.as_millis(),
        wait_elapsed.as_micros(),
        began.elapsed().as_millis()
    );
}
