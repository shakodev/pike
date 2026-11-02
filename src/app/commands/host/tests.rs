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
    assert!(app.notice.unwrap().contains("Commands menu"));
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

#[test]
fn current_reference_has_all_controls_effective_alternatives_and_disabled_context() {
    let mut app = App::default();
    app.window_focus.set_focused(true);
    app.model.create_tab().unwrap();
    let owner = app.active_session();
    let mut source = "version=1\n[[shortcuts]]\ncommand='new-tab'\nunbind=true\n".to_owned();
    for key in [
        "b", "d", "f", "g", "h", "i", "j", "k", "m", "n", "p", "s", "u", "x", "y", "z",
    ] {
        source.push_str(&format!(
            "[[shortcuts]]\ncommand='help'\nkey='{key}'\nmodifiers=['ctrl','shift']\n"
        ));
    }
    app.settings.pending = true;
    drop(app.settings_loaded(Ok(crate::settings::Settings::parse(&source).unwrap())));
    let reference = app.command_reference();
    assert_eq!(
        reference.len(),
        crate::settings::shortcuts::Command::ALL.len()
    );
    assert!(
        reference
            .iter()
            .find(|r| r.command == Command::NewTab)
            .unwrap()
            .bindings
            .is_empty()
    );
    let help = reference
        .iter()
        .find(|r| r.command == Command::Help)
        .unwrap();
    assert_eq!(help.bindings.len(), 16);
    assert!(!help.bindings.iter().any(|binding| binding == "F1"));
    assert!(
        !reference
            .iter()
            .find(|r| r.command == Command::Paste)
            .unwrap()
            .enabled
    );
    assert!(
        reference
            .iter()
            .any(|r| matches!(r.command, Command::MovePane(_)))
    );
    assert!(
        reference
            .iter()
            .any(|r| matches!(r.command, Command::MoveTab(_)))
    );
    assert!(reference.iter().any(|r| r.command == Command::UseDefault));
    command(&mut app, Command::Help);
    let token = app.command_state.help_token;
    drop(app.run_help_action(token, super::super::HelpAction::Last));
    assert_eq!(app.command_state.help_selected, reference.len() - 1);
    drop(app.run_help_action(
        token,
        super::super::HelpAction::Execute(Command::NewProfile(15)),
    ));
    assert_eq!(app.active_session(), owner);
    assert_eq!(app.model.sessions().len(), 1);
    assert!(app.interaction_blocked());
    drop(app.dismiss_help());
    command(&mut app, Command::Help);
    drop(app.run_help_action(
        token,
        super::super::HelpAction::Execute(Command::CloseWindow),
    ));
    assert!(
        !app.exiting,
        "delayed old menu action affected a newer opening"
    );
    assert!(app.interaction_blocked());
    drop(app.dismiss_help());
    assert_eq!(app.active_session(), owner);
}

#[test]
fn ambiguous_reload_keeps_the_entire_last_valid_settings_and_current_reference() {
    let mut app = App::default();
    let valid = crate::settings::Settings::parse("version=1\ntheme='dark'\n[[shortcuts]]\ncommand='new-tab'\nkey='k'\nmodifiers=['ctrl','shift']").unwrap();
    app.settings.pending = true;
    drop(app.settings_loaded(Ok(valid.clone())));
    let before = app.command_reference();
    let invalid = crate::settings::Settings::parse(
        "version=1\ntheme='light'\n[[shortcuts]]\ncommand='new-tab'\nkey='o'\nmodifiers=['ctrl','shift']",
    );
    assert!(invalid.is_err());
    app.settings.pending = true;
    drop(app.settings_loaded(invalid));
    assert_eq!(app.active_settings(), &valid);
    assert_eq!(app.command_reference(), before);
    assert!(
        app.settings_error()
            .unwrap()
            .to_string()
            .contains("conflicts")
    );
}

#[test]
fn reload_while_help_open_rejects_old_rows_and_refreshes_the_live_menu() {
    let mut app = App::default();
    app.window_focus.set_focused(true);
    app.model.create_tab().unwrap();
    let owner = app.active_session();
    app.synchronize();
    let old_generation = app.settings.generation;
    app.settings.pending = true;
    command(&mut app, Command::Help);
    let old_token = app.command_state.help_token;
    let settings = crate::settings::Settings::parse(
        "version=1\n[[shortcuts]]\ncommand='help'\nkey='k'\nmodifiers=['ctrl','shift']",
    )
    .unwrap();
    drop(app.settings_loaded(Ok(settings)));
    assert!(app.command_state.help.is_some());
    assert_ne!(app.command_state.help_token, old_token);
    assert_eq!(app.command_state.help_generation, app.settings.generation);
    drop(app.run_help_action(
        old_token,
        super::super::HelpAction::Execute(Command::CloseWindow),
    ));
    assert!(!app.exiting);
    assert!(app.command_state.help.is_some());
    let current = app.command_state.help_token;
    drop(app.run_help_action(
        current,
        super::super::HelpAction::Execute(Command::RenameTab),
    ));
    assert!(app.command_state.help.is_none());
    assert!(
        app.tab_dialog.active(),
        "newly rendered menu became trapped after reload"
    );
    let dialog = app.tab_dialog_token().unwrap();
    drop(app.update(Message::Control(Control::TabDialog(
        dialog,
        crate::app::DialogAction::Cancel,
    ))));
    drop(app.update(Message::Control(Control::MappedCommand(
        owner,
        old_generation,
        Command::ClosePane,
    ))));
    assert!(app.close_confirmation().is_none());
    assert_eq!(app.model.sessions().len(), 1);
    let current_generation = app.settings.generation;
    drop(app.update(Message::Control(Control::MappedCommand(
        owner,
        current_generation,
        Command::Help,
    ))));
    assert!(app.command_state.help.is_some());
    drop(app.dismiss_help());
    assert_eq!(app.active_session(), owner);
}

#[test]
fn held_help_actions_cannot_survive_native_or_app_blur_even_after_refocus() {
    for native_only in [false, true] {
        for roundtrip in [false, true] {
            let mut app = App::default();
            app.window_focus.set_focused(true);
            app.model.create_tab().unwrap();
            app.synchronize();
            let owner = app.active_session();
            command(&mut app, Command::Help);
            let held = app.command_state.help_token;
            if native_only {
                app.window_focus.set_focused(false);
            } else {
                drop(app.update(Message::WindowFocused(false)));
            }
            if roundtrip {
                if native_only {
                    app.window_focus.set_focused(true);
                } else {
                    drop(app.update(Message::WindowFocused(true)));
                }
            }
            drop(app.run_help_action(
                held,
                super::super::HelpAction::Execute(Command::CloseWindow),
            ));
            assert!(app.command_state.help.is_some());
            assert!(!app.exiting, "held help action survived focus loss");
            assert!(app.close_confirmation().is_none());
            drop(app.update(Message::WindowFocused(true)));
            let fresh = app.command_state.help_token;
            drop(app.update(Message::WindowFocused(true)));
            assert_eq!(
                app.command_state.help_token, fresh,
                "redundant focus retired a current menu"
            );
            drop(app.run_help_action(fresh, super::super::HelpAction::Last));
            assert_eq!(
                app.command_state.help_selected,
                crate::settings::shortcuts::Command::ALL.len() - 1
            );
            drop(app.dismiss_help());
            assert_eq!(app.active_session(), owner);
        }
    }
}
