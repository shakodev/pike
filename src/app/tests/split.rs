use super::*;
use crate::launch::LaunchRequest;
use iced::Size;

fn source() -> (SharedExecutor, App, PaneTarget, SessionId) {
    source_with_limit(32)
}

fn source_with_limit(limit: u16) -> (SharedExecutor, App, PaneTarget, SessionId) {
    configured_source(limit, waiting())
}

fn configured_source(
    limit: u16,
    settings: BackendSettings,
) -> (SharedExecutor, App, PaneTarget, SessionId) {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        model: Model::new(std::num::NonZeroU16::new(limit).unwrap()),
        profile: settings,
        ..App::default()
    };
    drop(app.update(Message::WindowFocused(true)));
    let (ids, ready) = launch(&mut app, &executor);
    drop(app.launched(ids, Ok(ready)));
    let target = app.active_target().unwrap();
    drop(app.update(Message::GridSize(target.tab, Size::new(960.0, 560.0))));
    (executor, app, target, ids.session)
}

fn receive(app: &mut App, executor: &SharedExecutor, request: LaunchRequest) -> SessionId {
    let ids = Reservation {
        session: request.session,
        request: request.request,
    };
    let result = executor.block_on(async {
        tokio::time::timeout(DEADLINE, request.receive())
            .await
            .unwrap()
    });
    drop(app.launched(ids, result));
    ids.session
}

#[test]
fn split_reserves_before_launch_keeps_source_selected_and_focuses_on_attachment() {
    let (executor, mut app, target, id) = source();
    let original = app.terminal(id).unwrap().widget_id().clone();
    let before = app.terminal(id).unwrap().input_status();
    // A changed global default must not change an existing session's profile.
    app.profile.program = "/pike/missing-default".into();
    let request = executor
        .enter(|| app.begin_split(target, Direction::Right))
        .unwrap();
    let next = request.session;
    assert_ne!(next, id);
    assert_eq!(app.active_session(), Some(id));
    assert_eq!(
        app.model.sessions().get(next).unwrap().lifecycle(),
        Lifecycle::Starting
    );
    assert!(app.model.target(next).is_some());
    assert_eq!(receive(&mut app, &executor, request), next);
    assert_eq!(app.active_session(), Some(next));
    for session in [id, next] {
        assert!(app.terminal(session).unwrap().is_visible());
        assert_eq!(
            app.terminal(session)
                .unwrap()
                .renderable_content()
                .unwrap()
                .focused,
            session == next
        );
    }
    assert_eq!(app.terminal(id).unwrap().widget_id(), &original);
    assert_eq!(app.terminal(id).unwrap().input_status(), before);
    // Refocusing an older ID clears the newer input owner first.
    drop(app.update(Message::Control(Control::Focus(target))));
    assert!(
        app.terminal(id)
            .unwrap()
            .renderable_content()
            .unwrap()
            .focused
    );
    assert!(
        !app.terminal(next)
            .unwrap()
            .renderable_content()
            .unwrap()
            .focused
    );
    shutdown(&mut app, &executor);
}

#[test]
fn failed_split_can_retry_in_place_and_limit_or_geometry_refusal_is_actionable() {
    let (executor, mut app, target, id) = source();
    // Preserve the already-running owner while injecting a missing shell only
    // into its future launch profile.
    let profile = app.profiles.get_mut(&id).unwrap();
    profile.program = "/pike/synthetic/missing-secret-shell".into();
    let request = executor
        .enter(|| app.begin_split(target, Direction::Down))
        .unwrap();
    let failed = receive(&mut app, &executor, request);
    assert_eq!(app.active_session(), Some(id));
    assert!(matches!(
        app.sessions.entries[&failed].owner,
        Owner::Failed(_)
    ));
    let failed_target = app.model.target(failed).unwrap();
    let text = match &app.sessions.entries[&failed].owner {
        Owner::Failed(e) => e.to_string(),
        _ => unreachable!(),
    };
    assert!(!text.contains("secret"));
    app.profiles.insert(failed, waiting());
    let retry = executor.enter(|| app.begin_retry(failed, false)).unwrap();
    let replacement = app.model.workspace().pane(failed_target).unwrap().session;
    assert_ne!(replacement, failed);
    assert!(app.terminal(id).is_some());
    assert_eq!(app.model.workspace().pane(target).unwrap().session, id);
    assert_eq!(receive(&mut app, &executor, retry), replacement);
    assert_eq!(app.active_session(), Some(replacement));
    assert!(app.terminal(id).is_some());
    // A queued error-pane close names its old session, never the replacement
    // now occupying the same stable pane handle.
    drop(app.update(Message::Control(Control::CloseSession(failed))));
    assert!(app.terminal(replacement).is_some());
    assert_eq!(
        app.model.workspace().pane(failed_target).unwrap().session,
        replacement
    );
    app.close(failed_target);
    // A tiny current layout refuses before reservation.
    app.grid_size = Some((target.tab, Size::new(10.0, 10.0)));
    let count = app.model.sessions().len();
    assert!(
        executor
            .enter(|| app.begin_split(target, Direction::Right))
            .is_none()
    );
    assert_eq!(app.model.sessions().len(), count);
    assert!(app.notice.unwrap().contains("Enlarge"));
    shutdown(&mut app, &executor);
}

#[test]
fn host_session_limit_refusal_keeps_original_and_explains_recovery() {
    let (executor, mut app, target, id) = source_with_limit(1);
    assert!(
        executor
            .enter(|| app.begin_split(target, Direction::Right))
            .is_none()
    );
    assert_eq!(app.model.sessions().len(), 1);
    assert!(app.notice.unwrap().contains("Close a pane"));
    assert!(app.notice.unwrap().contains("cleanup"));
    assert_eq!(app.active_session(), Some(id));
    assert!(!app.terminal(id).unwrap().input_status().closed);
    shutdown(&mut app, &executor);
}

#[test]
fn host_geometry_uses_measured_cells_and_accounts_for_status_chrome() {
    let (executor, mut app, target, id) = source();
    let cell = app
        .terminal(id)
        .unwrap()
        .renderable_content()
        .unwrap()
        .terminal_size;
    let width = (20.0 * cell.cell_width + 2.0 * super::super::split::BORDER).ceil();
    let height =
        (5.0 * cell.cell_height + super::super::split::HEADER + 2.0 * super::super::split::BORDER)
            .ceil();
    app.grid_size = Some((
        target.tab,
        Size::new(2.0 * width + super::super::split::GUTTER, height),
    ));
    app.sessions.entries.get_mut(&id).unwrap().notice = Some("synthetic input notice".into());
    assert!(
        executor
            .enter(|| app.begin_split(target, Direction::Right))
            .is_none()
    );
    assert_eq!(app.model.sessions().len(), 1);
    assert!(app.notice.unwrap().contains("20×5"));
    app.sessions.entries.get_mut(&id).unwrap().notice = None;
    let request = executor
        .enter(|| app.begin_split(target, Direction::Right))
        .unwrap();
    receive(&mut app, &executor, request);
    assert_eq!(app.model.sessions().len(), 2);
    shutdown(&mut app, &executor);
}

#[test]
fn visible_sibling_stale_focus_and_keyboard_cannot_change_input_authority() {
    use iced_term::BackendCommand as B;
    let (executor, mut app, target, id) = source();
    let request = executor
        .enter(|| app.begin_split(target, Direction::Right))
        .unwrap();
    let next = receive(&mut app, &executor, request);
    let before = app.terminal(id).unwrap().input_status();
    drop(app.update(Message::Terminal((
        id,
        Event::BackendCall(id.get(), B::FocusChanged(true)),
    ))));
    let focused = app
        .terminal(id)
        .unwrap()
        .renderable_content()
        .unwrap()
        .focused;
    // Even a stale backend focus state cannot substitute for model authority.
    if let Owner::Attached(terminal) = &mut app.sessions.entries.get_mut(&id).unwrap().owner {
        terminal.set_focused(true);
    }
    let input = if cfg!(windows) {
        b"stale\r"
    } else {
        b"stale\n"
    };
    drop(app.update(Message::Terminal((
        id,
        Event::BackendCall(id.get(), B::Write(input.to_vec())),
    ))));
    let after = app.terminal(id).unwrap().input_status();
    assert_eq!(app.active_session(), Some(next));
    shutdown(&mut app, &executor);
    assert!(
        !focused,
        "a queued focus receipt selected an inactive sibling"
    );
    assert_eq!(before, after, "queued sibling input was admitted");
}

#[cfg(unix)]
#[test]
fn visible_sibling_hovered_mouse_and_alternate_wheel_do_not_require_or_steal_focus() {
    use iced::keyboard::Modifiers;
    use iced_term::{AlacrittyEvent, AlacrittyPoint, BackendCommand as B, MouseButton, TermMode};
    let (executor, mut app, target, id) = configured_source(
        32,
        profile("printf '\\033[?1049h\\033[?1007h\\033[?1000h\\033[?1006h'; read pike_wait"),
    );
    let request = executor
        .enter(|| app.begin_split(target, Direction::Right))
        .unwrap();
    let next = receive(&mut app, &executor, request);
    let mut events = iced::futures::stream::select_all(
        into_recipes(app.sessions.subscription())
            .into_iter()
            .map(|recipe| recipe.stream(stream::pending().boxed())),
    );
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while !app
                .terminal(id)
                .unwrap()
                .renderable_content()
                .unwrap()
                .terminal_mode
                .contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL | TermMode::SGR_MOUSE)
            {
                drop(app.update(Message::Terminal(events.next().await.unwrap())));
            }
        })
        .await
        .unwrap();
    });
    let admitted = |app: &App| {
        let status = app.terminal(id).unwrap().input_status();
        status.written_bytes + status.outstanding_bytes + status.cancelled_bytes
    };
    let before = admitted(&app);
    for command in [
        B::MouseReport(
            MouseButton::ScrollUp,
            Modifiers::empty(),
            AlacrittyPoint::default(),
            true,
        ),
        B::Scroll(1),
        B::Write(b"blocked\n".to_vec()),
    ] {
        drop(app.update(Message::Terminal((
            id,
            Event::BackendCall(id.get(), command),
        ))));
    }
    let bytes = admitted(&app) - before;
    let focused = app
        .terminal(id)
        .unwrap()
        .renderable_content()
        .unwrap()
        .focused;
    assert_eq!(app.active_session(), Some(next));
    // Finish the synthetic terminal line before cleanup. This is not a Unix
    // partial-line cancellation fixture.
    drop(app.update(Message::Terminal((
        id,
        Event::BackendCall(
            id.get(),
            B::ProcessAlacrittyEvent(AlacrittyEvent::PtyWrite("\n".into())),
        ),
    ))));
    shutdown(&mut app, &executor);
    assert_eq!(bytes, b"\x1b[<64;1;1M".len() + b"\x1bOA".len());
    assert!(!focused);
}

#[test]
#[ignore = "requires a local graphics device; run explicitly for visible sibling paste routing"]
fn real_widget_sibling_paste_cancels_held_clones_and_releases_pending_capacity() {
    use iced::advanced::renderer::Headless;
    use iced_term::BackendCommand as B;
    let (executor, mut app, target, id) = source();
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .expect("headless renderer");
    let token = super::paste::request(app.terminal(id).unwrap(), &renderer);
    let held = token.clone();
    let request = executor
        .enter(|| app.begin_split(target, Direction::Right))
        .unwrap();
    let next = receive(&mut app, &executor, request);
    let before = app.terminal(id).unwrap().input_status();
    // Model authority must still reject a queued token if stale backend state
    // claims focus after another pane was selected.
    if let Owner::Attached(terminal) = &mut app.sessions.entries.get_mut(&id).unwrap().owner {
        terminal.set_focused(true);
    }
    drop(app.update(Message::Terminal((
        id,
        Event::BackendCall(id.get(), B::Paste(token)),
    ))));
    if let Owner::Attached(terminal) = &mut app.sessions.entries.get_mut(&id).unwrap().owner {
        assert_eq!(
            terminal.handle(iced_term::Command::ProxyToBackend(B::Paste(held))),
            iced_term::actions::Action::Ignore
        );
        let reusable = super::paste::request(terminal, &renderer);
        reusable.cancel();
    }
    assert_eq!(app.terminal(id).unwrap().input_status(), before);
    assert_eq!(app.active_session(), Some(next));
    shutdown(&mut app, &executor);
}

#[test]
fn removing_a_prepared_split_disposes_only_its_owner_and_keeps_original_input() {
    let (executor, mut app, target, id) = source();
    let request = executor
        .enter(|| app.begin_split(target, Direction::Right))
        .unwrap();
    let ids = Reservation {
        session: request.session,
        request: request.request,
    };
    let ready = executor.block_on(request.receive()).unwrap();
    let handle = ready.shutdown_handle();
    let removed = app.model.target(ids.session).unwrap();
    app.close(removed);
    drop(app.launched(ids, Ok(ready)));
    assert!(app.terminal(ids.session).is_none());
    assert_eq!(app.active_session(), Some(id));
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap();
    });
    drop(app.update(Message::Disposed(ids)));
    assert!(app.model.sessions().get(ids.session).is_none());
    assert!(!app.profiles.contains_key(&ids.session));
    assert!(!app.terminal(id).unwrap().input_status().closed);
    assert_eq!(app.model.sessions().len(), 1);
    // Removed target cannot create a replacement or steal source identity.
    assert!(
        executor
            .enter(|| app.begin_split(removed, Direction::Down))
            .is_none()
    );
    assert!(app.notice.unwrap().contains("no longer available"));
    shutdown(&mut app, &executor);
}

#[test]
fn slow_queued_split_is_cancelable_while_the_original_remains_usable() {
    use std::sync::{Arc, Condvar, Mutex, mpsc};
    let (executor, mut app, target, id) = source();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (started, observed) = mpsc::channel();
    let mut blockers = vec![];
    for _ in 0..2 {
        let gate = gate.clone();
        let started = started.clone();
        blockers.push(executor.enter(|| {
            tokio::task::spawn_blocking(move || {
                started.send(()).unwrap();
                let (lock, wake) = &*gate;
                let _held = wake
                    .wait_timeout_while(lock.lock().unwrap(), DEADLINE, |open| !*open)
                    .unwrap();
            })
        }));
    }
    for _ in 0..2 {
        observed.recv_timeout(DEADLINE).unwrap();
    }
    let request = executor
        .enter(|| app.begin_split(target, Direction::Right))
        .unwrap();
    let ids = Reservation {
        session: request.session,
        request: request.request,
    };
    assert_eq!(app.active_session(), Some(id));
    assert!(!app.terminal(id).unwrap().input_status().closed);
    app.close(app.model.target(ids.session).unwrap());
    assert!(matches!(
        app.model.sessions().get(ids.session).unwrap().lifecycle(),
        Lifecycle::Closing {
            awaiting_spawn: true
        }
    ));
    let (lock, wake) = &*gate;
    *lock.lock().unwrap() = true;
    wake.notify_all();
    for blocker in blockers {
        executor.block_on(blocker).unwrap();
    }
    let result = executor.block_on(request.receive());
    assert!(matches!(result, Err(LaunchError::Canceled)));
    drop(app.launched(ids, result));
    assert_eq!(app.model.sessions().len(), 1);
    assert_eq!(app.active_session(), Some(id));
    assert!(!app.terminal(id).unwrap().input_status().closed);
    shutdown(&mut app, &executor);
}
