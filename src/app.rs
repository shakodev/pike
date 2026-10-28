//! First-pane application integration; workspace transactions own identity.
use iced::{Subscription, Task};
use iced_term::settings::BackendSettings;
use iced_term::{Event, Terminal, WindowFocus};

use crate::identity::{Reservation, SessionId};
use crate::launch::{LaunchError, LaunchPool, PreparedSession};
use crate::layout::PaneTarget;
use crate::session::{Effect, Lifecycle};
use crate::terminal::{Owner, Sessions};
use crate::workspace::Model;

mod focus;
mod startup;
mod view;

pub enum Message {
    Control(Control),
    Launched(Reservation, Result<Box<PreparedSession>, LaunchError>),
    Terminal((SessionId, Event)),
    Disposed(Reservation),
    WindowFocused(bool),
    CloseWindow,
    ShutdownComplete,
}

#[derive(Clone, Copy)]
pub enum Control {
    New,
    Retry(SessionId),
    UseDefault(SessionId),
    Close(PaneTarget),
    Copy(SessionId),
}

#[derive(Default)]
pub struct App {
    model: Model,
    sessions: Sessions,
    launches: LaunchPool,
    profile: BackendSettings,
    window_focus: WindowFocus,
    exiting: bool,
    notice: Option<&'static str>,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        Self::with_settings(BackendSettings::default())
    }

    /// Structured launch configuration; no process-global environment mutation.
    pub fn with_settings(profile: BackendSettings) -> (Self, Task<Message>) {
        let mut app = Self {
            profile,
            ..Self::default()
        };
        let task = app.open();
        (app, task)
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    pub fn active_target(&self) -> Option<PaneTarget> {
        let tab = self.model.workspace().active()?;
        Some(PaneTarget {
            tab,
            pane: self.model.workspace().tab(tab)?.focused(),
        })
    }

    pub fn active_session(&self) -> Option<SessionId> {
        self.model
            .workspace()
            .pane(self.active_target()?)
            .map(|pane| pane.session)
    }

    pub fn terminal(&self, id: SessionId) -> Option<&Terminal> {
        self.sessions.entries.get(&id)?.terminal()
    }

    fn synchronize(&mut self) {
        let active = (!self.exiting).then(|| self.active_session()).flatten();
        self.sessions
            .synchronize(&self.model, active, &self.window_focus);
    }

    fn close(&mut self, target: PaneTarget) {
        let Some(id) = self.model.workspace().pane(target).map(|pane| pane.session) else {
            return;
        };
        if let Ok(effect) = self.model.close_pane(target) {
            if let Some(Effect::Close(reservation)) = effect {
                self.sessions.close(reservation);
            } else {
                self.sessions.entries.remove(&id);
            }
        }
        self.synchronize();
    }

    fn retry(&mut self, id: SessionId, use_default: bool) -> Task<Message> {
        if self.exiting
            || self.active_session() != Some(id)
            || !matches!(
                self.model.sessions().get(id).map(|e| e.lifecycle()),
                Some(Lifecycle::Failed(_) | Lifecycle::Exited { .. })
            )
        {
            return Task::none();
        }
        if let Some(target) = self.active_target() {
            self.close(target);
        }
        if use_default {
            self.profile = BackendSettings::default();
        }
        self.open()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Control(Control::New) => return self.open(),
            Message::Control(Control::Retry(id)) => return self.retry(id, false),
            Message::Control(Control::UseDefault(id)) => return self.retry(id, true),
            Message::Control(Control::Close(target)) => self.close(target),
            Message::Control(Control::Copy(id)) => {
                if !self.exiting
                    && self.active_session() == Some(id)
                    && let Some(terminal) = self.terminal(id)
                {
                    return terminal
                        .copy_selection(iced::clipboard::write)
                        .unwrap_or_else(Task::none);
                }
            }
            Message::Launched(reservation, result) => return self.launched(reservation, result),
            Message::Terminal((id, event)) => {
                self.sessions
                    .route(&mut self.model, &self.window_focus, id, event)
            }
            Message::Disposed(reservation) => {
                let disposed = self.sessions.entries.get(&reservation.session).is_some_and(|entry| {
                    entry.reservation == reservation && matches!(&entry.owner, Owner::Disposing(handle) if handle.completion().is_some())
                });
                if disposed
                    && self
                        .model
                        .sessions()
                        .get(reservation.session)
                        .is_some_and(|e| e.reservation() == reservation)
                    && self.model.closed(reservation.session)
                {
                    self.sessions.entries.remove(&reservation.session);
                }
            }
            Message::WindowFocused(focused) => {
                self.window_focus.set_focused(focused);
                self.synchronize();
            }
            Message::CloseWindow if !self.exiting => {
                self.exiting = true;
                for tab in self.model.workspace().order().to_vec() {
                    if let Ok(effects) = self.model.close_tab(tab) {
                        for effect in effects {
                            if let Effect::Close(reservation) = effect {
                                self.sessions.close(reservation);
                            }
                        }
                    }
                }
                self.sessions
                    .entries
                    .retain(|id, _| self.model.sessions().get(*id).is_some());
                self.synchronize();
                return Task::perform(self.launches.shutdown(), |_| Message::ShutdownComplete);
            }
            Message::CloseWindow => {}
            Message::ShutdownComplete if self.exiting => {
                // This receipt follows LaunchPool's aggregate native-disposal
                // acknowledgement; queued bridge/late-result receipts may lag.
                for id in self
                    .model
                    .sessions()
                    .iter()
                    .map(|(id, _)| id)
                    .collect::<Vec<_>>()
                {
                    self.model.closed(id);
                }
                self.sessions.entries.clear();
                return iced::exit();
            }
            Message::ShutdownComplete => {}
        }
        Task::none()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::window::close_requests().map(|_| Message::CloseWindow),
            iced::window::events().filter_map(|(_, event)| match event {
                iced::window::Event::Focused => Some(Message::WindowFocused(true)),
                iced::window::Event::Unfocused => Some(Message::WindowFocused(false)),
                _ => None,
            }),
            self.sessions.subscription().map(Message::Terminal),
        ])
    }
}

#[cfg(test)]
mod tests;
