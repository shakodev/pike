//! Settings enter through one explicit read and commit as a whole on the UI
//! thread. Existing process owners and reservations never change during reload.
use super::{App, Message};
use crate::{
    identity::{Reservation, SessionId},
    launch::LaunchRequest,
    settings::{self, Error, ProfileId, Settings},
};
use iced::{Task, theme::Mode};
use iced_term::{
    BackendCommand, Command,
    settings::{BackendSettings, ThemeSettings},
};
use std::{collections::BTreeMap, path::PathBuf};

pub(super) struct State {
    pub(super) active: Settings,
    pub(super) path: Option<PathBuf>,
    pub(super) error: Option<Error>,
    pub(super) enabled: bool,
    pub(super) pending: bool,
    pub(super) booting: bool,
    pub(super) generation: u64,
    pub(super) system: Mode,
    pub(super) names: BTreeMap<SessionId, ProfileId>,
}

#[cfg(test)]
mod tests;

impl Default for State {
    fn default() -> Self {
        Self {
            active: Settings::default(),
            path: None,
            error: None,
            enabled: false,
            pending: false,
            booting: false,
            generation: 0,
            system: Mode::Dark,
            names: BTreeMap::new(),
        }
    }
}

impl App {
    /// Explicit path injection for embedders/fixtures; production uses user_path.
    pub fn with_config_path(path: PathBuf) -> (Self, Task<Message>) {
        let mut app = Self::default();
        app.settings.enabled = true;
        app.settings.path = Some(path);
        app.settings.booting = true;
        let task = app.reload_settings();
        (
            app,
            Task::batch([task, iced::system::theme().map(Message::SystemTheme)]),
        )
    }

    pub fn active_settings(&self) -> &Settings {
        &self.settings.active
    }
    pub fn settings_error(&self) -> Option<Error> {
        self.settings.error
    }
    pub fn settings_pending(&self) -> bool {
        self.settings.pending
    }

    pub fn theme(&self) -> iced::Theme {
        if self.settings.active.theme().is_light(self.settings.system) {
            iced::Theme::Light
        } else {
            iced::Theme::Dark
        }
    }

    pub(super) fn reload_settings(&mut self) -> Task<Message> {
        if self.settings.pending || self.exiting {
            return Task::none();
        }
        let Some(path) = self.settings.path.clone() else {
            self.settings.error = Some(Error::Location);
            return self.focus_active();
        };
        self.settings.pending = true;
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || settings::load(&path))
                    .await
                    .unwrap_or(Err(Error::Io {
                        operation: settings::Operation::Read,
                        kind: std::io::ErrorKind::Other,
                        code: None,
                    }))
            },
            Message::SettingsLoaded,
        )
    }

    pub(super) fn settings_loaded(&mut self, result: Result<Settings, Error>) -> Task<Message> {
        if !self.settings.pending {
            return Task::none();
        }
        self.settings.pending = false;
        if self.exiting {
            return Task::none();
        }
        match result {
            Ok(candidate) => {
                // Profile controls from older views cannot select a new profile
                // at the same list index after a reload.
                if let Some(generation) = self.settings.generation.checked_add(1) {
                    self.settings.generation = generation;
                    self.settings.active = candidate;
                    self.settings.error = None;
                    self.apply_appearance();
                } else {
                    self.notice = Some(
                        "Settings reload identifiers exhausted. Restart Pike when convenient.",
                    );
                }
            }
            Err(error) => self.settings.error = Some(error),
        }
        if std::mem::take(&mut self.settings.booting) {
            self.open()
        } else {
            self.focus_active()
        }
    }

    pub(super) fn system_theme(&mut self, mode: Mode) {
        if self.settings.system != mode {
            self.settings.system = mode;
            if self.settings.active.theme() == settings::Theme::System {
                self.apply_palette();
            }
        }
    }

    pub(super) fn palette(&self) -> ThemeSettings {
        ThemeSettings::new(self.settings.active.theme().palette(self.settings.system))
    }

    fn apply_palette(&mut self) {
        let palette = self.palette().color_pallete;
        for entry in self.sessions.entries.values_mut() {
            if let Some(terminal) = entry.terminal_mut() {
                terminal.handle(Command::ChangeTheme(palette.clone()));
            }
        }
    }

    fn apply_appearance(&mut self) {
        let font = self.settings.active.font().terminal();
        let history = self.settings.active.history();
        let palette = self.palette().color_pallete;
        // A successful reload resets temporary per-pane zoom to configured
        // font. Pending/hidden/exited attachments use the same active values.
        self.command_state.fonts.clear();
        for entry in self.sessions.entries.values_mut() {
            if let Some(terminal) = entry.terminal_mut() {
                terminal.handle(Command::ChangeFont(font.clone()));
                terminal.handle(Command::ChangeTheme(palette.clone()));
                terminal.handle(Command::ProxyToBackend(BackendCommand::SetHistoryLimit(
                    history,
                )));
            }
        }
        self.refresh_resize_layout();
    }

    /// Resolve a retained profile identity against the current configuration.
    /// Explicit current cwd wins; otherwise inherit the source's frozen launch
    /// directory. Removed profiles are recoverable errors, never silent fallback.
    pub(super) fn launch_profile(
        &mut self,
        source: Option<SessionId>,
    ) -> Option<(BackendSettings, Option<ProfileId>)> {
        let name = if self.settings.enabled {
            match source {
                Some(id) => self.settings.names.get(&id).cloned(),
                None => Some(self.settings.active.default_profile().clone()),
            }
        } else {
            None
        };
        let mut launch = if let Some(name) = &name {
            match self.settings.active.launch(name) {
                Ok(launch) => launch,
                Err(error) => {
                    self.settings.error = Some(error);
                    return None;
                }
            }
        } else if let Some(id) = source {
            let Some(profile) = self.profiles.get(&id).cloned() else {
                self.notice = Some("Wait for the source terminal to start, then try again.");
                return None;
            };
            profile
        } else {
            self.profile.clone()
        };
        if self.settings.enabled {
            launch.history = self.settings.active.history();
        }
        if launch.working_directory.is_none() {
            launch.working_directory = source
                .and_then(|id| self.profiles.get(&id))
                .and_then(|p| p.working_directory.clone());
        }
        Some((launch, name))
    }

    pub(super) fn submit_named_profile(
        &mut self,
        reservation: Reservation,
        profile: BackendSettings,
        name: Option<ProfileId>,
    ) -> Option<LaunchRequest> {
        if let Some(name) = name {
            self.settings.names.insert(reservation.session, name);
        }
        self.submit_profile(reservation, profile)
    }

    pub(super) fn new_profile(&mut self, generation: u64, index: u8) -> Task<Message> {
        if !self.settings.enabled || self.settings.pending || generation != self.settings.generation
        {
            return Task::none();
        }
        let Some(name) = self
            .settings
            .active
            .profiles()
            .nth(usize::from(index))
            .cloned()
        else {
            return Task::none();
        };
        let profile = match self.settings.active.launch(&name) {
            Ok(profile) => profile,
            Err(error) => {
                self.settings.error = Some(error);
                return self.focus_active();
            }
        };
        let Some(reservation) = self.create_tab_reservation() else {
            return self.focus_active();
        };
        let task = self
            .submit_named_profile(reservation, profile, Some(name))
            .map(Self::receive)
            .unwrap_or_else(Task::none);
        self.synchronize();
        Task::batch([task, self.focus_active()])
    }
}
