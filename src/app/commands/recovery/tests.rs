use super::*;
use crate::app::commands::Command;

fn menu(app: &mut App) -> u64 {
    app.window_focus.set_focused(true);
    drop(app.run_command(Command::Help));
    app.command_state.help_token
}

#[test]
fn recovery_rows_cannot_retarget_after_eviction_or_survive_reopening_or_blur() {
    let mut app = App::default();
    app.report_error(
        Owner::Application,
        Error::Config(crate::settings::Error::Location),
    );
    let id = app.latest_error(Owner::Application).unwrap().id;
    let token = menu(&mut app);
    let held = HelpAction::Recovery(id, Action::Dismiss);
    assert!(app.command_state.help_actions.contains(&held));
    app.errors.dismiss(id);
    app.report_error(Owner::Application, Error::Cleanup);
    let replacement = app.latest_error(Owner::Application).unwrap().id;
    assert_ne!(id, replacement);
    drop(app.run_help_action(token, held));
    assert!(app.command_state.help.is_some());
    assert!(app.errors.get(replacement).is_some());
    drop(app.dismiss_help());
    let token = menu(&mut app);
    let action = HelpAction::Recovery(replacement, Action::Dismiss);
    app.window_focus.set_focused(false);
    app.window_focus.set_focused(true);
    drop(app.run_help_action(token, action));
    assert!(app.errors.get(replacement).is_some());
    drop(app.dismiss_help());
    let fresh = menu(&mut app);
    drop(app.run_help_action(token, action));
    assert!(app.errors.get(replacement).is_some());
    drop(app.run_help_action(fresh, action));
    assert!(app.errors.is_empty());
    assert!(app.command_state.help.is_none());
}

#[test]
fn menu_tab_traversal_wraps_and_error_actions_are_bounded_and_opt_in() {
    let mut app = App::default();
    app.set_diagnostics_enabled(true);
    app.report_error(
        Owner::Application,
        Error::Config(crate::settings::Error::Location),
    );
    let token = menu(&mut app);
    let count = app.command_state.help_actions.len();
    assert!(
        count
            <= crate::settings::shortcuts::Command::ALL.len() + crate::errors::NOTICE_LIMIT * 3 + 2
    );
    drop(app.run_help_action(token, HelpAction::Previous));
    assert_eq!(app.command_state.help_selected, count - 1);
    drop(app.run_help_action(token, HelpAction::Next));
    assert_eq!(app.command_state.help_selected, 0);
    drop(app.run_help_action(token, HelpAction::ClearDiagnostics));
    assert!(app.diagnostic_capture().lines().count() <= 2);
    assert!(app.command_state.help.is_none());
    app.set_diagnostics_enabled(false);
    menu(&mut app);
    assert!(
        !app.command_state
            .help_actions
            .contains(&HelpAction::CopyDiagnostics)
    );
    assert!(
        !app.command_state
            .help_actions
            .contains(&HelpAction::ClearDiagnostics)
    );
}
