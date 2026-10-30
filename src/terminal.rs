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
        // Clear the previous input owner before enabling another, regardless of
        // SessionId ordering. Terminal owns transient focus/composition state.
        for (id, entry) in &mut self.entries {
            if active != Some(*id)
                && !entry.ended
                && let Owner::Attached(terminal) = &mut entry.owner
                && !terminal.input_status().closed
            {
                terminal.set_focused(false);
            }
        }
        let tab = active
            .and_then(|_| model.workspace().active())
            .and_then(|id| model.workspace().tab(id));
        for (id, entry) in &mut self.entries {
            let Owner::Attached(terminal) = &mut entry.owner else {
                continue;
            };
            // Admission closes on the PTY worker before ChildExit reaches the
            // UI queue. Do not send focus traffic into that EOF-drain interval.
            entry.ended |= terminal.input_status().closed;
            let visible = tab.is_some_and(|tab| {
                tab.grid().iter().any(|(pane, state)| {
                    state.session == *id && tab.grid().maximized().is_none_or(|max| max == *pane)
                })
            }) && !matches!(
                model.sessions().get(*id).map(|entry| entry.lifecycle()),
                Some(Lifecycle::Closing { .. }) | None
            );
            if terminal.is_visible() != visible {
                terminal.set_visible(visible);
            }
            // ChildExit closes input before delivery. Avoid turning a focus
            // reply to an exited child into a requested shutdown during drain.
            if !entry.ended {
                terminal.set_focused(visible && active == Some(*id) && window.is_focused());
            }
        }
    }

    /// A typed subscription/widget origin and the emulator ID must agree.
    /// Stale messages cannot select the current pane as an implicit target.
    pub fn route(
        &mut self,
        model: &mut Model,
        window: &WindowFocus,
        origin: SessionId,
        event: Event,
    ) {
        let Event::BackendCall(raw, mut command) = event;
        let active = model
            .workspace()
            .active()
            .and_then(|tab| model.workspace().tab(tab))
            .and_then(|tab| tab.grid().get(tab.focused()))
            .is_some_and(|pane| pane.session == origin);
        if matches!(
            command,
            BackendCommand::Paste(_) | BackendCommand::ImeCommit(_)
        ) {
            let accepts = raw == origin.get()
                && active
                && match &command {
                    BackendCommand::Paste(request) => request.session_id() == origin.get(),
                    _ => true, // IME's opaque owner is checked again by the adapter.
                }
                && window.is_focused()
                && self.entries.get(&origin).is_some_and(|entry| {
                    model.sessions().get(origin).is_some_and(|session| {
                        session.reservation() == entry.reservation
                            && session.lifecycle() == Lifecycle::Running
                    }) && !entry.ended
                        && entry.terminal().is_some_and(|terminal| {
                            terminal.id == raw
                                && terminal.is_visible()
                                && !terminal.input_status().closed
                                && terminal.renderable_content().is_some_and(|c| c.focused)
                        })
                });
            if !accepts {
                // Invalidate every clone before any early identity/lifecycle
                // return, even when the owner was removed from the host.
                match &command {
                    BackendCommand::Paste(request) => request.cancel(),
                    BackendCommand::ImeCommit(commit) => commit.cancel(),
                    _ => unreachable!(),
                }
                return;
            }
        }
        if raw != origin.get() {
            return;
        }
        // Widget focus receipts can be queued across pane selection. Only the
        // common app selection path transfers input authority to a sibling.
        if !active && matches!(command, BackendCommand::FocusChanged(true)) {
            return;
        }
        let Some(entry) = self.entries.get_mut(&origin) else {
            return;
        };
        let Some(session) = model.sessions().get(origin) else {
            return;
        };
        if session.reservation() != entry.reservation {
            return;
        }
        let Owner::Attached(terminal) = &mut entry.owner else {
            return;
        };
        if terminal.id != raw {
            return;
        }
        entry.ended |= terminal.input_status().closed;
        let visible =
            terminal.is_visible() && !matches!(session.lifecycle(), Lifecycle::Closing { .. });
        let writable = visible && !entry.ended && session.lifecycle() == Lifecycle::Running;
        // Recheck focus at consumption: widget input may be queued across a
        // pane/window focus change. Hovered mouse/wheel must not acquire focus.
        if matches!(command, BackendCommand::Write(_))
            && !(writable
                && active
                && window.is_focused()
                && terminal.renderable_content().is_some_and(|c| c.focused))
        {
            return;
        }
        if !writable && matches!(command, BackendCommand::MouseReport(..)) {
            return;
        }
        if !visible
            && matches!(
                command,
                BackendCommand::Scroll(_)
                    | BackendCommand::ScrollHistory(_)
                    | BackendCommand::SelectStart(..)
                    | BackendCommand::SelectUpdate(_)
                    | BackendCommand::ProcessLink(..)
                    | BackendCommand::FocusChanged(true)
            )
        {
            return;
        }
        if entry.ended && matches!(command, BackendCommand::FocusChanged(_)) {
            return;
        }
        // Scroll can generate alternate-screen arrow input. Retained final
        // output permits only the adapter's explicitly local history path.
        if !writable && let BackendCommand::Scroll(lines) = command {
            command = BackendCommand::ScrollHistory(iced_term::HistoryScroll::Lines(lines));
        }
        // Protocol replies and lifecycle receipts remain origin-bound and do
        // not require visibility or keyboard focus.
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
            Action::PasteRejected(error) => entry.notice = Some(error.to_string()),
            Action::ImeRejected(error) => entry.notice = Some(error.to_string()),
            Action::EventBridgeError(_) => {
                entry.notice =
                    Some("Terminal output stopped. Close this terminal to recover.".into())
            }
            _ => {}
        }
    }
}
