#[test]
fn reviewer2_stale_no_op_controls_preserve_pending_launch_focus() {
    let mut failures = Vec::new();
    for case in 0..6 {
        let (executor, mut app, _) = source();
        let original = app.active_target().unwrap();
        drop(app.update(Message::GridSize(original.tab, Size::new(1200.0, 800.0))));
        let (old_target, Effect::Spawn(old)) = app.model.create_tab().unwrap() else {
            panic!("expected stale owner reservation");
        };
        app.model.spawn_failed(old, Failure::Spawn);
        app.close(old_target);
        let pending = executor.enter(|| app.begin_split(original, Direction::Right)).unwrap();
        let pending_id = pending.session;
        assert!(app.focus_on_attach.contains(&pending_id));
        let (name, control) = match case {
            0 => ("closed session", Control::CloseSession(old.session)),
            1 => ("closed tab", Control::CloseTab(old_target.tab)),
            2 => ("stale focus", Control::Focus(old_target)),
            3 => ("stale activation", Control::ActivateTab(old_target.tab)),
            4 => ("stale retry", Control::Retry(old.session)),
            _ => ("stale default retry", Control::UseDefault(old.session)),
        };
        drop(app.update(Message::Control(control)));
        let retained = app.focus_on_attach.contains(&pending_id);
        receive(&mut app, &executor, pending);
        let selected = app.active_session() == Some(pending_id);
        shutdown(&mut app, &executor);
        if !retained || !selected {
            failures.push(format!("{name}: pending-intent-retained={retained}, attached-pane-selected={selected}"));
        }
    }
    assert!(failures.is_empty(), "stale no-op controls superseded a valid intent: {failures:?}");
}

#[test]
fn reviewer2_cancelled_close_preserves_pending_launch_intent() {
    let (executor, mut app, first) = source();
    let original = app.active_target().unwrap();
    drop(app.update(Message::GridSize(original.tab, Size::new(1200.0, 800.0))));
    let pending = executor.enter(|| app.begin_split(original, Direction::Right)).unwrap();
    let pending_id = pending.session;
    drop(app.update(Message::Control(Control::CloseSession(first.session))));
    let token = app.close_confirmation().unwrap().0;
    drop(app.update(Message::Control(Control::CloseAction(CloseAction::Cancel(token)))));
    let retained = app.focus_on_attach.contains(&pending_id);
    receive(&mut app, &executor, pending);
    let selected = app.active_session() == Some(pending_id);
    shutdown(&mut app, &executor);
    assert!(retained && selected, "cancelled close superseded pending launch focus");
}

#[test]
fn reviewer2_committed_close_supersedes_pending_launch_intent() {
    let (executor, mut app, _) = source();
    let original = app.active_target().unwrap();
    drop(app.update(Message::GridSize(original.tab, Size::new(1200.0, 800.0))));
    let sibling = executor.enter(|| app.begin_split(original, Direction::Right)).unwrap();
    let sibling = receive(&mut app, &executor, sibling);
    let pending = executor.enter(|| app.begin_split(original, Direction::Down)).unwrap();
    let pending_id = pending.session;
    drop(app.update(Message::Control(Control::CloseSession(sibling))));
    let token = app.close_confirmation().unwrap().0;
    drop(app.update(Message::Control(Control::CloseAction(CloseAction::Confirm(token)))));
    let cleared = !app.focus_on_attach.contains(&pending_id);
    receive(&mut app, &executor, pending);
    let selected = app.active_target() == Some(original);
    shutdown(&mut app, &executor);
    assert!(cleared && selected, "real committed close failed to supersede pending launch focus");
}

#[test]
fn reviewer2_empty_window_shutdown_still_completes() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App::default();
    drop(app.update(Message::CloseWindow));
    assert!(app.exiting);
    assert!(app.close_confirmation().is_none());
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, app.launches.shutdown()).await.unwrap();
    });
    assert!(app.launches.shutdown_complete());
    assert!(app.update(Message::ShutdownComplete).units() > 0);
}
