//! A delayed focus request for a removed widget must not unfocus its replacement.
use crate::layout::PaneTarget;
use iced::advanced::widget::{
    Id, Operation, operate,
    operation::{Focusable, Outcome, focusable},
};
use iced::{Rectangle, Task};

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
            .and_then(|id| self.terminal(id))
            .map(|terminal| request(terminal.widget_id().clone()))
            .unwrap_or_else(Task::none)
    }
}

pub(super) fn request(target: Id) -> Task<super::Message> {
    operate(IfPresent {
        target,
        found: false,
    })
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
