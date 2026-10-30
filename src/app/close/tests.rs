use super::*;
use crate::session::Failure;
use iced::widget::pane_grid::Axis;

fn tab(app: &mut App) -> (PaneTarget, SessionId) {
    let (target, Effect::Spawn(ids)) = app.model.create_tab().unwrap() else {
        unreachable!()
    };
    (target, ids.session)
}

fn decision(app: &App) -> u64 {
    app.close_confirmation().unwrap().0
}

fn key(named: Named) -> keyboard::Event {
    keyboard::Event::KeyPressed {
        key: Key::Named(named),
        modified_key: Key::Named(named),
        physical_key: keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::empty(),
        text: None,
        repeat: false,
    }
}

#[test]
fn tab_confirmation_counts_starting_running_once_and_cancel_changes_nothing() {
    let mut app = App::default();
    let (a, first) = tab(&mut app);
    let (b, Effect::Spawn(second)) = app.model.split(a, Axis::Vertical).unwrap() else {
        unreachable!()
    };
    app.model.spawn_succeeded(second);
    let (other, unrelated) = tab(&mut app);
    let before = app.model.workspace().order().to_vec();
    drop(app.request_close(Scope::Tab(a.tab)));
    let token = decision(&app);
    assert_eq!(app.close_confirmation(), Some((token, 2)));
    drop(app.update(Message::CloseWindow));
    assert_eq!(decision(&app), token);
    drop(app.update(Message::Control(Control::CloseSession(unrelated))));
    drop(app.update(Message::Control(Control::New)));
    assert_eq!(app.model.sessions().len(), 3);
    assert_eq!(app.model.workspace().order(), before);
    drop(app.close_key(token, key(Named::Enter))); // Cancel is default.
    assert!(app.close_confirmation().is_none());
    assert!(!app.exiting);
    assert_eq!(app.model.workspace().order(), before);
    assert_eq!(app.active_target(), Some(other));
    assert_eq!(app.model.workspace().pane(a).unwrap().session, first);
    assert_eq!(
        app.model.workspace().pane(b).unwrap().session,
        second.session
    );
    assert_eq!(
        app.model.sessions().get(first).unwrap().lifecycle(),
        Lifecycle::Starting
    );
    assert_eq!(
        app.model
            .sessions()
            .get(second.session)
            .unwrap()
            .lifecycle(),
        Lifecycle::Running
    );
}

#[test]
fn confirm_resolves_the_original_sessions_after_moves_and_replays_do_nothing() {
    let mut app = App::default();
    let (a, first) = tab(&mut app);
    let (b, second) = tab(&mut app);
    drop(app.request_close(Scope::Pane(first)));
    let token = decision(&app);
    let moved = app.model.move_to_tab(a, b, Axis::Horizontal).unwrap();
    drop(app.close_action(CloseAction::Confirm(token)));
    assert!(app.model.workspace().pane(moved).is_none());
    assert_eq!(app.model.workspace().pane(b).unwrap().session, second);
    assert!(matches!(
        app.model.sessions().get(first).unwrap().lifecycle(),
        Lifecycle::Closing { .. }
    ));
    drop(app.close_action(CloseAction::Confirm(token)));
    drop(app.update(Message::Control(Control::CloseSession(first))));
    assert_eq!(app.model.workspace().pane(b).unwrap().session, second);
    drop(app.request_close(Scope::Pane(second)));
    let new = decision(&app);
    assert_ne!(token, new);
    drop(app.close_action(CloseAction::Cancel(token)));
    drop(app.close_action(CloseAction::Confirm(token)));
    assert_eq!(decision(&app), new);
    drop(app.close_action(CloseAction::Cancel(new)));
}

#[test]
fn exited_and_failed_close_without_prompt_last_tab_stays_empty_and_can_open_again() {
    let mut app = App::default();
    let (a, first) = tab(&mut app);
    let ids = app.model.sessions().get(first).unwrap().reservation();
    app.model.spawn_succeeded(ids);
    app.model.exited(first, Some(23));
    let (b, second) = tab(&mut app);
    let ids = app.model.sessions().get(second).unwrap().reservation();
    app.model.spawn_failed(ids, Failure::Spawn);
    drop(app.request_close(Scope::Pane(first)));
    assert!(app.model.workspace().tab(a.tab).is_none());
    assert_eq!(app.active_target(), Some(b));
    drop(app.request_close(Scope::Tab(b.tab)));
    assert!(app.model.sessions().is_empty());
    assert!(app.model.workspace().order().is_empty());
    assert!(app.active_session().is_none());
    assert!(!app.exiting);
    assert!(app.close_confirmation().is_none());
    let (_, next) = tab(&mut app);
    assert_ne!(first, next);
    assert_ne!(second, next);
}

#[test]
fn background_exit_during_confirmation_never_changes_focus_or_target_set() {
    let mut app = App::default();
    let (_, first) = tab(&mut app);
    let (_, second) = tab(&mut app);
    let ids = app.model.sessions().get(first).unwrap().reservation();
    app.model.spawn_succeeded(ids);
    drop(app.request_close(Scope::Pane(first)));
    let token = decision(&app);
    assert!(app.model.exited(first, Some(7)));
    assert_eq!(app.active_session(), Some(second));
    drop(app.close_action(CloseAction::Cancel(token)));
    assert_eq!(
        app.model.sessions().get(first).unwrap().lifecycle(),
        Lifecycle::Exited { code: Some(7) }
    );
    assert_eq!(app.active_session(), Some(second));
    assert_eq!(app.model.sessions().len(), 2);
}

#[test]
fn last_pane_confirm_retains_closing_budget_and_no_respawn() {
    let mut app = App::default();
    let (_, id) = tab(&mut app);
    drop(app.request_close(Scope::Pane(id)));
    let token = decision(&app);
    drop(app.close_action(CloseAction::Confirm(token)));
    assert!(app.model.workspace().order().is_empty());
    assert!(matches!(
        app.model.sessions().get(id).unwrap().lifecycle(),
        Lifecycle::Closing {
            awaiting_spawn: true
        }
    ));
    assert_eq!(app.model.sessions().len(), 1);
    drop(app.update(Message::Control(Control::CloseSession(id))));
    assert!(app.close_confirmation().is_none());
    assert_eq!(app.model.sessions().len(), 1);
}

#[test]
fn timeout_wait_force_and_stale_tokens_are_scoped_and_event_driven() {
    let mut app = App::default();
    // Synthetic timeout state: native gated cleanup is tested separately.
    drop(app.update(Message::CloseWindow));
    assert!(app.exiting);
    let first = app.close_policy.timer.unwrap();
    assert!(!app.close_policy.timed_out);
    assert_eq!(app.close_action(CloseAction::Force(first)).units(), 0);
    app.close_timeout(first + 1);
    assert!(!app.close_policy.timed_out);
    app.close_timeout(first);
    assert!(app.close_policy.timed_out);
    assert!(!app.close_policy.force_selected);
    assert!(app.close_view().is_some());
    drop(app.close_key(first, key(Named::Enter))); // Wait is default.
    let next = app.close_policy.timer.unwrap();
    assert_ne!(first, next);
    assert!(!app.close_policy.timed_out);
    app.close_timeout(first);
    assert!(!app.close_policy.timed_out);
    assert_eq!(app.close_action(CloseAction::Force(first)).units(), 0);
    app.close_timeout(next);
    assert!(app.close_action(CloseAction::Force(next)).units() > 0); // Never poll exit effect in-process.
    assert!(app.update(Message::CloseWindow).units() == 0);
    assert!(app.update(Message::Control(Control::New)).units() == 0);
}

#[test]
fn keyboard_selection_is_explicit_escape_cancels_and_repeats_cannot_confirm() {
    let mut app = App::default();
    let (_, id) = tab(&mut app);
    drop(app.request_close(Scope::Pane(id)));
    let token = decision(&app);
    drop(app.close_key(token, key(Named::Tab)));
    let mut repeated = key(Named::Enter);
    if let keyboard::Event::KeyPressed { repeat, .. } = &mut repeated {
        *repeat = true;
    }
    drop(app.close_key(token, repeated));
    assert_eq!(decision(&app), token);
    drop(app.close_key(token, key(Named::Escape)));
    assert!(app.close_confirmation().is_none());
    assert!(app.target_for(id).is_some());
}

#[test]
fn modal_drops_queued_user_commands_but_accepts_protocol_and_cleanup() {
    use iced_term::BackendCommand as B;
    let mut app = App::default();
    let (_, id) = tab(&mut app);
    for command in [
        B::Write(vec![1]),
        B::Scroll(1),
        B::FocusChanged(true),
        B::RequestShutdown,
    ] {
        assert!(!background_event(&Event::BackendCall(id.get(), command)));
    }
    assert!(background_event(&Event::BackendCall(
        id.get(),
        B::ProcessAlacrittyEvent(iced_term::AlacrittyEvent::Wakeup)
    )));
}

#[test]
#[ignore = "subprocess fixture; invoked only by force_exit_is_scoped_to_its_application"]
fn force_scope_fixture() {
    match std::env::var("PIKE28_FORCE_ROLE").as_deref() {
        Ok("force") => {
            use iced::Executor;
            let executor = crate::executor::SharedExecutor::new().unwrap();
            executor.block_on(force_application_exit());
            panic!("force returned");
        }
        Ok("witness") => {
            std::fs::write(std::env::var_os("PIKE28_FORCE_READY").unwrap(), "ready").unwrap();
            let mut release = String::new();
            std::io::stdin().read_line(&mut release).unwrap();
            assert_eq!(release, "release\n");
        }
        _ => panic!("fixture must be explicitly configured"),
    }
}

#[test]
fn force_exit_is_scoped_to_its_application() {
    use std::io::Write;
    use std::process::{Child, Command, Stdio};
    use std::time::Instant;
    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            // Only this fixture's owned process handle, never a name/PID search.
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }
    fn wait(child: &mut Child) -> std::process::ExitStatus {
        let began = Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status;
            }
            assert!(began.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(5)); // Finite subprocess oracle.
        }
    }
    let executable = std::env::current_exe().unwrap();
    let ready = std::env::temp_dir().join(format!("pike28-force-{}.ready", std::process::id()));
    let mut witness = OwnedChild(
        Command::new(&executable)
            .args([
                "--exact",
                "app::close::tests::force_scope_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("PIKE28_FORCE_ROLE", "witness")
            .env("PIKE28_FORCE_READY", &ready)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let began = Instant::now();
    while !ready.exists() {
        assert!(began.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    }
    let began = Instant::now();
    let mut forced = OwnedChild(
        Command::new(&executable)
            .args([
                "--exact",
                "app::close::tests::force_scope_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("PIKE28_FORCE_ROLE", "force")
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    assert!(wait(&mut forced.0).success());
    assert!(
        witness.0.try_wait().unwrap().is_none(),
        "same-executable witness was terminated"
    );
    witness
        .0
        .stdin
        .take()
        .unwrap()
        .write_all(b"release\n")
        .unwrap();
    assert!(wait(&mut witness.0).success());
    std::fs::remove_file(ready).unwrap();
    eprintln!(
        "pike28 native application-only force process_ms={}",
        began.elapsed().as_millis()
    );
}
