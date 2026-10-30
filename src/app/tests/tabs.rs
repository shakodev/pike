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

#[test]
fn switching_frozen_tabs_restores_bounded_presentation_and_prunes_closed_owners() {
    let (executor, mut app, first) = source();
    let first_tab = app.active_target().unwrap().tab;
    let size = Size::new(960.0, 600.0);
    drop(app.update(Message::GridSize(first_tab, size)));
    let request = executor.enter(|| app.begin_new_tab()).unwrap();
    let second = receive(&mut app, &executor, request);
    let second_tab = app.active_target().unwrap().tab;
    assert_eq!(app.resize_cache.len(), 1);
    drop(app.update(Message::Control(Control::ActivateTab(first_tab))));
    drop(app.update(Message::GridSize(first_tab, Size::new(20_000.0, 20_000.0))));
    assert!(app.resize_layout.as_ref().unwrap().frozen);
    drop(app.update(Message::Control(Control::ActivateTab(second_tab))));
    assert_eq!(app.resize_layout.as_ref().unwrap().size, size);
    assert!(app.resize_layout.as_ref().unwrap().frozen);
    let hidden = app.terminal(first.session).unwrap();
    let snapshots = hidden.snapshot_stats().snapshots;
    let work = hidden.work_stats();
    // Delayed sensors and hidden footer changes cannot fit the hidden tree.
    drop(app.update(Message::GridSize(first_tab, Size::new(300.0, 200.0))));
    drop(app.update(Message::Terminal((
        first.session,
        Event::BackendCall(
            first.session.get(),
            B::ImeRejected(iced_term::ImeError::WrongSession),
        ),
    ))));
    assert_eq!(
        app.terminal(first.session)
            .unwrap()
            .snapshot_stats()
            .snapshots,
        snapshots
    );
    assert_eq!(app.terminal(first.session).unwrap().work_stats(), work);
    assert_eq!(
        app.grid_size,
        Some((second_tab, Size::new(20_000.0, 20_000.0)))
    );
    drop(app.update(Message::Control(Control::ActivateTab(first_tab))));
    assert_eq!(app.resize_layout.as_ref().unwrap().tab, first_tab);
    assert_eq!(app.resize_layout.as_ref().unwrap().size, size);
    assert!(app.resize_layout.as_ref().unwrap().frozen);
    app.close(app.active_target().unwrap());
    assert_eq!(app.active_session(), Some(second));
    assert!(app.resize_cache.is_empty());
    assert_eq!(app.resize_layout.as_ref().unwrap().tab, second_tab);
    shutdown(&mut app, &executor);
}

#[test]
#[ignore = "requires native graphics; run explicitly for tab IME ownership"]
fn tab_switch_invalidates_deferred_ime_without_restoring_old_tokens() {
    use iced::advanced::renderer::Headless;
    let (executor, mut app, first) = source();
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .expect("native renderer");
    let first_tab = app.active_target().unwrap().tab;
    let token = super::ime::request(app.terminal(first.session).unwrap(), &renderer);
    let held = token.clone();
    let request = executor.enter(|| app.begin_new_tab()).unwrap();
    let second = receive(&mut app, &executor, request);
    drop(app.update(Message::Terminal((
        first.session,
        Event::BackendCall(first.session.get(), B::ImeCommit(token)),
    ))));
    drop(app.update(Message::Control(Control::ActivateTab(first_tab))));
    for token in [held.clone(), held] {
        drop(app.update(Message::Terminal((
            first.session,
            Event::BackendCall(first.session.get(), B::ImeCommit(token)),
        ))));
    }
    let admitted = |id| {
        let status = app.terminal(id).unwrap().input_status();
        status.written_bytes + status.outstanding_bytes + status.cancelled_bytes
    };
    assert_eq!(admitted(first.session), 0);
    assert_eq!(admitted(second), 0);
    let fresh = super::ime::request(app.terminal(first.session).unwrap(), &renderer);
    for token in [fresh.clone(), fresh] {
        drop(app.update(Message::Terminal((
            first.session,
            Event::BackendCall(first.session.get(), B::ImeCommit(token)),
        ))));
    }
    let status = app.terminal(first.session).unwrap().input_status();
    assert_eq!(
        status.written_bytes + status.outstanding_bytes + status.cancelled_bytes,
        "你好".len()
    );
    drop(app.update(Message::Terminal((
        first.session,
        Event::BackendCall(first.session.get(), B::Write(b"\n".to_vec())),
    ))));
    shutdown(&mut app, &executor);
}

// Reviewer-owned native App regression probes, included temporarily in tabs tests.
#[test]
fn late_split_round_trip_respects_newer_explicit_pane_choice() {
    for round_trip in [false, true] {
        let (executor, mut app, first) = source();
        let original = app.active_target().unwrap();
        drop(app.update(Message::GridSize(original.tab, Size::new(1200.0, 800.0))));
        let sibling = executor
            .enter(|| app.begin_split(original, Direction::Right))
            .unwrap();
        let sibling = receive(&mut app, &executor, sibling);
        let sibling_target = app.model.target(sibling).unwrap();
        let pending = executor
            .enter(|| app.begin_split(original, Direction::Down))
            .unwrap();
        let pending_id = pending.session;
        if round_trip {
            let other = executor.enter(|| app.begin_new_tab()).unwrap();
            receive(&mut app, &executor, other);
            drop(app.update(Message::Control(Control::ActivateTab(original.tab))));
        }
        drop(app.update(Message::Control(Control::Focus(sibling_target))));
        assert_eq!(app.active_session(), Some(sibling));
        receive(&mut app, &executor, pending);
        let actual = app.active_session();
        let focused = app
            .terminal(sibling)
            .unwrap()
            .renderable_content()
            .unwrap()
            .focused;
        let pending_focused = app
            .terminal(pending_id)
            .unwrap()
            .renderable_content()
            .unwrap()
            .focused;
        shutdown(&mut app, &executor);
        assert_eq!(
            actual,
            Some(sibling),
            "late split overrode explicit newer sibling focus; sibling focused={focused}, pending focused={pending_focused}"
        );
        assert!(app.launches.shutdown_complete());
        let _ = first;
    }
}

#[test]
fn hidden_close_prunes_cached_session_ids_before_reactivation() {
    let (executor, mut app, first) = source();
    let original = app.active_target().unwrap();
    drop(app.update(Message::GridSize(original.tab, Size::new(1200.0, 800.0))));
    let sibling = executor
        .enter(|| app.begin_split(original, Direction::Right))
        .unwrap();
    receive(&mut app, &executor, sibling);
    let other = executor.enter(|| app.begin_new_tab()).unwrap();
    let other = receive(&mut app, &executor, other);
    assert!(app.resize_cache.contains_key(&original.tab));
    app.close(original);
    let stale: Vec<_> = app.resize_cache[&original.tab]
        .grid
        .iter()
        .map(|(_, pane)| pane.session)
        .filter(|id| app.model.target(*id).is_none())
        .collect();
    assert_eq!(app.active_session(), Some(other));
    shutdown(&mut app, &executor);
    assert!(
        stale.is_empty(),
        "cached grid retains closed session IDs: {stale:?}, closed={:?}",
        first.session
    );
}

#[test]
fn hidden_restart_updates_cached_owner_and_late_result_respects_newer_selection() {
    let (executor, mut app, first) = source();
    let original = app.active_target().unwrap();
    drop(app.update(Message::GridSize(original.tab, Size::new(1200.0, 800.0))));
    app.profiles.get_mut(&first.session).unwrap().program = "/pike/synthetic/missing-shell".into();
    let request = executor
        .enter(|| app.begin_split(original, Direction::Right))
        .unwrap();
    let failed = receive(&mut app, &executor, request);
    assert!(matches!(
        app.model.sessions().get(failed).unwrap().lifecycle(),
        Lifecycle::Failed(_)
    ));
    // Match the real host's post-update presentation before hiding this tree.
    app.synchronize();
    let failed_target = app.model.target(failed).unwrap();
    app.profiles.insert(first.session, waiting());
    app.profiles.insert(failed, waiting());
    let other = executor.enter(|| app.begin_new_tab()).unwrap();
    let other = receive(&mut app, &executor, other);
    let before = app
        .terminal(first.session)
        .unwrap()
        .snapshot_stats()
        .snapshots;
    let canvas = app.resize_cache[&original.tab].size;
    let restart = executor.enter(|| app.begin_retry(failed, false)).unwrap();
    let replacement = restart.session;
    let cached = app.resize_cache[&original.tab]
        .grid
        .get(failed_target.pane)
        .unwrap()
        .session;
    let snapshots = app
        .terminal(first.session)
        .unwrap()
        .snapshot_stats()
        .snapshots;
    assert_eq!(app.active_session(), Some(other));
    assert_eq!(app.resize_cache[&original.tab].size, canvas);
    drop(app.update(Message::Control(Control::ActivateTab(original.tab))));
    drop(app.update(Message::Control(Control::Focus(original))));
    receive(&mut app, &executor, restart);
    let active = app.active_session();
    let focused = app
        .terminal(first.session)
        .unwrap()
        .renderable_content()
        .unwrap()
        .focused;
    shutdown(&mut app, &executor);
    assert_eq!(
        cached, replacement,
        "hidden restart retained the obsolete cache owner"
    );
    assert_ne!(cached, failed);
    assert_eq!(
        snapshots, before,
        "hidden cache reconciliation built a snapshot"
    );
    assert_eq!(
        active,
        Some(first.session),
        "late restart overrode a newer explicit selection"
    );
    assert!(focused);
}

fn failed_sibling() -> (SharedExecutor, App, SessionId, crate::layout::PaneTarget) {
    let (executor, mut app, first) = source();
    let original = app.active_target().unwrap();
    drop(app.update(Message::GridSize(original.tab, Size::new(1200.0, 800.0))));
    app.profiles.get_mut(&first.session).unwrap().program =
        "/pike/synthetic/reviewer2-missing".into();
    let failed = executor
        .enter(|| app.begin_split(original, Direction::Right))
        .unwrap();
    let failed = receive(&mut app, &executor, failed);
    assert!(matches!(
        app.model.sessions().get(failed).unwrap().lifecycle(),
        Lifecycle::Failed(_)
    ));
    // Mirror split()'s host synchronization, rather than testing an obsolete
    // one-pane cache which never contained the failed sibling.
    drop(app.update(Message::Control(Control::Focus(original))));
    app.profiles.insert(first.session, waiting());
    app.profiles.insert(failed, waiting());
    (executor, app, failed, original)
}

#[test]
fn retry_round_trip_respects_newer_explicit_focus() {
    let (executor, mut app, failed, original) = failed_sibling();
    let restart = executor.enter(|| app.begin_retry(failed, false)).unwrap();
    let replacement = restart.session;
    let other = executor.enter(|| app.begin_new_tab()).unwrap();
    receive(&mut app, &executor, other);
    drop(app.update(Message::Control(Control::ActivateTab(original.tab))));
    drop(app.update(Message::Control(Control::Focus(original))));
    let expected = app.active_session();
    receive(&mut app, &executor, restart);
    let actual = app.active_session();
    let focused = app
        .terminal(replacement)
        .unwrap()
        .renderable_content()
        .unwrap()
        .focused;
    shutdown(&mut app, &executor);
    assert_eq!(
        actual, expected,
        "late restart stole focus; replacement focused={focused}"
    );
}

#[test]
fn hidden_retry_replaces_cached_owner_without_geometry() {
    let (executor, mut app, failed, original) = failed_sibling();
    let other = executor.enter(|| app.begin_new_tab()).unwrap();
    let other = receive(&mut app, &executor, other);
    assert!(
        app.resize_cache[&original.tab]
            .grid
            .iter()
            .any(|(_, pane)| pane.session == failed)
    );
    let before = app
        .terminal(app.model.workspace().pane(original).unwrap().session)
        .unwrap()
        .work_stats();
    let restart = executor.enter(|| app.begin_retry(failed, false)).unwrap();
    let replacement = restart.session;
    receive(&mut app, &executor, restart);
    assert_eq!(app.active_session(), Some(other));
    assert!(!app.terminal(replacement).unwrap().is_visible());
    assert_eq!(
        app.terminal(replacement)
            .unwrap()
            .snapshot_stats()
            .snapshots,
        0
    );
    let stale: Vec<_> = app.resize_cache[&original.tab]
        .grid
        .iter()
        .map(|(_, pane)| pane.session)
        .filter(|id| app.model.target(*id).is_none())
        .collect();
    let after = app
        .terminal(app.model.workspace().pane(original).unwrap().session)
        .unwrap()
        .work_stats();
    shutdown(&mut app, &executor);
    assert_eq!(
        before, after,
        "hidden sibling geometry changed during restart"
    );
    assert!(
        stale.is_empty(),
        "cached grid retains pre-restart owner IDs: {stale:?}, old={failed:?}"
    );
}
