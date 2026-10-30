use super::*;

fn command(app: &mut App, command: Command) {
    drop(app.update(Message::Control(Control::Command(command))));
}

#[test]
fn missing_tab_is_typed_atomic_and_recovers_through_existing_activation() {
    let mut app = App::default();
    app.model.create_tab().unwrap();
    let owner = app.active_session();
    let target = app.active_target();
    command(&mut app, Command::SelectTab(TabSelection::Index(7)));
    assert_eq!(app.command_error(), Some(Error::MissingTab));
    assert_eq!(app.active_session(), owner);
    assert_eq!(app.active_target(), target);
    assert_eq!(app.model.sessions().len(), 1);
    assert!(app.notice.unwrap().contains("Ctrl+Shift+T"));
    command(&mut app, Command::SelectTab(TabSelection::Index(0)));
    assert_eq!(app.command_error(), None);
    assert_eq!(app.active_session(), owner);
    assert!(app.notice.is_none());
}

#[test]
fn delayed_key_cannot_close_a_newer_owner_or_bypass_help_priority() {
    let mut app = App::default();
    app.window_focus.set_focused(true);
    app.model.create_tab().unwrap();
    let old = app.active_session();
    app.model.create_tab().unwrap();
    let current = app.active_session();
    app.focus_on_attach.insert(old.unwrap());
    command(
        &mut app,
        Command::Focus(iced::widget::pane_grid::Direction::Left),
    );
    assert!(
        app.focus_on_attach.is_empty(),
        "accepted edge navigation retained older launch intent"
    );
    assert_eq!(app.active_session(), current);
    drop(app.update(Message::Control(Control::KeyCommand(
        old,
        Command::ClosePane,
    ))));
    assert!(app.close_confirmation().is_none());
    assert_eq!(app.model.sessions().len(), 2);
    command(&mut app, Command::Help);
    assert!(app.interaction_blocked());
    drop(app.update(Message::Control(Control::KeyCommand(
        current,
        Command::CloseTab,
    ))));
    assert!(app.close_confirmation().is_none());
    assert_eq!(app.model.sessions().len(), 2);
    drop(app.update(Message::Control(Control::DismissHelp)));
    assert!(!app.interaction_blocked());
    assert_eq!(app.active_session(), current);
    app.window_focus.set_focused(false);
    drop(app.update(Message::Control(Control::KeyCommand(
        current,
        Command::ClosePane,
    ))));
    assert!(app.close_confirmation().is_none());
}

#[test]
fn unavailable_terminal_and_empty_workspace_remain_usable_for_help() {
    let mut app = App::default();
    command(&mut app, Command::Font(FontChange::Larger));
    assert_eq!(app.command_error(), Some(Error::NoActivePane));
    command(&mut app, Command::Help);
    assert_eq!(app.command_error(), None);
    assert!(app.interaction_blocked());
    drop(app.update(Message::Control(Control::DismissHelp)));
    assert!(!app.interaction_blocked());
    app.model.create_tab().unwrap();
    let target = app.active_target();
    command(&mut app, Command::Font(FontChange::Larger));
    assert_eq!(app.command_error(), Some(Error::TerminalNotReady));
    assert_eq!(app.active_target(), target);
    assert_eq!(app.model.sessions().len(), 1);
}

#[test]
fn new_tab_capacity_refusal_uses_the_existing_guard_and_retains_current_owner() {
    let mut app = App::default();
    for _ in 0..32 {
        app.model.create_tab().unwrap();
    }
    let owner = app.active_session().unwrap();
    app.profiles.insert(owner, Default::default());
    let target = app.active_target();
    command(&mut app, Command::NewTab);
    assert_eq!(app.command_error(), Some(Error::NewTabRefused));
    assert_eq!(app.model.sessions().len(), 32);
    assert_eq!(app.active_session(), Some(owner));
    assert_eq!(app.active_target(), target);
    assert!(
        app.sessions.entries.is_empty(),
        "refusal launched a native owner"
    );
    assert!(app.notice.unwrap().contains("32-session limit"));
    command(&mut app, Command::SelectTab(TabSelection::Last));
    assert_eq!(app.command_error(), None);
    assert_eq!(app.active_session(), Some(owner));
}
