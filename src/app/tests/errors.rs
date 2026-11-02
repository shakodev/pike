//! Inject only synthetic failures; assert stable ownership and survivor behavior.
use super::*;
use crate::errors::{Action, Error, Owner};
use iced_term::{BackendCommand as B, EventBridgeError, EventBridgeEvent};

fn route(app: &mut App, id: SessionId, command: B) {
    drop(app.update(Message::Terminal((
        id,
        Event::BackendCall(id.get(), command),
    ))));
}

fn pair(executor: &SharedExecutor) -> (App, Reservation, Reservation) {
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    app.window_focus.set_focused(true);
    let (a, ready) = launch(&mut app, executor);
    drop(app.launched(a, Ok(ready)));
    let request = executor.enter(|| app.begin_new_tab()).unwrap();
    let b = Reservation {
        session: request.session,
        request: request.request,
    };
    let ready = executor.block_on(request.receive()).unwrap();
    drop(app.launched(b, Ok(ready)));
    drop(app.select(app.model.target(a.session).unwrap()));
    (app, a, b)
}

#[test]
fn injected_nonfatal_categories_coalesce_on_origin_without_focus_or_survivor_changes() {
    let executor = SharedExecutor::new().unwrap();
    let (mut app, healthy, origin) = pair(&executor);
    app.set_diagnostics_enabled(true);
    let before = app.terminal(healthy.session).unwrap().input_status();
    for (command, error) in [
        (
            B::InputRejected(iced_term::InputError::Full {
                requested: 7,
                available: 0,
            }),
            Error::Input(iced_term::InputError::Full {
                requested: 7,
                available: 0,
            }),
        ),
        (
            B::PasteRejected(iced_term::PasteError::ClipboardUnavailable),
            Error::Clipboard(iced_term::PasteError::ClipboardUnavailable),
        ),
        (
            B::ImeRejected(iced_term::ImeError::WrongSession),
            Error::Ime(iced_term::ImeError::WrongSession),
        ),
    ] {
        for _ in 0..1000 {
            route(&mut app, origin.session, command.clone());
        }
        let notice = app.latest_error(Owner::Session(origin.session)).unwrap();
        assert_eq!(notice.owner, Owner::Session(origin.session));
        assert_eq!(notice.error, error);
        assert_eq!(notice.count, 1000);
        assert_eq!(app.active_session(), Some(healthy.session));
        assert_eq!(
            app.terminal(healthy.session).unwrap().input_status(),
            before
        );
        assert_eq!(
            app.model
                .sessions()
                .get(healthy.session)
                .unwrap()
                .lifecycle(),
            Lifecycle::Running
        );
        assert_eq!(
            app.update(Message::Control(Control::ErrorAction(
                notice.id,
                Action::Dismiss
            )))
            .units(),
            0
        );
        assert_eq!(app.active_session(), Some(healthy.session));
    }
    let receipt = app.clipboard_writes.begin(Some(origin)).unwrap();
    drop(app.update(Message::ClipboardWritten(
        receipt,
        Err(iced_runtime::clipboard::WriteError::WriteFailed),
    )));
    assert_eq!(
        app.latest_error(Owner::Session(origin.session))
            .unwrap()
            .error,
        Error::ClipboardWrite(iced_runtime::clipboard::WriteError::WriteFailed)
    );
    let count = app.errors.len();
    drop(app.update(Message::Terminal((
        origin.session,
        Event::BackendCall(
            healthy.session.get(),
            B::PasteRejected(iced_term::PasteError::Pending),
        ),
    ))));
    assert_eq!(app.errors.len(), count);
    assert!(app.latest_error(Owner::Session(healthy.session)).is_none());
    assert!(app.diagnostic_capture().contains("count: 1000"));
    let admitted = before.written_bytes + before.outstanding_bytes + before.cancelled_bytes;
    route(&mut app, healthy.session, B::Write(b"x".to_vec()));
    let after = app.terminal(healthy.session).unwrap().input_status();
    assert_eq!(
        after.written_bytes + after.outstanding_bytes + after.cancelled_bytes,
        admitted + 1
    );
    shutdown(&mut app, &executor);
}

#[test]
fn launch_retry_uses_fresh_ids_and_stale_notice_or_clipboard_receipts_cannot_target_survivor() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    app.window_focus.set_focused(true);
    let (healthy, ready) = launch(&mut app, &executor);
    drop(app.launched(healthy, Ok(ready)));
    let (_, Effect::Spawn(failed)) = app.model.create_tab().unwrap() else {
        panic!("spawn")
    };
    app.profiles.insert(failed.session, waiting());
    drop(app.launched(
        failed,
        Err(LaunchError::Spawn(std::io::ErrorKind::PermissionDenied)),
    ));
    let notice = app.latest_error(Owner::Session(failed.session)).unwrap();
    drop(app.select(app.model.target(healthy.session).unwrap()));
    let before = app.terminal(healthy.session).unwrap().input_status();
    let task = executor.enter(|| {
        app.update(Message::Control(Control::ErrorAction(
            notice.id,
            Action::Retry,
        )))
    });
    let mut tasks = iced_runtime::task::into_stream(task).unwrap();
    let Some(iced_runtime::Action::Output(message)) = executor.block_on(tasks.next()) else {
        panic!("launch receipt")
    };
    drop(app.update(message));
    assert_eq!(app.active_session(), Some(healthy.session));
    assert!(app.model.sessions().get(failed.session).is_none());
    assert_eq!(app.model.sessions().len(), 2);
    assert_eq!(
        app.terminal(healthy.session).unwrap().input_status(),
        before
    );
    assert_eq!(
        app.update(Message::Control(Control::ErrorAction(
            notice.id,
            Action::UseDefault
        )))
        .units(),
        0
    );
    let receipt = app.clipboard_writes.begin(Some(failed)).unwrap();
    drop(app.update(Message::ClipboardWritten(
        receipt,
        Err(iced_runtime::clipboard::WriteError::Unavailable),
    )));
    assert!(app.latest_error(Owner::Session(failed.session)).is_none());
    assert!(app.latest_error(Owner::Session(healthy.session)).is_none());
    shutdown(&mut app, &executor);
}

#[test]
fn overload_and_io_completion_keep_source_and_healthy_session_and_gate_retry_on_real_cleanup() {
    let executor = SharedExecutor::new().unwrap();
    let (mut app, healthy, origin) = pair(&executor);
    let handle = app.terminal(origin.session).unwrap().shutdown_handle();
    let before = app.terminal(healthy.session).unwrap().input_status();
    route(
        &mut app,
        origin.session,
        B::EventBridge(EventBridgeEvent::Error(EventBridgeError::PayloadOverflow)),
    );
    let overload = app.latest_error(Owner::Session(origin.session)).unwrap();
    assert_eq!(
        overload.error,
        Error::Overload(EventBridgeError::PayloadOverflow)
    );
    assert_eq!(
        app.update(Message::Control(Control::ErrorAction(
            overload.id,
            Action::Retry
        )))
        .units(),
        0
    );
    let completion =
        executor.block_on(async { tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap() });
    assert!(handle.completion().is_some());
    // Inject an I/O error code only after the real backend has completed cleanup.
    route(
        &mut app,
        origin.session,
        B::EventBridge(EventBridgeEvent::BackendCompleted(
            iced_term::BackendCompletion {
                reason: iced_term::ExitReason::IoError(std::io::ErrorKind::BrokenPipe),
                ..completion
            },
        )),
    );
    assert_eq!(
        app.latest_error(Owner::Session(origin.session))
            .unwrap()
            .error,
        Error::Io(std::io::ErrorKind::BrokenPipe)
    );
    assert_eq!(app.active_session(), Some(healthy.session));
    assert_eq!(
        app.terminal(healthy.session).unwrap().input_status(),
        before
    );
    assert!(matches!(
        app.model
            .sessions()
            .get(origin.session)
            .unwrap()
            .lifecycle(),
        Lifecycle::Exited { .. }
    ));
    let admitted = before.written_bytes + before.outstanding_bytes + before.cancelled_bytes;
    route(&mut app, healthy.session, B::Write(b"y".to_vec()));
    let after = app.terminal(healthy.session).unwrap().input_status();
    assert_eq!(
        after.written_bytes + after.outstanding_bytes + after.cancelled_bytes,
        admitted + 1
    );
    shutdown(&mut app, &executor);
}

#[test]
fn real_terminal_input_output_environment_arguments_clipboard_and_titles_stay_out_of_capture() {
    let executor = SharedExecutor::new().unwrap();
    let secrets = [
        "SECRET_OUTPUT34",
        "SECRET_INPUT34",
        "SECRET_ENV34",
        "SECRET_ARGS34",
        "SECRET_CLIPBOARD34",
        "SECRET_TITLE34",
    ];
    let script = if cfg!(windows) {
        "echo SECRET_OUTPUT34&set PIKE34_ARGS=SECRET_ARGS34&set /p PIKE34="
    } else {
        "printf 'SECRET_OUTPUT34'; : SECRET_ARGS34; read pike34"
    };
    let mut profile = profile(script);
    profile
        .env
        .insert("PIKE34_SYNTHETIC".into(), secrets[2].into());
    let mut app = App {
        profile,
        ..App::default()
    };
    app.window_focus.set_focused(true);
    app.set_diagnostics_enabled(true);
    let (owner, ready) = launch(&mut app, &executor);
    drop(app.launched(owner, Ok(ready)));
    let mut events = into_recipes(app.sessions.subscription())
        .pop()
        .unwrap()
        .stream(stream::pending().boxed());
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(event) = events.next().await {
                drop(app.update(Message::Terminal(event)));
                let content = app
                    .terminal(owner.session)
                    .unwrap()
                    .renderable_content()
                    .unwrap();
                let text: String = content.viewport.cells().iter().map(|cell| cell.c).collect();
                if text.contains(secrets[0]) {
                    return;
                }
            }
            panic!("missing controlled output")
        })
        .await
        .unwrap();
    });
    route(
        &mut app,
        owner.session,
        B::Write(secrets[1].as_bytes().to_vec()),
    );
    route(
        &mut app,
        owner.session,
        B::ProcessAlacrittyEvent(iced_term::AlacrittyEvent::Title(format!(
            "\u{202e}{}{}",
            secrets[5],
            "x".repeat(4096)
        ))),
    );
    assert!(
        app.sessions.entries[&owner.session]
            .title
            .as_ref()
            .unwrap()
            .as_str()
            .chars()
            .count()
            <= crate::tabs::TITLE_LIMIT
    );
    // Send real synthetic clipboard content through the common terminal adapter.
    let terminal = app
        .sessions
        .entries
        .get_mut(&owner.session)
        .unwrap()
        .terminal_mut()
        .unwrap();
    let mut clipboard = secrets[4].to_owned();
    clipboard.extend(std::iter::repeat_n('x', iced_term::PASTE_BYTE_LIMIT));
    let iced_term::actions::Action::PasteRejected(error) = terminal.paste(Some(clipboard)) else {
        panic!("whole-paste admission")
    };
    route(&mut app, owner.session, B::PasteRejected(error));
    for _ in 0..1000 {
        route(
            &mut app,
            owner.session,
            B::PasteRejected(iced_term::PasteError::TooLarge {
                bytes: iced_term::PASTE_BYTE_LIMIT + 1,
                limit: iced_term::PASTE_BYTE_LIMIT,
            }),
        );
    }
    app.report_error(
        Owner::Application,
        Error::Config(crate::settings::Error::Invalid(
            crate::settings::Field::Environment,
        )),
    );
    let capture = app.diagnostic_capture();
    for secret in secrets {
        assert!(!capture.contains(secret));
    }
    assert!(capture.len() < crate::diagnostics::RECORD_LIMIT * 1024);
    app.set_diagnostics_enabled(false);
    assert!(app.diagnostic_capture().is_empty());
    shutdown(&mut app, &executor);
}

#[test]
fn cleanup_timeout_records_original_ids_and_renderer_failure_uses_actual_pool_shutdown() {
    let executor = SharedExecutor::new().unwrap();
    let (mut app, a, b) = pair(&executor);
    app.set_diagnostics_enabled(true);
    let handles: Vec<_> = [a, b]
        .map(|owner| app.terminal(owner.session).unwrap().shutdown_handle())
        .into();
    let task = app.update(Message::RendererFailed(
        iced_runtime::system::RendererFailure::OutOfMemory,
    ));
    assert!(app.exiting);
    assert!(app.close_confirmation().is_none());
    assert_eq!(
        app.latest_error(Owner::Application).unwrap().error,
        Error::Renderer(iced_runtime::system::RendererFailure::OutOfMemory)
    );
    let token = app.close_policy.timer.unwrap();
    app.close_timeout(token);
    for owner in [a, b] {
        assert_eq!(
            app.latest_error(Owner::Session(owner.session))
                .unwrap()
                .error,
            Error::Cleanup
        );
    }
    let mut tasks = iced_runtime::task::into_stream(task).unwrap();
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(action) = tasks.next().await {
                if matches!(
                    action,
                    iced_runtime::Action::Output(Message::ShutdownComplete)
                ) {
                    return;
                }
            }
            panic!("missing pool cleanup receipt")
        })
        .await
        .unwrap();
    });
    assert!(app.launches.shutdown_complete());
    assert!(handles.iter().all(|h| h.completion().is_some()));
    assert!(app.shutdown_complete().units() > 0);
    assert!(app.errors.is_empty());
    assert!(app.diagnostic_capture().is_empty());
}

#[test]
fn current_resize_failure_and_retry_preserve_owner_focus_and_healthy_input() {
    let executor = SharedExecutor::new().unwrap();
    let (mut app, healthy, origin) = pair(&executor);
    app.model
        .move_to_tab(
            app.model.target(origin.session).unwrap(),
            app.model.target(healthy.session).unwrap(),
            iced::widget::pane_grid::Axis::Vertical,
        )
        .unwrap();
    drop(app.select(app.model.target(healthy.session).unwrap()));
    let mut events = into_recipes(app.sessions.subscription())
        .into_iter()
        .map(|r| r.stream(stream::pending().boxed()))
        .collect::<Vec<_>>();
    let mut all = stream::select_all(events.drain(..));
    route(
        &mut app,
        origin.session,
        B::Resize(Some(Size::new(800.0, 400.0)), Some(Size::new(8.0, 16.0))),
    );
    let result = executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some((id, event)) = all.next().await {
                if let Event::BackendCall(
                    _,
                    B::ProcessAlacrittyEvent(iced_term::AlacrittyEvent::ResizeCompleted(result)),
                ) = &event
                    && id == origin.session
                    && result.size.num_cols == 100
                    && result.size.num_lines == 25
                {
                    return *result;
                }
                drop(app.update(Message::Terminal((id, event))));
            }
            panic!("native resize result")
        })
        .await
        .unwrap()
    });
    let error = iced_term::ResizeError {
        kind: std::io::ErrorKind::PermissionDenied,
        code: Some(5),
    };
    route(
        &mut app,
        origin.session,
        B::ProcessAlacrittyEvent(iced_term::AlacrittyEvent::ResizeCompleted(
            iced_term::ResizeResult {
                result: Err(error),
                ..result
            },
        )),
    );
    let notice = app.latest_error(Owner::Session(origin.session)).unwrap();
    assert_eq!(notice.error, Error::Resize(error));
    assert!(!notice.resolved);
    assert_eq!(app.active_session(), Some(healthy.session));
    let count = app
        .terminal(origin.session)
        .unwrap()
        .work_stats()
        .pty_resizes;
    for _ in 0..1000 {
        route(
            &mut app,
            origin.session,
            B::Resize(Some(Size::new(800.0, 400.0)), Some(Size::new(8.0, 16.0))),
        );
    }
    assert_eq!(
        app.terminal(origin.session)
            .unwrap()
            .work_stats()
            .pty_resizes,
        count
    );
    drop(app.update(Message::Control(Control::ErrorAction(
        notice.id,
        Action::Retry,
    ))));
    assert_eq!(
        app.terminal(origin.session)
            .unwrap()
            .work_stats()
            .pty_resizes,
        count + 1
    );
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(event) = all.next().await {
                drop(app.update(Message::Terminal(event)));
                if app
                    .latest_error(Owner::Session(origin.session))
                    .is_some_and(|n| n.resolved)
                {
                    return;
                }
            }
            panic!("retry acknowledgment")
        })
        .await
        .unwrap();
    });
    assert_eq!(
        app.latest_error(Owner::Session(origin.session)).unwrap().id,
        notice.id
    );
    assert_eq!(app.active_session(), Some(healthy.session));
    let before = app.terminal(healthy.session).unwrap().input_status();
    route(&mut app, healthy.session, B::Write(b"r".to_vec()));
    let after = app.terminal(healthy.session).unwrap().input_status();
    assert_eq!(
        after.written_bytes + after.outstanding_bytes + after.cancelled_bytes,
        before.written_bytes + before.outstanding_bytes + before.cancelled_bytes + 1
    );
    drop(app.update(Message::Control(Control::ErrorAction(
        notice.id,
        Action::Dismiss,
    ))));
    assert!(app.latest_error(Owner::Session(origin.session)).is_none());
    shutdown(&mut app, &executor);
}

#[test]
fn clipboard_retry_controls_are_one_use_focus_bound_and_native_acks_are_ordered() {
    let executor = SharedExecutor::new().unwrap();
    let (mut app, owner, other) = pair(&executor);
    app.report_error(
        Owner::Session(owner.session),
        Error::Clipboard(iced_term::PasteError::ClipboardUnavailable),
    );
    let notice = app.latest_error(Owner::Session(owner.session)).unwrap();
    let held = app.recovery_request(notice.id).unwrap();
    drop(app.select(app.model.target(other.session).unwrap()));
    drop(app.select(app.model.target(owner.session).unwrap()));
    assert_eq!(app.update(Message::ErrorRetry(held)).units(), 0);
    let blurred = app.recovery_request(notice.id).unwrap();
    drop(app.update(Message::WindowFocused(false)));
    drop(app.update(Message::WindowFocused(true)));
    assert_eq!(app.update(Message::ErrorRetry(blurred)).units(), 0);
    let request = app.recovery_request(notice.id).unwrap();
    let replay = request.clone();
    let task = app.update(Message::ErrorRetry(request));
    assert!(
        app.latest_error(Owner::Session(owner.session))
            .unwrap()
            .retrying
    );
    assert_eq!(app.update(Message::ErrorRetry(replay)).units(), 0);
    assert_eq!(
        app.update(Message::Control(Control::ErrorAction(
            notice.id,
            Action::Retry
        )))
        .units(),
        0
    );
    let mut actions = iced_runtime::task::into_stream(task).unwrap();
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            let mut reads = 0;
            while let Some(action) = actions.next().await {
                match action {
                    iced_runtime::Action::Clipboard(iced_runtime::clipboard::Action::Read {
                        channel,
                        ..
                    }) => {
                        reads += 1;
                        channel.send(None).unwrap();
                    }
                    iced_runtime::Action::Output(message @ Message::CommandPaste(..)) => {
                        drop(app.update(message));
                        break;
                    }
                    _ => {}
                }
            }
            assert_eq!(reads, 1);
        })
        .await
        .unwrap();
    });
    let failed_again = app.latest_error(Owner::Session(owner.session)).unwrap();
    assert_ne!(failed_again.id, notice.id);
    assert!(!failed_again.retrying);
    assert_eq!(
        app.update(Message::Control(Control::ErrorAction(
            notice.id,
            Action::Dismiss
        )))
        .units(),
        0
    );
    assert!(app.latest_error(Owner::Session(owner.session)).is_some());
    let old = app.clipboard_writes.begin(Some(owner)).unwrap();
    let current = app.clipboard_writes.begin(Some(owner)).unwrap();
    drop(app.update(Message::ClipboardWritten(current, Ok(()))));
    drop(app.update(Message::ClipboardWritten(
        old,
        Err(iced_runtime::clipboard::WriteError::WriteFailed),
    )));
    drop(app.update(Message::ClipboardWritten(
        current,
        Err(iced_runtime::clipboard::WriteError::Unavailable),
    )));
    assert_eq!(
        app.latest_error(Owner::Session(owner.session)).unwrap().id,
        failed_again.id
    );
    assert_eq!(app.active_session(), Some(owner.session));
    assert_eq!(
        app.model.sessions().get(other.session).unwrap().lifecycle(),
        Lifecycle::Running
    );
    shutdown(&mut app, &executor);
}

#[test]
fn config_retry_and_command_refusal_keep_current_owner_and_private_values_out_of_capture() {
    let executor = SharedExecutor::new().unwrap();
    let (mut app, healthy, other) = pair(&executor);
    app.set_diagnostics_enabled(true);
    let before = app.terminal(healthy.session).unwrap().input_status();
    let settings_before = app.active_settings().clone();
    app.settings.pending = true;
    assert_eq!(
        app.update(Message::SettingsLoaded(Err(
            crate::settings::Error::Invalid(crate::settings::Field::Environment)
        )))
        .units(),
        0
    );
    assert_eq!(app.active_settings(), &settings_before);
    let notice = app.latest_error(Owner::Application).unwrap();
    assert!(matches!(notice.error, Error::Config(_)));
    let path = std::env::temp_dir().join(format!(
        "pike34-config-retry-{}.toml",
        healthy.session.get()
    ));
    std::fs::write(&path, "version=1\n[profiles.default]\nshell='pike34-unexecuted-shell'\nargs=['SECRET_CONFIG_ARGS34']\nenv={ PIKE34='SECRET_CONFIG_ENV34' }\n").unwrap();
    app.settings.path = Some(path.clone());
    let task = app.update(Message::Control(Control::ErrorAction(
        notice.id,
        Action::Retry,
    )));
    assert!(app.settings_pending());
    assert_eq!(
        app.update(Message::Control(Control::ErrorAction(
            notice.id,
            Action::Retry
        )))
        .units(),
        0
    );
    let mut actions = iced_runtime::task::into_stream(task).unwrap();
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(action) = actions.next().await {
                if let iced_runtime::Action::Output(message @ Message::SettingsLoaded(..)) = action
                {
                    assert_eq!(app.update(message).units(), 0);
                    return;
                }
            }
            panic!("configuration completion")
        })
        .await
        .unwrap();
    });
    std::fs::remove_file(path).unwrap();
    assert!(app.latest_error(Owner::Application).is_none());
    drop(app.update(Message::Control(Control::Command(
        commands::Command::SelectTab(commands::TabSelection::Index(7)),
    ))));
    assert_eq!(
        app.latest_error(Owner::Session(healthy.session))
            .unwrap()
            .error,
        Error::Operation(commands::Error::MissingTab)
    );
    assert_eq!(app.active_session(), Some(healthy.session));
    assert_eq!(
        app.terminal(healthy.session).unwrap().input_status(),
        before
    );
    assert_eq!(
        app.model.sessions().get(other.session).unwrap().lifecycle(),
        Lifecycle::Running
    );
    let capture = app.diagnostic_capture();
    assert!(!capture.contains("SECRET_CONFIG_ARGS34"));
    assert!(!capture.contains("SECRET_CONFIG_ENV34"));
    shutdown(&mut app, &executor);
}
