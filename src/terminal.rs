//! UI-thread backend owners, keyed by the model's existing stable identities.
use std::collections::BTreeMap;

use iced::Subscription;
use iced_term::actions::Action;
use iced_term::{BackendCommand, BackendCompletion, Event, ShutdownHandle, Terminal, WindowFocus};

use crate::identity::{Reservation, SessionId};
use crate::launch::{CancelLaunch, LaunchError};
use crate::session::Lifecycle;
use crate::workspace::Model;

pub(crate) enum Owner {
    Starting(CancelLaunch),
    Attached(Box<Terminal>),
    Disposing(ShutdownHandle),
    Failed(LaunchError),
}

pub(crate) struct Entry {
    pub reservation: Reservation,
    pub owner: Owner,
    pub ended: bool,
    pub completion: Option<BackendCompletion>,
    pub notice: Option<String>,
}

impl Entry {
    pub fn new(reservation: Reservation, owner: Owner) -> Self {
        Self {
            reservation,
            owner,
            ended: false,
            completion: None,
            notice: None,
        }
    }

    pub fn terminal(&self) -> Option<&Terminal> {
        match &self.owner {
            Owner::Attached(terminal) => Some(terminal),
            _ => None,
        }
    }
}

#[derive(Default)]
pub(crate) struct Sessions {
    pub entries: BTreeMap<SessionId, Entry>,
}

impl Sessions {
    /// Completion is recorded only after its reliable bridge event is handled;
    /// checking the watch here could unsubscribe before final queued events.
    pub fn subscription(&self) -> Subscription<(SessionId, Event)> {
        Subscription::batch(self.entries.iter().filter_map(|(id, entry)| {
            entry
                .terminal()
                .filter(|_| entry.completion.is_none())
                .map(|terminal| terminal.subscription().with(*id))
        }))
    }

    pub fn close(&mut self, reservation: Reservation) {
        let Some(entry) = self
            .entries
            .get_mut(&reservation.session)
            .filter(|entry| entry.reservation == reservation)
        else {
            return;
        };
        match &mut entry.owner {
            Owner::Starting(cancel) => cancel.cancel(),
            Owner::Attached(terminal) => {
                terminal.request_close();
            }
            Owner::Disposing(handle) => {
                handle.request_close();
            }
            Owner::Failed(_) => {}
        }
    }

    pub fn synchronize(&mut self, model: &Model, active: Option<SessionId>, window: &WindowFocus) {
        for (id, entry) in &mut self.entries {
            let Owner::Attached(terminal) = &mut entry.owner else {
                continue;
            };
            // Admission closes on the PTY worker before ChildExit reaches the
            // UI queue. Do not send focus traffic into that EOF-drain interval.
            entry.ended |= terminal.input_status().closed;
            let visible = active == Some(*id)
                && !matches!(
                    model.sessions().get(*id).map(|entry| entry.lifecycle()),
                    Some(Lifecycle::Closing { .. }) | None
                );
            if terminal.is_visible() != visible {
                terminal.set_visible(visible);
            }
            // ChildExit closes input before delivery. Avoid turning a focus
            // reply to an exited child into a requested shutdown during drain.
            if !entry.ended {
                terminal.set_focused(visible && window.is_focused());
            }
        }
    }

    /// A typed subscription/widget origin and the emulator ID must agree.
    /// Stale messages cannot select the current pane as an implicit target.
    pub fn route(&mut self, model: &mut Model, origin: SessionId, event: Event) {
        let Event::BackendCall(raw, command) = event;
        if raw != origin.get() {
            return;
        }
        let Some(entry) = self.entries.get_mut(&origin) else {
            return;
        };
        if model.sessions().get(origin).is_none() {
            return;
        }
        let Owner::Attached(terminal) = &mut entry.owner else {
            return;
        };
        entry.ended |= terminal.input_status().closed;
        // Retain selection, viewport resize and final protocol/lifecycle traffic.
        // The backend already closes admission at ChildExit; this also prevents
        // stale widget input/focus from disturbing its final EOF drain.
        if (entry.ended || !terminal.is_visible())
            && matches!(
                command,
                BackendCommand::Write(_) | BackendCommand::MouseReport(..)
            )
        {
            return;
        }
        if entry.ended && matches!(command, BackendCommand::FocusChanged(_)) {
            return;
        }
        if !terminal.is_visible()
            && matches!(
                command,
                BackendCommand::Scroll(_) | BackendCommand::FocusChanged(true)
            )
        {
            return;
        }
        match terminal.handle(iced_term::Command::ProxyToBackend(command)) {
            Action::ChildExit(_) | Action::Shutdown { .. } => entry.ended = true,
            Action::BackendCompleted(completion) => {
                entry.ended = true;
                entry.completion = Some(completion);
                terminal.final_content();
                if matches!(
                    completion.reason,
                    iced_term::ExitReason::IoError(_) | iced_term::ExitReason::Panicked
                ) {
                    entry.notice = Some(
                        "The terminal stopped unexpectedly. Its final output is retained.".into(),
                    );
                }
                if matches!(
                    model.sessions().get(origin).map(|e| e.lifecycle()),
                    Some(Lifecycle::Closing { .. })
                ) {
                    model.closed(origin);
                    self.entries.remove(&origin);
                } else {
                    model.exited(origin, completion.exit_code);
                }
            }
            Action::InputRejected(error) if !entry.ended => {
                entry.notice = Some(format!("Input was not sent: {error}"))
            }
            Action::EventBridgeError(_) => {
                entry.notice =
                    Some("Terminal output stopped. Close this terminal to recover.".into())
            }
            _ => {}
        }
    }
}
