//! Application integration; workspace transactions own pane/session identity.
use iced::{Size, Subscription, Task};
use iced_term::settings::BackendSettings;
use iced_term::{Event, Terminal, WindowFocus};
use std::collections::{BTreeMap, BTreeSet};

use crate::identity::{Reservation, SessionId, TabId};
use crate::launch::{LaunchError, LaunchPool, LaunchRequest, PreparedSession};
use crate::layout::PaneTarget;
use crate::session::{Effect, Lifecycle};
use crate::terminal::{Owner, Sessions};
use crate::workspace::Model;

mod focus;
mod resize;
mod split;
mod startup;
mod view;
pub use resize::ResizeStep;
pub use split::Direction;

pub enum Message {
    Control(Control),
    Launched(Reservation, Result<Box<PreparedSession>, LaunchError>),
    Terminal((SessionId, Event)),
    Disposed(Reservation),
    WindowFocused(bool),
    CloseWindow,
    ShutdownComplete,
    GridSize(TabId, Size),
}

#[derive(Clone, Copy)]
pub enum Control {
    New,
    Retry(SessionId),
    UseDefault(SessionId),
    Close(PaneTarget),
    CloseSession(SessionId),
    Copy(SessionId),
    Split(PaneTarget, Direction),
    Focus(PaneTarget),
    Resize(TabId, iced::widget::pane_grid::ResizeEvent),
    ResizeStep(ResizeStep),
    ResizeKey(u64, ResizeStep),
    ResizeFocus(u64),
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
    profiles: BTreeMap<SessionId, BackendSettings>,
    focus_on_attach: BTreeSet<SessionId>,
    grid_size: Option<(TabId, Size)>,
    resize_layout: Option<resize::Presentation>,
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
        self.profiles
            .retain(|id, _| self.model.sessions().get(*id).is_some());
        self.focus_on_attach.retain(|id| {
            self.model
                .sessions()
                .get(*id)
                .is_some_and(|e| self.model.accepts_spawn(e.reservation()))
        });
        let active = (!self.exiting).then(|| self.active_session()).flatten();
        self.sessions
            .synchronize(&self.model, active, &self.window_focus);
        self.refresh_resize_layout();
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
        self.begin_retry(id, use_default)
            .map(Self::receive)
            .unwrap_or_else(Task::none)
    }

    fn begin_retry(&mut self, id: SessionId, use_default: bool) -> Option<LaunchRequest> {
        if self.exiting
            || !matches!(
                self.model.sessions().get(id).map(|e| e.lifecycle()),
                Some(Lifecycle::Failed(_) | Lifecycle::Exited { .. })
            )
        {
            return None;
        }
        let target = self.model.target(id)?;
        let profile = if use_default {
            BackendSettings {
                working_directory: self
                    .profiles
                    .get(&id)
                    .and_then(|p| p.working_directory.clone()),
                ..BackendSettings::default()
            }
        } else {
            self.profiles
                .get(&id)
                .cloned()
                .unwrap_or_else(|| self.profile.clone())
        };
        let Ok(Effect::Spawn(reservation)) = self.model.restart(target) else {
            self.notice = Some("Cannot restart this terminal. Close the error pane and try again.");
            return None;
        };
        self.sessions.entries.remove(&id);
        self.profiles.remove(&id);
        self.focus_on_attach.insert(reservation.session);
        self.submit_profile(reservation, profile)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Control(Control::New) => return self.open(),
            Message::Control(Control::Retry(id)) => return self.retry(id, false),
            Message::Control(Control::UseDefault(id)) => return self.retry(id, true),
            Message::Control(Control::Close(target)) => {
                self.close(target);
                return self.focus_active();
            }
            Message::Control(Control::CloseSession(id)) => {
                if let Some(target) = self.model.target(id) {
                    self.close(target);
                    return self.focus_active();
                }
            }
            Message::Control(Control::Split(target, direction)) => {
                return self.split(target, direction);
            }
            Message::Control(Control::Focus(target)) => return self.select(target),
            Message::Control(Control::Resize(tab, event)) => {
                if self.resize_divider(tab, event) {
                    return self.focus_active();
                }
            }
            Message::Control(Control::ResizeStep(step)) => return self.resize_step(step),
            Message::Control(Control::ResizeKey(origin, step)) => {
                if self.active_session().is_some_and(|id| id.get() == origin) {
                    return self.resize_step(step);
                }
            }
            Message::Control(Control::ResizeFocus(origin)) => {
                if !self.exiting
                    && self.window_focus.is_focused()
                    && self.active_session().is_some_and(|id| id.get() == origin)
                    && let Some(target) = self.active_target()
                {
                    return self.select(target);
                }
            }
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
                if self.resize_layout.as_ref().is_some_and(|p| p.frozen)
                    && matches!(
                        &event,
                        Event::BackendCall(_, iced_term::BackendCommand::Resize(..))
                    )
                {
                    return Task::none();
                }
                let footer = self.footer_height(id);
                self.sessions
                    .route(&mut self.model, &self.window_focus, id, event);
                if footer != self.footer_height(id) {
                    self.refresh_resize_layout();
                }
                if self.model.sessions().get(id).is_none() {
                    self.profiles.remove(&id);
                    self.focus_on_attach.remove(&id);
                }
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
                    self.profiles.remove(&reservation.session);
                    self.focus_on_attach.remove(&reservation.session);
                }
            }
            Message::WindowFocused(focused) => {
                self.window_focus.set_focused(focused);
                self.synchronize();
            }
            Message::GridSize(tab, size) => {
                if self.model.workspace().active() == Some(tab) && resize::valid_area(size) {
                    self.grid_size = Some((tab, size));
                    self.refresh_resize_layout();
                }
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
                self.profiles.clear();
                self.focus_on_attach.clear();
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
