use super::*;
use crate::executor::SharedExecutor;
use crate::session::{Effect, Failure};
use iced::Executor;
use iced::advanced::subscription::into_recipes;
use iced::futures::{StreamExt, stream};
use std::time::Duration;

const DEADLINE: Duration = Duration::from_secs(10);

#[cfg(unix)]
mod cancellation;
mod closing;
mod ime;
mod interaction;
mod paste;
mod resize;
mod split;
mod tabs;

fn profile(script: &str) -> BackendSettings {
    #[cfg(windows)]
    let (program, args) = ("cmd.exe", vec!["/D", "/Q", "/C", script]);
    #[cfg(not(windows))]
    let (program, args) = ("/bin/sh", vec!["-c", script]);
    BackendSettings {
        program: program.into(),
        args: args.into_iter().map(str::to_owned).collect(),
        #[cfg(windows)]
        escape_args: false,
        ..Default::default()
    }
}

fn waiting() -> BackendSettings {
    profile(if cfg!(windows) {
        "set /p PIKE_WAIT="
    } else {
        "read pike_wait"
    })
}

fn launch(app: &mut App, executor: &SharedExecutor) -> (Reservation, Box<PreparedSession>) {
    let request = executor.enter(|| app.begin_open()).unwrap();
    let ids = Reservation {
        session: request.session,
        request: request.request,
    };
    let ready = executor.block_on(async {
        tokio::time::timeout(DEADLINE, request.receive())
            .await
            .unwrap()
            .unwrap()
    });
    (ids, ready)
}

fn shutdown(app: &mut App, executor: &SharedExecutor) {
    drop(app.update(Message::CloseWindow));
    confirm_close(app);
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, app.launches.shutdown())
            .await
            .unwrap();
    });
}

fn confirm_close(app: &mut App) {
    if let Some((token, _)) = app.close_confirmation() {
        drop(app.update(Message::Control(Control::CloseAction(
            CloseAction::Confirm(token),
        ))));
    }
}

#[test]
fn first_reservation_attaches_with_its_id_on_the_update_thread_and_requests_focus() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    drop(app.update(Message::WindowFocused(true)));
    let (ids, ready) = launch(&mut app, &executor);
    assert_eq!(
        app.model.sessions().get(ids.session).unwrap().lifecycle(),
        Lifecycle::Starting
    );
    assert_eq!(app.active_session(), Some(ids.session));
    let focus = app.launched(ids, Ok(ready));
    assert!(focus.units() > 0);
    let terminal = app.terminal(ids.session).unwrap();
    assert_eq!(terminal.id, ids.session.get());
    assert!(terminal.is_visible());
    assert_eq!(terminal.work_stats().font_measurements, 1);
    assert_eq!(
        app.model.sessions().get(ids.session).unwrap().lifecycle(),
        Lifecycle::Running
    );
    assert_eq!(into_recipes(app.sessions.subscription()).len(), 1);
    shutdown(&mut app, &executor);
}

#[test]
fn invalid_launch_is_redacted_and_retry_reserves_a_fresh_session() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: BackendSettings {
            program: "/pike/synthetic/missing-secret-shell".into(),
            ..Default::default()
        },
        ..App::default()
    };
    let request = executor.enter(|| app.begin_open()).unwrap();
    let ids = Reservation {
        session: request.session,
        request: request.request,
    };
    let error = executor.block_on(request.receive()).unwrap_err();
    assert!(!error.to_string().contains("secret"));
    drop(app.launched(ids, Err(error)));
    assert!(matches!(
        app.model.sessions().get(ids.session).unwrap().lifecycle(),
        Lifecycle::Failed(_)
    ));
    assert_eq!(into_recipes(app.sessions.subscription()).len(), 0);
    app.close(app.active_target().unwrap());
    app.profile = waiting();
    let (next, ready) = launch(&mut app, &executor);
    assert_ne!(next.session, ids.session);
    drop(app.launched(next, Ok(ready)));
    let before = app.terminal(next.session).unwrap().input_status();
    drop(app.update(Message::Terminal((
        ids.session,
        Event::BackendCall(
            next.session.get(),
            iced_term::BackendCommand::Write(b"stale".to_vec()),
        ),
    ))));
    drop(app.update(Message::Control(Control::Retry(ids.session))));
    assert_eq!(app.terminal(next.session).unwrap().input_status(), before);
    assert_eq!(app.active_session(), Some(next.session));
    shutdown(&mut app, &executor);
}

#[test]
fn rapid_exit_retains_final_output_without_a_live_subscription_or_admitted_input() {
    let executor = SharedExecutor::new().unwrap();
    let script = if cfg!(windows) {
        "echo PIKE23_FINAL&exit /b 7"
    } else {
        "printf PIKE23_FINAL; exit 7"
    };
    let mut app = App {
        profile: profile(script),
        ..App::default()
    };
    let (ids, ready) = launch(&mut app, &executor);
    drop(app.launched(ids, Ok(ready)));
    let mut events = into_recipes(app.sessions.subscription())
        .pop()
        .unwrap()
        .stream(stream::pending().boxed());
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(event) = events.next().await {
                drop(app.update(Message::Terminal(event)));
                if matches!(
                    app.model.sessions().get(ids.session).unwrap().lifecycle(),
                    Lifecycle::Exited { .. }
                ) {
                    return;
                }
            }
            panic!("missing completion");
        })
        .await
        .unwrap();
    });
    assert_eq!(
        app.model.sessions().get(ids.session).unwrap().lifecycle(),
        Lifecycle::Exited { code: Some(7) }
    );
    let terminal = app.terminal(ids.session).unwrap();
    let text: String = terminal
        .renderable_content()
        .unwrap()
        .viewport
        .cells()
        .iter()
        .map(|cell| cell.c)
        .collect();
    assert!(text.contains("PIKE23_FINAL"));
    assert!(terminal.input_status().closed);
    let before = terminal.input_status();
    drop(app.update(Message::Terminal((
        ids.session,
        Event::BackendCall(
            ids.session.get(),
            iced_term::BackendCommand::Write(b"never".to_vec()),
        ),
    ))));
    assert_eq!(app.terminal(ids.session).unwrap().input_status(), before);
    assert_eq!(into_recipes(app.sessions.subscription()).len(), 0);
    assert_eq!(app.model.sessions().len(), 1);
    shutdown(&mut app, &executor);
}

#[test]
fn canceled_prepared_success_is_disposed_without_attaching_or_replacing_retry() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    let (old, prepared) = launch(&mut app, &executor);
    let cleanup = prepared.shutdown_handle();
    let stale_target = app.active_target().unwrap();
    app.close(stale_target);
    let (current, ready) = launch(&mut app, &executor);
    drop(app.launched(current, Ok(ready)));
    drop(app.launched(old, Ok(prepared)));
    assert!(matches!(
        app.model.sessions().get(old.session).unwrap().lifecycle(),
        Lifecycle::Closing {
            awaiting_spawn: false
        }
    ));
    assert!(app.terminal(old.session).is_none());
    assert_eq!(app.active_session(), Some(current.session));
    drop(app.update(Message::Control(Control::CloseSession(old.session))));
    drop(app.launched(old, Err(LaunchError::WorkerFailed)));
    assert!(app.terminal(current.session).is_some());
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, cleanup.wait())
            .await
            .unwrap();
    });
    drop(app.update(Message::Disposed(old)));
    assert!(app.model.sessions().get(old.session).is_none());
    assert_eq!(app.model.sessions().len(), 1);
    shutdown(&mut app, &executor);
}

#[test]
fn queued_preparation_is_nonblocking_and_canceled_before_native_creation() {
    use std::sync::{Arc, Condvar, Mutex, mpsc};
    let executor = SharedExecutor::new().unwrap();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (started, observed) = mpsc::channel();
    let mut blockers = Vec::new();
    for _ in 0..2 {
        let gate = gate.clone();
        let started = started.clone();
        blockers.push(executor.enter(|| {
            tokio::task::spawn_blocking(move || {
                started.send(()).unwrap();
                let (lock, wake) = &*gate;
                let _guard = wake
                    .wait_timeout_while(lock.lock().unwrap(), DEADLINE, |open| !*open)
                    .unwrap();
            })
        }));
    }
    for _ in 0..2 {
        observed.recv_timeout(DEADLINE).unwrap();
    }
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    let request = executor.enter(|| app.begin_open()).unwrap();
    let ids = Reservation {
        session: request.session,
        request: request.request,
    };
    assert_eq!(
        app.model.sessions().get(ids.session).unwrap().lifecycle(),
        Lifecycle::Starting
    );
    drop(app.update(Message::WindowFocused(true)));
    assert!(app.window_focus.is_focused()); // Update remains available while both workers are blocked.
    app.close(app.active_target().unwrap());
    let (lock, wake) = &*gate;
    *lock.lock().unwrap() = true;
    wake.notify_all();
    for blocker in blockers {
        executor.block_on(blocker).unwrap();
    }
    let result = executor.block_on(request.receive());
    assert!(matches!(result, Err(LaunchError::Canceled)));
    drop(app.launched(ids, result));
    assert!(app.model.sessions().is_empty());
    assert!(app.sessions.entries.is_empty());
    shutdown(&mut app, &executor);
}

#[test]
fn all_live_entries_subscribe_and_background_exit_keeps_the_active_session() {
    let executor = SharedExecutor::new().unwrap();
    let script = if cfg!(windows) {
        "set /p PIKE_MARK=&echo PIKE23_BACKGROUND&exit /b 3"
    } else {
        "read pike_mark; printf PIKE23_BACKGROUND; exit 3"
    };
    let mut app = App {
        profile: profile(script),
        ..App::default()
    };
    let request = executor.enter(|| app.begin_open()).unwrap();
    let old = Reservation {
        session: request.session,
        request: request.request,
    };
    let first_target = app.active_target().unwrap();
    app.profile = waiting();
    let (_, Effect::Spawn(current)) = app.model.create_tab().unwrap() else {
        unreachable!()
    };
    let second = executor.enter(|| app.submit(current)).unwrap();
    let ready = executor.block_on(request.receive()).unwrap();
    drop(app.launched(old, Ok(ready)));
    let ready = executor.block_on(second.receive()).unwrap();
    drop(app.launched(current, Ok(ready)));
    assert!(!app.terminal(old.session).unwrap().is_visible());
    assert_eq!(
        app.terminal(old.session)
            .unwrap()
            .snapshot_stats()
            .snapshots,
        0
    );
    let mut events = iced::futures::stream::select_all(
        into_recipes(app.sessions.subscription())
            .into_iter()
            .map(|recipe| recipe.stream(stream::pending().boxed())),
    );
    assert_eq!(into_recipes(app.sessions.subscription()).len(), 2);
    let before = app.terminal(current.session).unwrap().input_status();
    let hidden_before = app.terminal(old.session).unwrap().input_status();
    drop(app.update(Message::Terminal((
        old.session,
        Event::BackendCall(
            old.session.get(),
            iced_term::BackendCommand::Write(b"hidden input".to_vec()),
        ),
    ))));
    drop(app.update(Message::Terminal((
        old.session,
        Event::BackendCall(
            current.session.get(),
            iced_term::BackendCommand::RequestShutdown,
        ),
    ))));
    assert_eq!(
        app.terminal(current.session).unwrap().input_status(),
        before
    );
    assert_eq!(
        app.terminal(old.session).unwrap().input_status(),
        hidden_before
    );
    // A hidden session must still receive its ordered protocol reply, even
    // though an obsolete widget's user input is ignored.
    let reply = if cfg!(windows) {
        "continue\r"
    } else {
        "continue\n"
    };
    drop(app.update(Message::Terminal((
        old.session,
        Event::BackendCall(
            old.session.get(),
            iced_term::BackendCommand::ProcessAlacrittyEvent(iced_term::AlacrittyEvent::PtyWrite(
                reply.into(),
            )),
        ),
    ))));
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(event) = events.next().await {
                drop(app.update(Message::Terminal(event)));
                if matches!(
                    app.model.sessions().get(old.session).unwrap().lifecycle(),
                    Lifecycle::Exited { .. }
                ) {
                    return;
                }
            }
            panic!("missing background completion");
        })
        .await
        .unwrap();
    });
    assert_eq!(app.active_session(), Some(current.session));
    assert_eq!(
        app.model
            .sessions()
            .get(current.session)
            .unwrap()
            .lifecycle(),
        Lifecycle::Running
    );
    assert_eq!(into_recipes(app.sessions.subscription()).len(), 1);
    app.model.focus(first_target).unwrap();
    app.synchronize();
    let text: String = app
        .terminal(old.session)
        .unwrap()
        .renderable_content()
        .unwrap()
        .viewport
        .cells()
        .iter()
        .map(|cell| cell.c)
        .collect();
    assert!(text.contains("PIKE23_BACKGROUND"));
    shutdown(&mut app, &executor);
}

#[test]
fn disposal_receipt_cannot_acknowledge_a_different_attached_owner() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    let (ids, ready) = launch(&mut app, &executor);
    drop(app.launched(ids, Ok(ready)));
    app.close(app.active_target().unwrap());
    drop(app.update(Message::Disposed(ids)));
    assert!(matches!(
        app.model.sessions().get(ids.session).unwrap().lifecycle(),
        Lifecycle::Closing { .. }
    ));
    assert!(app.terminal(ids.session).is_some());
    shutdown(&mut app, &executor);
}

#[test]
fn malformed_prepared_identity_fails_without_committing_a_running_entry() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    let (ids, mut ready) = launch(&mut app, &executor);
    let mut unrelated = Model::default();
    let (_, Effect::Spawn(other)) = unrelated.create_tab().unwrap() else {
        unreachable!()
    };
    ready.request = other.request;
    drop(app.launched(ids, Ok(ready)));
    assert_eq!(
        app.model.sessions().get(ids.session).unwrap().lifecycle(),
        Lifecycle::Failed(Failure::Worker)
    );
    assert!(app.terminal(ids.session).is_none());
    shutdown(&mut app, &executor);
}

#[test]
fn aggregate_shutdown_acknowledges_disposal_even_when_bridge_receipts_are_queued() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    let (ids, ready) = launch(&mut app, &executor);
    drop(app.launched(ids, Ok(ready)));
    let handle = app.terminal(ids.session).unwrap().shutdown_handle();
    assert_eq!(app.update(Message::ShutdownComplete).units(), 0);
    drop(app.update(Message::CloseWindow));
    confirm_close(&mut app);
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, app.launches.shutdown())
            .await
            .unwrap();
    });
    assert!(handle.completion().is_some());
    assert_eq!(app.model.sessions().len(), 1); // No bridge consumer has acknowledged its sticky completion yet.
    assert!(app.update(Message::ShutdownComplete).units() > 0);
    assert!(app.model.sessions().is_empty());
    assert!(app.sessions.entries.is_empty());
}

#[test]
fn already_exited_child_is_attached_read_only_without_late_focus_reports() {
    let executor = SharedExecutor::new().unwrap();
    let script = if cfg!(windows) {
        "echo \u{1b}[?1004hPIKE23_FOCUS_FINAL&exit /b 5"
    } else {
        "printf '\\033[?1004hPIKE23_FOCUS_FINAL'; exit 5"
    };
    let mut app = App {
        profile: profile(script),
        ..App::default()
    };
    drop(app.update(Message::WindowFocused(true)));
    let (ids, ready) = launch(&mut app, &executor);
    let handle = ready.shutdown_handle();
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap();
    });
    assert!(app.launched(ids, Ok(ready)).units() > 0); // UI Copy/selection focus still applies.
    let terminal = app.terminal(ids.session).unwrap();
    assert!(terminal.input_status().closed);
    assert!(!terminal.renderable_content().unwrap().focused);
    assert_eq!(terminal.input_status().outstanding_bytes, 0);
    let text: String = terminal
        .renderable_content()
        .unwrap()
        .viewport
        .cells()
        .iter()
        .map(|cell| cell.c)
        .collect();
    assert!(text.contains("PIKE23_FOCUS_FINAL"));
    shutdown(&mut app, &executor);
}

#[test]
fn input_failure_metadata_is_visible_only_on_its_origin_and_keeps_the_model() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    let (ids, ready) = launch(&mut app, &executor);
    drop(app.launched(ids, Ok(ready)));
    let mut unrelated = Model::default();
    let (_, Effect::Spawn(other)) = unrelated.create_tab().unwrap() else {
        unreachable!()
    };
    let error = iced_term::InputError::Full {
        requested: 7,
        available: 0,
    };
    let expected = format!("Input was not sent: {error}");
    drop(app.update(Message::Terminal((
        other.session,
        Event::BackendCall(
            ids.session.get(),
            iced_term::BackendCommand::InputRejected(error),
        ),
    ))));
    assert!(
        app.sessions
            .entries
            .get(&ids.session)
            .unwrap()
            .notice
            .is_none()
    );
    drop(app.update(Message::Terminal((
        ids.session,
        Event::BackendCall(
            ids.session.get(),
            iced_term::BackendCommand::InputRejected(error),
        ),
    ))));
    assert_eq!(
        app.sessions
            .entries
            .get(&ids.session)
            .unwrap()
            .notice
            .as_deref(),
        Some(expected.as_str())
    );
    assert!(app.terminal(ids.session).is_some());
    shutdown(&mut app, &executor);
}
