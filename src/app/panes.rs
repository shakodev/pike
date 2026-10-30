//! Pane presentation and header actions; terminal ownership stays in Sessions.
use std::sync::{Arc, Mutex};

use iced::Task;
use iced::widget::pane_grid::{DragEvent, Pane};

use super::{App, Message, commands::Command};
use crate::identity::Reservation;
use crate::layout::PaneTarget;

mod gesture;
mod view;

/// A widget-issued gesture receipt. Its reservation rejects late pre-restart
/// releases even when a retained pane handle still exists.
#[derive(Clone, Copy, Debug)]
pub struct DragAction {
    target: PaneTarget,
    owner: Reservation,
    serial: u64,
    event: DragEvent,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Context {
    target: PaneTarget,
    owner: Reservation,
    zoom: Option<Pane>,
}

#[derive(Default)]
struct State {
    context: Option<Context>,
    serial: u64,
    picked: Option<(PaneTarget, Reservation, u64)>,
}

impl State {
    fn cancel(&mut self) {
        self.picked = None;
        // Exhaustion permanently disables gestures rather than reusing a token.
        if let Some(next) = self.serial.checked_add(1) {
            self.serial = next;
        } else {
            self.context = None;
        }
    }
}

/// Gesture admission only; the existing focus Authority owns widget focus.
#[derive(Clone, Default)]
pub(super) struct Gestures(Arc<Mutex<State>>);

impl Gestures {
    fn synchronize(&self, context: Option<Context>) {
        let mut state = self.0.lock().unwrap();
        if state.context != context {
            state.cancel();
            state.context = context;
        }
    }

    fn action(&self, target: PaneTarget, owner: Reservation, event: DragEvent) -> DragAction {
        DragAction {
            target,
            owner,
            serial: self.0.lock().unwrap().serial,
            event,
        }
    }

    fn begin(&self, action: DragAction) -> bool {
        let mut state = self.0.lock().unwrap();
        if state.serial == u64::MAX
            || state.serial != action.serial
            || !state
                .context
                .is_some_and(|c| c.target.tab == action.target.tab && c.zoom.is_none())
        {
            return false;
        }
        state.picked = Some((action.target, action.owner, action.serial));
        // PaneGrid publishes one Focus before Picked. Permit that exact header
        // selection; any later owner loss (including A -> modal -> A) cancels.
        state.context = Some(Context {
            target: action.target,
            owner: action.owner,
            zoom: None,
        });
        true
    }

    fn current(&self, action: DragAction) -> bool {
        let state = self.0.lock().unwrap();
        state.serial == action.serial
            && state.picked == Some((action.target, action.owner, action.serial))
    }

    fn cancel(&self) {
        self.0.lock().unwrap().cancel();
    }
}

#[cfg(test)]
mod tests;

impl App {
    pub(super) fn synchronize_pane_gestures(&self) {
        let context = (!self.interaction_blocked() && self.window_focus.is_focused())
            .then(|| {
                let target = self.active_target()?;
                let owner = self
                    .model
                    .sessions()
                    .get(self.model.workspace().pane(target)?.session)?
                    .reservation();
                let zoom = self.model.workspace().tab(target.tab)?.grid().maximized();
                Some(Context {
                    target,
                    owner,
                    zoom,
                })
            })
            .flatten();
        self.pane_gestures.synchronize(context);
    }

    pub(super) fn pane_drag(&mut self, action: DragAction) -> Task<Message> {
        let valid = self.pane_gestures.current(action);
        self.pane_gestures.cancel();
        if !valid
            || !self.resize_allowed()
            || self.model.workspace().active() != Some(action.target.tab)
            || !self
                .model
                .workspace()
                .pane(action.target)
                .is_some_and(|pane| {
                    self.model
                        .sessions()
                        .get(pane.session)
                        .is_some_and(|e| e.reservation() == action.owner)
                })
        {
            return Task::none();
        }
        let DragEvent::Dropped { target, .. } = action.event else {
            return Task::none();
        };
        if self.model.drop_pane(action.target, target).is_err() {
            // A destination may have disappeared after the press. Refuse the
            // stale drop without changing any layout, focus or session owner.
            return Task::none();
        }
        self.notice = None;
        self.synchronize();
        self.focus_active()
    }

    pub(super) fn zoom_pane(&mut self, target: PaneTarget) -> Task<Message> {
        if !self.resize_allowed()
            || self.model.workspace().active() != Some(target.tab)
            || self.model.workspace().pane(target).is_none()
        {
            return Task::none();
        }
        self.pane_gestures.cancel();
        self.focus_on_attach.clear();
        drop(self.select(target));
        // The keyboard and header share R26's existing model/focus consumer.
        self.run_command(Command::ToggleZoom)
    }
}
