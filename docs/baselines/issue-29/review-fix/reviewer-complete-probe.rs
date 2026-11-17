// Reviewer-owned native App regression probes, included temporarily in tabs tests.
#[test]
fn reviewer2_late_split_after_tab_round_trip_respects_newer_pane_focus() {
    let (executor, mut app, first) = source();
    let original = app.active_target().unwrap();
    drop(app.update(Message::GridSize(original.tab, Size::new(1200.0, 800.0))));
    let sibling = executor.enter(|| app.begin_split(original, Direction::Right)).unwrap();
    let sibling = receive(&mut app, &executor, sibling);
    let sibling_target = app.model.target(sibling).unwrap();
    let pending = executor.enter(|| app.begin_split(original, Direction::Down)).unwrap();
    let pending_id = pending.session;
    let other = executor.enter(|| app.begin_new_tab()).unwrap();
    receive(&mut app, &executor, other);
    drop(app.update(Message::Control(Control::ActivateTab(original.tab))));
    drop(app.update(Message::Control(Control::Focus(sibling_target))));
    assert_eq!(app.active_session(), Some(sibling));
    receive(&mut app, &executor, pending);
    let actual = app.active_session();
    let focused = app.terminal(sibling).unwrap().renderable_content().unwrap().focused;
    let pending_focused = app.terminal(pending_id).unwrap().renderable_content().unwrap().focused;
    shutdown(&mut app, &executor);
    assert_eq!(actual, Some(sibling), "late split overrode explicit newer sibling focus; sibling focused={focused}, pending focused={pending_focused}");
    assert!(app.launches.shutdown_complete());
    let _ = first;
}

#[test]
fn reviewer2_hidden_close_prunes_cached_session_ids() {
    let (executor, mut app, first) = source();
    let original = app.active_target().unwrap();
    drop(app.update(Message::GridSize(original.tab, Size::new(1200.0, 800.0))));
    let sibling = executor.enter(|| app.begin_split(original, Direction::Right)).unwrap();
    receive(&mut app, &executor, sibling);
    let other = executor.enter(|| app.begin_new_tab()).unwrap();
    let other = receive(&mut app, &executor, other);
    assert!(app.resize_cache.contains_key(&original.tab));
    app.close(original);
    let stale: Vec<_> = app.resize_cache[&original.tab].grid.iter()
        .map(|(_, pane)| pane.session)
        .filter(|id| app.model.target(*id).is_none())
        .collect();
    assert_eq!(app.active_session(), Some(other));
    shutdown(&mut app, &executor);
    assert!(stale.is_empty(), "cached grid retains closed session IDs: {stale:?}, closed={:?}", first.session);
}

fn reviewer2_failed_sibling() -> (SharedExecutor, App, SessionId, crate::layout::PaneTarget) {
    let (executor, mut app, first) = source();
    let original = app.active_target().unwrap();
    drop(app.update(Message::GridSize(original.tab, Size::new(1200.0, 800.0))));
    app.profiles.get_mut(&first.session).unwrap().program = "/pike/synthetic/reviewer2-missing".into();
    let failed = executor.enter(|| app.begin_split(original, Direction::Right)).unwrap();
    let failed = receive(&mut app, &executor, failed);
    assert!(matches!(app.model.sessions().get(failed).unwrap().lifecycle(), Lifecycle::Failed(_)));
    // Mirror split()'s host synchronization, rather than testing an obsolete
    // one-pane cache which never contained the failed sibling.
    drop(app.update(Message::Control(Control::Focus(original))));
    app.profiles.insert(first.session, waiting());
    app.profiles.insert(failed, waiting());
    (executor, app, failed, original)
}

#[test]
fn reviewer2_restart_after_tab_round_trip_respects_newer_focus() {
    let (executor, mut app, failed, original) = reviewer2_failed_sibling();
    let restart = executor.enter(|| app.begin_retry(failed, false)).unwrap();
    let replacement = restart.session;
    let other = executor.enter(|| app.begin_new_tab()).unwrap();
    receive(&mut app, &executor, other);
    drop(app.update(Message::Control(Control::ActivateTab(original.tab))));
    drop(app.update(Message::Control(Control::Focus(original))));
    let expected = app.active_session();
    receive(&mut app, &executor, restart);
    let actual = app.active_session();
    let focused = app.terminal(replacement).unwrap().renderable_content().unwrap().focused;
    shutdown(&mut app, &executor);
    assert_eq!(actual, expected, "late restart stole focus; replacement focused={focused}");
}

#[test]
fn reviewer2_restart_hidden_cache_replaces_old_owner_without_geometry() {
    let (executor, mut app, failed, original) = reviewer2_failed_sibling();
    let other = executor.enter(|| app.begin_new_tab()).unwrap();
    let other = receive(&mut app, &executor, other);
    assert!(app.resize_cache[&original.tab].grid.iter().any(|(_, pane)| pane.session == failed));
    let before = app.terminal(app.model.workspace().pane(original).unwrap().session).unwrap().work_stats();
    let restart = executor.enter(|| app.begin_retry(failed, false)).unwrap();
    let replacement = restart.session;
    receive(&mut app, &executor, restart);
    assert_eq!(app.active_session(), Some(other));
    assert!(!app.terminal(replacement).unwrap().is_visible());
    assert_eq!(app.terminal(replacement).unwrap().snapshot_stats().snapshots, 0);
    let stale: Vec<_> = app.resize_cache[&original.tab].grid.iter()
        .map(|(_, pane)| pane.session)
        .filter(|id| app.model.target(*id).is_none())
        .collect();
    let after = app.terminal(app.model.workspace().pane(original).unwrap().session).unwrap().work_stats();
    shutdown(&mut app, &executor);
    assert_eq!(before, after, "hidden sibling geometry changed during restart");
    assert!(stale.is_empty(), "cached grid retains pre-restart owner IDs: {stale:?}, old={failed:?}");
}
