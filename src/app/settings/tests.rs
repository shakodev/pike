use super::*;
use crate::{
    app::{Control, close::Scope, commands::Command},
    session::Effect,
};

fn complete(app: &mut App, result: Result<Settings, Error>) {
    app.settings.pending = true;
    drop(app.settings_loaded(result));
}

#[test]
fn commit_is_whole_configuration_and_errors_or_unsolicited_receipts_cannot_change_it() {
    let mut app = App::default();
    let candidate = Settings::parse(
        "version = 1\nhistory = 17\ntheme = 'light'\nclose_policy = 'never'\n[font]\nsize = 18",
    )
    .unwrap();
    complete(&mut app, Ok(candidate.clone()));
    assert_eq!(app.active_settings(), &candidate);
    assert_eq!(app.theme(), iced::Theme::Light);
    complete(&mut app, Err(Error::Version));
    assert_eq!(app.active_settings(), &candidate);
    assert_eq!(app.settings_error(), Some(Error::Version));
    drop(app.update(Message::SettingsLoaded(Ok(Settings::default()))));
    assert_eq!(app.active_settings(), &candidate);
    assert!(!app.settings_pending());
}

#[test]
fn launch_lookup_reuses_profile_identity_with_current_settings_and_frozen_directory_fallback() {
    let mut app = App::default();
    app.settings.enabled = true;
    app.settings.active = Settings::parse("version = 1\ndefault_profile = 'a'\n[profiles.a]\nshell = 'first-shell'\nargs = ['first']\nenv = { PIKE31 = 'first' }").unwrap();
    let (initial, name) = app.launch_profile(None).unwrap();
    assert_eq!(initial.program, "first-shell");
    let (_, Effect::Spawn(reservation)) = app.model.create_tab().unwrap() else {
        panic!("spawn");
    };
    let frozen = std::env::current_dir().unwrap();
    app.profiles.insert(
        reservation.session,
        BackendSettings {
            working_directory: Some(frozen.clone()),
            ..initial
        },
    );
    app.settings
        .names
        .insert(reservation.session, name.unwrap());
    complete(
        &mut app,
        Settings::parse(
            "version = 1\ndefault_profile = 'b'\nhistory = 5\n[profiles.a]\nshell = 'second-shell'\nargs = ['second']\nenv = { PIKE31 = 'second' }\n[profiles.b]\nshell = 'other-shell'",
        ),
    );
    let (next, name) = app.launch_profile(Some(reservation.session)).unwrap();
    assert_eq!(name.unwrap().as_str(), "a");
    assert_eq!(next.program, "second-shell");
    assert_eq!(next.args, ["second"]);
    assert_eq!(next.env["PIKE31"], "second");
    assert_eq!(next.history.lines(), 5);
    assert_eq!(next.working_directory, Some(frozen.clone()));
    assert_eq!(app.profiles[&reservation.session].program, "first-shell");
    assert_eq!(
        app.profiles[&reservation.session].working_directory,
        Some(frozen)
    );
    assert_eq!(app.launch_profile(None).unwrap().0.program, "other-shell");
    complete(
        &mut app,
        Settings::parse("version = 1\ndefault_profile = 'b'\n[profiles.b]"),
    );
    let sessions = app.model.sessions().len();
    assert!(app.begin_new_tab().is_none());
    assert_eq!(app.settings_error(), Some(Error::MissingProfile));
    assert_eq!(app.model.sessions().len(), sessions);
}

#[test]
fn modal_and_repeat_gates_keep_reload_on_the_existing_command_path_and_reject_stale_profile_choices()
 {
    let mut app = App::default();
    app.settings.enabled = true;
    app.command_state.help = Some(None);
    drop(app.update(Message::Control(Control::Command(Command::ReloadSettings))));
    assert!(!app.settings_pending());
    assert_eq!(app.settings_error(), None);
    app.command_state.help = None;
    app.settings.generation = 2;
    drop(app.update(Message::Control(Control::NewProfile(1, 0))));
    drop(app.update(Message::Control(Control::NewProfile(2, u8::MAX))));
    assert!(app.model.sessions().is_empty());
}

#[test]
fn configured_close_policy_changes_confirmation_without_changing_the_cleanup_owner() {
    for confirm in [true, false] {
        let mut app = App::default();
        if !confirm {
            app.settings.active = Settings::parse("version = 1\nclose_policy = 'never'").unwrap();
        }
        let (tab, _) = app.model.create_tab().unwrap();
        drop(app.request_close(Scope::Tab(tab.tab)));
        assert_eq!(app.close_confirmation().is_some(), confirm);
        assert_eq!(app.model.workspace().order().is_empty(), !confirm);
    }
}
