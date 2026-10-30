use super::*;
use crate::launch::LaunchRequest;
use crate::tabs::Activity;
use iced::Size;
use iced_term::{AlacrittyEvent, BackendCommand as B};

fn source() -> (SharedExecutor, App, Reservation) {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    drop(app.update(Message::WindowFocused(true)));
    let (ids, ready) = launch(&mut app, &executor);
    drop(app.launched(ids, Ok(ready)));
    (executor, app, ids)
}

fn receive(app: &mut App, executor: &SharedExecutor, request: LaunchRequest) -> SessionId {
    let ids = Reservation {
        session: request.session,
        request: request.request,
    };
    let ready = executor.block_on(async {
        tokio::time::timeout(DEADLINE, request.receive())
            .await
            .unwrap()
    });
    drop(app.launched(ids, ready));
    ids.session
}

#[test]
fn new_tab_inherits_frozen_source_profile_and_late_attachment_stays_hidden() {
    let (executor, mut app, first) = source();
    let source = app.active_target().unwrap();
    let inherited = app.profiles[&first.session].clone();
    app.profile.program = "/pike/synthetic/missing-default".into();
    let request = executor.enter(|| app.begin_new_tab()).unwrap();
    let second = request.session;
    assert_eq!(app.active_session(), Some(second));
    assert!(!app.terminal(first.session).unwrap().is_visible());
    let profile = &app.profiles[&second];
    assert_eq!(profile.program, inherited.program);
    assert_eq!(profile.args, inherited.args);
    assert_eq!(profile.env, inherited.env);
    assert_eq!(profile.working_directory, inherited.working_directory);
    assert!(profile.working_directory.as_ref().unwrap().is_absolute());
    drop(app.update(Message::Control(Control::ActivateTab(source.tab))));
    receive(&mut app, &executor, request);
    let hidden = app.terminal(second).unwrap();
    assert!(!hidden.is_visible());
    assert_eq!(hidden.snapshot_stats().snapshots, 0);
    assert_eq!(hidden.snapshot_stats().viewport_cells, 0);
    assert_eq!(app.active_session(), Some(first.session));
    assert!(
        app.terminal(first.session)
            .unwrap()
            .renderable_content()
            .unwrap()
            .focused
    );
    assert_eq!(into_recipes(app.sessions.subscription()).len(), 2);
    shutdown(&mut app, &executor);
}

#[test]
fn tab_switch_refreshes_once_and_rejects_hidden_geometry_input_and_stale_activity() {
    let (executor, mut app, first) = source();
    let first_tab = app.active_target().unwrap().tab;
    let request = executor.enter(|| app.begin_new_tab()).unwrap();
    let second = receive(&mut app, &executor, request);
    let second_tab = app.active_target().unwrap().tab;
    let before = app.terminal(first.session).unwrap().snapshot_stats();
    let work = app.terminal(first.session).unwrap().work_stats();
    let input = app.terminal(first.session).unwrap().input_status();
    for command in [
        B::Write(b"inactive".to_vec()),
        B::Resize(Some(Size::new(999.0, 333.0)), None),
        B::FocusChanged(true),
    ] {
        drop(app.update(Message::Terminal((
            first.session,
            Event::BackendCall(first.session.get(), command),
        ))));
    }
    // An invalid typed/raw pair must not generate a flag on either tab.
    drop(app.update(Message::Terminal((
        first.session,
        Event::BackendCall(second.get(), B::ProcessAlacrittyEvent(AlacrittyEvent::Bell)),
    ))));
    assert_eq!(
        app.model.workspace().tab(first_tab).unwrap().activity(),
        Activity::default()
    );
    for _ in 0..30 {
        for event in [AlacrittyEvent::Wakeup, AlacrittyEvent::Bell] {
            drop(app.update(Message::Terminal((
                first.session,
                Event::BackendCall(first.session.get(), B::ProcessAlacrittyEvent(event)),
            ))));
        }
    }
    let hidden = app.terminal(first.session).unwrap();
    assert_eq!(hidden.snapshot_stats().snapshots, before.snapshots);
    assert_eq!(hidden.snapshot_stats().viewport_cells, 0);
    assert_eq!(hidden.work_stats().grid_resizes, work.grid_resizes);
    assert_eq!(
        hidden.work_stats().cache_invalidations,
        work.cache_invalidations
    );
    assert_eq!(hidden.input_status(), input);
    assert_eq!(
        app.model.workspace().tab(first_tab).unwrap().activity(),
        Activity {
            unread: true,
            bell: true,
            exited: false
        }
    );
    drop(app.update(Message::Control(Control::ActivateTab(first_tab))));
    assert_eq!(
        app.terminal(first.session)
            .unwrap()
            .snapshot_stats()
            .snapshots,
        before.snapshots + 1
    );
    assert_eq!(
        app.model.workspace().tab(first_tab).unwrap().activity(),
        Activity::default()
    );
    let snapshots = app
        .terminal(first.session)
        .unwrap()
        .snapshot_stats()
        .snapshots;
    drop(app.update(Message::Control(Control::ActivateTab(first_tab))));
    assert_eq!(
        app.terminal(first.session)
            .unwrap()
            .snapshot_stats()
            .snapshots,
        snapshots
    );
    assert_eq!(app.active_session(), Some(first.session));
    assert!(!app.terminal(second).unwrap().is_visible());
    drop(app.update(Message::Control(Control::ActivateTab(second_tab))));
    assert_eq!(app.active_session(), Some(second));
    shutdown(&mut app, &executor);
}

#[test]
fn delayed_split_in_hidden_tab_cannot_steal_focus() {
    let (executor, mut app, first) = source();
    let target = app.active_target().unwrap();
    drop(app.update(Message::GridSize(target.tab, Size::new(960.0, 560.0))));
    let split = executor
        .enter(|| app.begin_split(target, Direction::Right))
        .unwrap();
    let pending = split.session;
    let tab = executor.enter(|| app.begin_new_tab()).unwrap();
    let active = receive(&mut app, &executor, tab);
    receive(&mut app, &executor, split);
    assert_eq!(app.active_session(), Some(active));
    assert!(!app.terminal(pending).unwrap().is_visible());
    assert_eq!(app.terminal(pending).unwrap().snapshot_stats().snapshots, 0);
    assert!(!app.focus_on_attach.contains(&pending));
    assert!(app.terminal(first.session).is_some());
    shutdown(&mut app, &executor);
}

#[test]
fn tab_admission_errors_are_retained_redacted_and_restartable_and_limit_is_atomic() {
    let (executor, mut app, first) = source();
    app.profiles.get_mut(&first.session).unwrap().program =
        "/pike/synthetic/secret-missing-shell".into();
    let request = executor.enter(|| app.begin_new_tab()).unwrap();
    let failed = receive(&mut app, &executor, request);
    let failed_target = app.active_target().unwrap();
    let Owner::Failed(error) = &app.sessions.entries[&failed].owner else {
        panic!("missing failure pane")
    };
    assert!(!error.to_string().contains("secret"));
    assert!(app.terminal(first.session).is_some());
    app.profiles.insert(failed, waiting());
    let retry = executor.enter(|| app.begin_retry(failed, false)).unwrap();
    let replacement = receive(&mut app, &executor, retry);
    assert_eq!(app.model.target(replacement), Some(failed_target));
    assert_ne!(failed, replacement);
    assert_eq!(app.active_session(), Some(replacement));
    for _ in app.model.sessions().len()..32 {
        app.model.create_tab().unwrap();
    }
    drop(app.update(Message::Control(Control::ActivateTab(failed_target.tab))));
    let order = app.model.workspace().order().to_vec();
    let active = app.model.workspace().active();
    assert!(executor.enter(|| app.begin_new_tab()).is_none());
    assert_eq!(app.model.workspace().order(), order);
    assert_eq!(app.model.workspace().active(), active);
    assert_eq!(app.model.sessions().len(), 32);
    assert!(app.notice.is_some_and(|s| s.contains("32-session")));
    shutdown(&mut app, &executor);
}

#[test]
fn modal_close_owns_new_tab_and_switch_and_last_tab_leaves_reopenable_empty_window() {
    let (executor, mut app, first) = source();
    let first_tab = app.active_target().unwrap().tab;
    let request = executor.enter(|| app.begin_new_tab()).unwrap();
    let second = receive(&mut app, &executor, request);
    drop(app.update(Message::Control(Control::CloseSession(second))));
    let token = app.close_confirmation().unwrap().0;
    for control in [Control::NewTab, Control::ActivateTab(first_tab)] {
        drop(app.update(Message::Control(control)));
    }
    assert_eq!(app.model.workspace().order().len(), 2);
    assert_eq!(app.active_session(), Some(second));
    drop(
        app.update(Message::Control(Control::CloseAction(CloseAction::Cancel(
            token,
        )))),
    );
    drop(app.update(Message::Control(Control::ActivateTab(first_tab))));
    assert_eq!(app.active_session(), Some(first.session));
    shutdown(&mut app, &executor);
    // Empty-window reopening is checked independently of window shutdown state.
    let mut empty = App::default();
    let (target, Effect::Spawn(ids)) = empty.model.create_tab().unwrap() else {
        panic!("expected launch")
    };
    empty.model.spawn_failed(ids, Failure::Spawn);
    empty.close(target);
    assert!(empty.model.workspace().order().is_empty());
    empty.profile = waiting();
    let request = executor.enter(|| empty.begin_new_tab()).unwrap();
    receive(&mut empty, &executor, request);
    assert!(empty.active_session().is_some());
    shutdown(&mut empty, &executor);
}
