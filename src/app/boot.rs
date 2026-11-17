//! Prepare no backend until the existing native renderer host owns its error
//! sink. Preserve the same App, WindowFocus and focus authority through readiness.
use super::{App, Message};
use iced::Task;
use iced_term::settings::BackendSettings;

pub(super) enum Source {
    Config(Option<std::path::PathBuf>),
    Profile,
}

#[cfg(test)]
mod tests;

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        Self::deferred(Source::Config(None), BackendSettings::default())
    }

    pub fn boot_with_config_path(path: std::path::PathBuf) -> (Self, Task<Message>) {
        Self::deferred(Source::Config(Some(path)), BackendSettings::default())
    }

    /// Native hosts/fixtures can use the same readiness gate with a controlled
    /// profile. Existing new/with_settings APIs retain their immediate semantics.
    pub fn boot_with_settings(profile: BackendSettings) -> (Self, Task<Message>) {
        Self::deferred(Source::Profile, profile)
    }

    fn deferred(source: Source, profile: BackendSettings) -> (Self, Task<Message>) {
        let mut app = Self {
            profile,
            boot: Some(source),
            ..Self::default()
        };
        app.settings.booting = true;
        (
            app,
            iced_runtime::system::renderer_errors().map(Message::RendererEvent),
        )
    }

    pub(super) fn renderer_ready(&mut self) -> Task<Message> {
        let Some(source) = self.boot.take() else {
            return Task::none();
        };
        if self.exiting {
            return Task::none();
        }
        self.settings.booting = false;
        let task = match source {
            Source::Profile => self.open(),
            Source::Config(path) => {
                self.settings.enabled = true;
                match path.map(Ok).unwrap_or_else(crate::settings::user_path) {
                    Ok(path) => {
                        self.settings.path = Some(path);
                        self.settings.booting = true;
                        self.reload_settings()
                    }
                    Err(error) => {
                        self.settings.error = Some(error);
                        self.report_error(
                            crate::errors::Owner::Application,
                            crate::errors::Error::Config(error),
                        );
                        self.open()
                    }
                }
            }
        };
        Task::batch([task, iced::system::theme().map(Message::SystemTheme)])
    }
}
