//! A delayed focus request for a removed widget must not unfocus its replacement.
use crate::{identity::SessionId, layout::PaneTarget};
use iced::advanced::widget::{
    Id, Operation, operate,
    operation::{Focusable, Outcome, focusable},
};
use iced::{Rectangle, Task};
use iced_term::WindowFocus;
use std::sync::{Arc, Mutex};

/// A lease changes on every ownership transition, including A -> modal -> A.
/// Widget operations are runtime tasks and may execute after a newer selection.
#[derive(Clone, Default)]
pub(super) struct Authority(Arc<Mutex<Option<Arc<SessionId>>>>);

impl Authority {
    pub(super) fn synchronize(&self, owner: Option<SessionId>) {
        let mut current = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if current.as_deref().copied() != owner {
            *current = owner.map(Arc::new);
        }
    }

    pub(super) fn request(
        &self,
        owner: SessionId,
        target: Id,
        window: WindowFocus,
    ) -> Task<super::Message> {
        let lease = self
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let Some(lease) = lease.filter(|lease| **lease == owner) else {
            return Task::none();
        };
        operate(Authorized {
            authority: self.clone(),
            lease,
            window,
            operation: Box::new(IfPresent {
                target,
                found: false,
            }),
        })
    }
}

struct Authorized {
    authority: Authority,
    lease: Arc<SessionId>,
    window: WindowFocus,
    operation: Box<dyn Operation<super::Message>>,
}

impl Authorized {
    fn valid(&self) -> bool {
        self.window.is_focused()
            && self
                .authority
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &self.lease))
    }
}

impl Operation<super::Message> for Authorized {
    fn traverse(&mut self, traverse: &mut dyn FnMut(&mut dyn Operation<super::Message>)) {
        if self.valid() {
            // Keep this guard around every traversal, including the focus pass.
            traverse(self);
        }
    }

    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        if self.valid() {
            self.operation.focusable(id, bounds, state);
        }
    }

    fn finish(&self) -> Outcome<super::Message> {
        if !self.valid() {
            return Outcome::None;
        }
        match self.operation.finish() {
            Outcome::Chain(operation) => Outcome::Chain(Box::new(Self {
                authority: self.authority.clone(),
                lease: self.lease.clone(),
                window: self.window.clone(),
                operation,
            })),
            outcome => outcome,
        }
    }
}

impl super::App {
    /// Every pane selection (click, attachment, close fallback) uses this route.
    pub(super) fn select(&mut self, target: PaneTarget) -> Task<super::Message> {
        if self.exiting || self.model.focus(target).is_err() {
            return Task::none();
        }
        self.synchronize();
        self.focus_active()
    }

    pub(super) fn focus_active(&self) -> Task<super::Message> {
        self.active_session()
            .and_then(|id| self.terminal(id).map(|terminal| (id, terminal)))
            .map(|(id, terminal)| {
                self.focus_authority.request(
                    id,
                    terminal.widget_id().clone(),
                    self.window_focus.clone(),
                )
            })
            .unwrap_or_else(Task::none)
    }
}

struct IfPresent {
    target: Id,
    found: bool,
}

impl<T: 'static> Operation<T> for IfPresent {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }

    fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, _state: &mut dyn Focusable) {
        self.found |= id == Some(&self.target);
    }

    fn finish(&self) -> Outcome<T> {
        if self.found {
            Outcome::Chain(Box::new(focusable::focus(self.target.clone())))
        } else {
            Outcome::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::futures::{FutureExt, StreamExt};
    struct State(bool);
    impl Focusable for State {
        fn is_focused(&self) -> bool {
            self.0
        }
        fn focus(&mut self) {
            self.0 = true;
        }
        fn unfocus(&mut self) {
            self.0 = false;
        }
    }

    fn pending(
        authority: &Authority,
        owner: SessionId,
        target: Id,
        window: WindowFocus,
    ) -> Box<dyn Operation> {
        let task = authority.request(owner, target, window);
        let action = iced_runtime::task::into_stream(task)
            .unwrap()
            .next()
            .now_or_never()
            .flatten()
            .unwrap();
        let iced_runtime::Action::Widget(operation) = action else {
            panic!("expected widget operation")
        };
        operation
    }

    fn visit(operation: &mut dyn Operation, target: &Id, other: &Id, states: &mut [State; 2]) {
        operation.traverse(&mut |operation| {
            operation.focusable(Some(target), Rectangle::default(), &mut states[0]);
            operation.focusable(Some(other), Rectangle::default(), &mut states[1]);
        });
    }

    #[test]
    fn delayed_visible_owner_cannot_override_a_newer_selection() {
        let first = crate::identity::Reservation::allocate().unwrap().session;
        let second = crate::identity::Reservation::allocate().unwrap().session;
        let authority = Authority::default();
        let window = WindowFocus::default();
        window.set_focused(true);
        let (target, other) = (Id::unique(), Id::unique());
        authority.synchronize(Some(first));
        let mut delayed = pending(&authority, first, target.clone(), window);
        authority.synchronize(Some(second));
        let mut states = [State(false), State(true)];
        visit(delayed.as_mut(), &target, &other, &mut states);
        assert!(matches!(delayed.finish(), Outcome::None));
        assert!(!states[0].0 && states[1].0);
    }

    #[test]
    fn authorization_is_rechecked_between_presence_and_focus_passes() {
        let owner = crate::identity::Reservation::allocate().unwrap().session;
        let authority = Authority::default();
        let window = WindowFocus::default();
        window.set_focused(true);
        let (target, other) = (Id::unique(), Id::unique());
        authority.synchronize(Some(owner));
        let mut operation = pending(&authority, owner, target.clone(), window.clone());
        let mut states = [State(false), State(true)];
        visit(operation.as_mut(), &target, &other, &mut states);
        let Outcome::Chain(mut focus) = operation.finish() else {
            panic!("missing focus pass")
        };
        // Even returning to the same SessionId cannot revive the old lease.
        authority.synchronize(None);
        authority.synchronize(Some(owner));
        visit(focus.as_mut(), &target, &other, &mut states);
        assert!(!states[0].0 && states[1].0);
        let mut current = pending(&authority, owner, target.clone(), window.clone());
        // Background synchronization of an unchanged owner preserves its lease.
        authority.synchronize(Some(owner));
        visit(current.as_mut(), &target, &other, &mut states);
        let Outcome::Chain(mut focus) = current.finish() else {
            panic!("missing current focus pass")
        };
        visit(focus.as_mut(), &target, &other, &mut states);
        assert!(states[0].0 && !states[1].0);
        // Native/widget blur is immediate, before its App message is consumed.
        let mut blurred = pending(&authority, owner, other.clone(), window.clone());
        window.set_focused(false);
        visit(blurred.as_mut(), &target, &other, &mut states);
        assert!(matches!(blurred.finish(), Outcome::None));
        assert!(states[0].0 && !states[1].0);
    }

    #[test]
    fn obsolete_focus_effect_does_not_unfocus_a_replacement() {
        let mut operation = IfPresent {
            target: Id::unique(),
            found: false,
        };
        let mut current = State(true);
        Operation::<()>::focusable(
            &mut operation,
            Some(&Id::unique()),
            Rectangle::default(),
            &mut current,
        );
        assert!(matches!(Operation::<()>::finish(&operation), Outcome::None));
        assert!(current.is_focused());
    }

    #[test]
    fn existing_target_still_gets_exclusive_focus() {
        let target = Id::unique();
        let other = Id::unique();
        let mut operation = IfPresent {
            target: target.clone(),
            found: false,
        };
        let mut previous = State(true);
        let mut current = State(false);
        Operation::<()>::focusable(
            &mut operation,
            Some(&other),
            Rectangle::default(),
            &mut previous,
        );
        Operation::<()>::focusable(
            &mut operation,
            Some(&target),
            Rectangle::default(),
            &mut current,
        );
        let Outcome::Chain(mut effect) = Operation::<()>::finish(&operation) else {
            panic!("missing focus effect");
        };
        effect.focusable(Some(&other), Rectangle::default(), &mut previous);
        effect.focusable(Some(&target), Rectangle::default(), &mut current);
        assert!(!previous.is_focused());
        assert!(current.is_focused());
    }
}
