use super::*;
use crate::executor::SharedExecutor;
use iced::futures::StreamExt;
use iced::{Executor, theme::Mode};
use iced_runtime::system::{RendererEvent, RendererFailure};
use std::time::Duration;

#[test]
fn readiness_preserves_window_theme_and_authority_and_initializes_once() {
    let executor = SharedExecutor::new().unwrap();
    let (mut app, registration) = App::boot_with_settings(BackendSettings {
        program: "/pike34/synthetic-missing".into(),
        ..Default::default()
    });
    drop(registration);
    assert!(app.model.sessions().is_empty());
    assert!(!app.settings.pending);
    assert!(app.boot.is_some());
    let window = app.window_focus.clone();
    let authority = app.focus_authority.clone();
    drop(app.update(Message::WindowFocused(true)));
    drop(app.update(Message::SystemTheme(Mode::Light)));
    assert!(window.is_focused());
    let task = executor.enter(|| app.update(Message::RendererEvent(RendererEvent::Ready)));
    assert!(app.boot.is_none());
    assert_eq!(app.settings.system, Mode::Light);
    assert!(app.window_focus.is_focused());
    let id = app.active_session().unwrap();
    app.synchronize();
    assert!(
        authority
            .request(id, iced::widget::Id::unique(), window.clone())
            .units()
            > 0
    );
    assert_eq!(
        app.update(Message::RendererEvent(RendererEvent::Ready))
            .units(),
        0
    );
    assert_eq!(app.model.sessions().len(), 1);
    drop(task);
    executor.block_on(async {
        tokio::time::timeout(Duration::from_secs(10), app.launches.shutdown())
            .await
            .unwrap();
    });
}

#[test]
fn close_before_readiness_never_reads_configuration_or_prepares_a_backend() {
    let (mut app, registration) = App::boot();
    drop(registration);
    drop(app.update(Message::CloseWindow));
    assert!(app.exiting);
    assert_eq!(
        app.update(Message::RendererEvent(RendererEvent::Ready))
            .units(),
        0
    );
    assert!(app.settings.path.is_none());
    assert!(!app.settings.pending);
    assert!(app.model.sessions().is_empty());
    assert!(app.launches.shutdown_complete());
}

#[test]
fn settings_read_receipt_after_renderer_shutdown_cannot_commit_or_launch() {
    let executor = SharedExecutor::new().unwrap();
    let path = std::env::temp_dir().join(format!(
        "pike34-read-{}-{}.toml",
        std::process::id(),
        crate::identity::Reservation::allocate()
            .unwrap()
            .session
            .get()
    ));
    std::fs::write(
        &path,
        "version=1\n[profiles.default]\nshell='pike34-synthetic-missing'\n",
    )
    .unwrap();
    let (mut app, registration) = App::boot_with_config_path(path.clone());
    drop(registration);
    let settings_task = app.update(Message::RendererEvent(RendererEvent::Ready));
    assert!(app.settings.pending);
    assert!(app.model.sessions().is_empty());
    drop(app.update(Message::RendererEvent(RendererEvent::Failed(
        RendererFailure::Other,
    ))));
    assert!(app.exiting);
    let mut stream = iced_runtime::task::into_stream(settings_task).unwrap();
    executor.block_on(async {
        tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(action) = stream.next().await {
                if let iced_runtime::Action::Output(Message::SettingsLoaded(result)) = action {
                    assert!(result.is_ok());
                    assert_eq!(app.update(Message::SettingsLoaded(result)).units(), 0);
                    return;
                }
            }
            panic!("missing actual settings read result")
        })
        .await
        .unwrap();
    });
    assert!(!app.settings.pending);
    assert!(app.model.sessions().is_empty());
    assert_eq!(app.settings.active, crate::settings::Settings::default());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn queued_pre_ready_controls_and_late_receipts_cannot_allocate_or_prepare() {
    use crate::app::{Control, Direction, commands::Command};
    let (mut app, registration) = App::boot();
    drop(registration);
    let mut unrelated = crate::workspace::Model::default();
    let (_, crate::session::Effect::Spawn(ids)) = unrelated.create_tab().unwrap() else {
        panic!("spawn")
    };
    let target = unrelated.target(ids.session).unwrap();
    for control in [
        Control::New,
        Control::NewTab,
        Control::NewProfile(0, 0),
        Control::Split(target, Direction::Right),
        Control::Retry(ids.session),
        Control::UseDefault(ids.session),
        Control::Command(Command::NewTab),
        Control::KeyCommand(None, Command::NewTab),
    ] {
        assert_eq!(app.update(Message::Control(control)).units(), 0);
    }
    assert!(app.begin_open().is_none());
    assert!(app.begin_split(target, Direction::Right).is_none());
    drop(app.update(Message::SettingsLoaded(Ok(
        crate::settings::Settings::default(),
    ))));
    drop(app.update(Message::Launched(
        ids,
        Err(crate::launch::LaunchError::WorkerFailed),
    )));
    assert!(app.model.sessions().is_empty());
    assert!(app.settings.path.is_none());
    assert!(!app.settings.pending);
    assert!(app.sessions.entries.is_empty());
    assert!(app.profiles.is_empty());
    drop(app.update(Message::CloseWindow));
    assert!(app.launches.shutdown_complete());
}
