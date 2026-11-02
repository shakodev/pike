//! Close transactions capture stable owners before asking; UI locations may change.
use std::time::Duration;

use iced::keyboard::{self, Key, key::Named};
use iced::{Subscription, Task};
use iced_term::{BackendCommand, Event};

use super::{App, Control, Message};
use crate::identity::{SessionId, TabId};
use crate::layout::PaneTarget;
use crate::session::{Effect, Lifecycle};

mod view;
pub(super) use view::{close_button, close_tab_button};

pub(super) const DEADLINE: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug)]
pub enum CloseAction {
    Cancel(u64),
    Confirm(u64),
    Wait(u64),
    Force(u64),
}

#[derive(Clone, Copy)]
pub(super) enum Scope {
    Pane(SessionId),
    Tab(TabId),
    Window,
}

pub(super) struct Confirmation {
    token: u64,
    sessions: Vec<SessionId>,
    running: usize,
    window: bool,
    restore: Option<SessionId>,
    affirmative: bool,
}

#[derive(Default)]
pub(super) struct Policy {
    next: u64,
    confirmation: Option<Confirmation>,
    pub(super) timer: Option<u64>,
    pub(super) timed_out: bool,
    force_selected: bool,
}

impl Policy {
    fn token(&mut self) -> Option<u64> {
        self.next = self.next.checked_add(1)?;
        Some(self.next)
    }

    pub(super) fn confirming(&self) -> bool {
        self.confirmation.is_some()
    }
}

impl App {
    /// Modal decisions and shutdown own user interaction; native protocol and
    /// lifecycle delivery continue. Capture wrappers must defer to this state.
    pub fn interaction_blocked(&self) -> bool {
        self.exiting
            || self.settings.booting
            || self.close_policy.confirming()
            || self.command_state.help.is_some()
            || self.tab_dialog.active()
    }

    /// Stable decision token and targeted live count, also useful to native
    /// hosts driving the same controls without inspecting terminal contents.
    pub fn close_confirmation(&self) -> Option<(u64, usize)> {
        self.close_policy
            .confirmation
            .as_ref()
            .map(|c| (c.token, c.running))
    }

    /// Close controls carry a session identity, never a recyclable pane location.
    pub(super) fn request_close(&mut self, scope: Scope) -> Task<Message> {
        if self.exiting || self.close_policy.confirming() {
            return Task::none();
        }
        let sessions: Vec<_> = match scope {
            Scope::Pane(id) => self.target_for(id).map(|_| vec![id]).unwrap_or_default(),
            Scope::Tab(tab) => self
                .model
                .workspace()
                .tab(tab)
                .map_or_else(Vec::new, |tab| {
                    tab.grid().iter().map(|(_, pane)| pane.session).collect()
                }),
            Scope::Window => self.model.sessions().iter().map(|(id, _)| id).collect(),
        };
        let running = sessions
            .iter()
            .filter(|id| {
                matches!(
                    self.model.sessions().get(**id).map(|e| e.lifecycle()),
                    Some(Lifecycle::Starting | Lifecycle::Running)
                )
            })
            .count();
        let window = matches!(scope, Scope::Window);
        if running == 0
            || self.settings.active.close_policy() == crate::settings::ClosePolicy::Never
        {
            return self.commit_close(sessions, window);
        }
        let Some(token) = self.close_policy.token() else {
            self.notice =
                Some("Close request identifiers exhausted. Restart Pike when convenient.");
            return Task::none();
        };
        self.close_policy.confirmation = Some(Confirmation {
            token,
            sessions,
            running,
            window,
            restore: self.active_session(),
            affirmative: false,
        });
        self.synchronize();
        Task::none()
    }

    pub(super) fn target_for(&self, session: SessionId) -> Option<PaneTarget> {
        self.model.target(session)
    }

    pub(super) fn close(&mut self, target: PaneTarget) {
        let Some(id) = self.model.workspace().pane(target).map(|pane| pane.session) else {
            return;
        };
        if let Ok(effect) = self.model.close_pane(target) {
            if let Some(Effect::Close(reservation)) = effect {
                self.sessions.close(reservation);
            } else if self.model.sessions().get(id).is_none() {
                self.sessions.entries.remove(&id);
            }
        }
        self.synchronize();
    }

    fn commit_close(&mut self, sessions: Vec<SessionId>, window: bool) -> Task<Message> {
        // A stale pane/tab scope has no authority over a newer launch choice.
        // Window close still owns pool cleanup even after every pane is gone.
        if sessions.is_empty() && !window {
            return Task::none();
        }
        self.focus_on_attach.clear();
        if window {
            // Admission stops before signaling any worker. Pool shutdown also
            // owns earlier removed panes, queued prepare, and undelivered results.
            self.exiting = true;
        }
        for id in sessions {
            if let Some(target) = self.target_for(id) {
                self.close(target);
            }
        }
        self.synchronize();
        if !window {
            return self.restore_close_focus(None);
        }
        Task::batch([
            Task::perform(self.launches.shutdown(), |_| Message::ShutdownComplete),
            self.arm_close_timer(),
        ])
    }

    fn arm_close_timer(&mut self) -> Task<Message> {
        let Some(token) = self.close_policy.token() else {
            // Exhaustion must not turn pending cleanup into a normal exit.
            self.close_policy.timed_out = true;
            return Task::none();
        };
        self.close_policy.timer = Some(token);
        Task::perform(wait_deadline(token), Message::CloseTimeout)
    }

    pub(super) fn close_timeout(&mut self, token: u64) {
        if self.exiting && self.close_policy.timer == Some(token) {
            self.close_policy.timed_out = true;
            self.close_policy.force_selected = false;
        }
    }

    pub(super) fn close_action(&mut self, action: CloseAction) -> Task<Message> {
        match action {
            CloseAction::Cancel(token) | CloseAction::Confirm(token)
                if self
                    .close_policy
                    .confirmation
                    .as_ref()
                    .is_some_and(|c| c.token == token) =>
            {
                let confirmation = self.close_policy.confirmation.take().unwrap();
                if matches!(action, CloseAction::Confirm(_)) {
                    self.commit_close(confirmation.sessions, confirmation.window)
                } else {
                    self.synchronize();
                    self.restore_close_focus(confirmation.restore)
                }
            }
            CloseAction::Wait(token)
                if self.exiting
                    && self.close_policy.timed_out
                    && self.close_policy.timer == Some(token) =>
            {
                self.close_policy.timed_out = false;
                self.close_policy.force_selected = false;
                self.arm_close_timer()
            }
            CloseAction::Force(token)
                if self.exiting
                    && self.close_policy.timed_out
                    && self.close_policy.timer == Some(token) =>
            {
                // Explicit application-only escape hatch. iced::exit alone can
                // join a stuck blocking prepare during Runtime::drop. Exiting
                // this process lets the OS close its owned handles, without a
                // process-name search or touching unrelated applications.
                Task::perform(force_application_exit(), |_| Message::ShutdownComplete)
            }
            _ => Task::none(),
        }
    }

    fn restore_close_focus(&mut self, restore: Option<SessionId>) -> Task<Message> {
        if let Some(target) = restore.and_then(|id| self.target_for(id)) {
            return self.select(target);
        }
        self.focus_active()
    }

    pub(super) fn shutdown_complete(&mut self) -> Task<Message> {
        if !self.exiting || !self.launches.shutdown_complete() {
            return Task::none();
        }
        // Pool permits return only after native disposal, including prepared
        // owners whose typed UI receipts are still queued. No timeout reaches here.
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
        iced::exit()
    }

    pub(super) fn close_subscription(&self) -> Subscription<Message> {
        let token = self
            .close_policy
            .confirmation
            .as_ref()
            .map(|c| c.token)
            .or_else(|| {
                self.close_policy
                    .timed_out
                    .then_some(self.close_policy.timer)
                    .flatten()
            });
        token.map_or_else(Subscription::none, |token| {
            keyboard::listen()
                .with(token)
                .map(|(token, event)| Message::CloseKey(token, event))
        })
    }

    pub(super) fn close_key(&mut self, token: u64, event: keyboard::Event) -> Task<Message> {
        let keyboard::Event::KeyPressed {
            key: Key::Named(key),
            repeat: false,
            ..
        } = event
        else {
            return Task::none();
        };
        let action = if let Some(c) = self
            .close_policy
            .confirmation
            .as_mut()
            .filter(|c| c.token == token)
        {
            match key {
                Named::Escape => Some(CloseAction::Cancel(token)),
                Named::Enter | Named::Space => Some(if c.affirmative {
                    CloseAction::Confirm(token)
                } else {
                    CloseAction::Cancel(token)
                }),
                Named::Tab | Named::ArrowLeft | Named::ArrowRight => {
                    c.affirmative = !c.affirmative;
                    None
                }
                _ => None,
            }
        } else if self.exiting
            && self.close_policy.timed_out
            && self.close_policy.timer == Some(token)
        {
            match key {
                Named::Escape => Some(CloseAction::Wait(token)),
                Named::Enter | Named::Space => Some(if self.close_policy.force_selected {
                    CloseAction::Force(token)
                } else {
                    CloseAction::Wait(token)
                }),
                Named::Tab | Named::ArrowLeft | Named::ArrowRight => {
                    self.close_policy.force_selected = !self.close_policy.force_selected;
                    None
                }
                _ => None,
            }
        } else {
            None
        };
        action
            .map(|action| self.close_action(action))
            .unwrap_or_else(Task::none)
    }
}

pub(super) async fn wait_deadline(token: u64) -> u64 {
    tokio::time::sleep(DEADLINE).await;
    token
}

async fn force_application_exit() {
    std::process::exit(0)
}

/// Modal input cannot reach terminal widgets, including events already queued
/// by the old view. Protocol replies, output and cleanup continue to flow.
pub(super) fn background_event(event: &Event) -> bool {
    let Event::BackendCall(_, command) = event;
    if let BackendCommand::Paste(request) = command {
        request.cancel();
    }
    if let BackendCommand::ImeCommit(commit) = command {
        commit.cancel();
    }
    matches!(
        command,
        BackendCommand::ProcessAlacrittyEvent(_)
            | BackendCommand::EventBridge(_)
            | BackendCommand::InputRejected(_)
            | BackendCommand::PasteRejected(_)
            | BackendCommand::ImeRejected(_)
    )
}

#[cfg(test)]
mod tests;
