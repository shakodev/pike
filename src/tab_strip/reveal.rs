//! Reveal the measured selected row, without assuming a font or row height.
use iced::advanced::widget::{Id, Operation, operation};
use iced::{Rectangle, Vector};

use crate::identity::TabId;

/// The modal supplies a fresh scroll Id for each opening. A delayed operation
/// from a removed modal cannot scroll a later modal or any workspace viewport.
pub fn reveal<T: 'static>(scroll: Id, selected: TabId) -> impl Operation<T> {
    Reveal {
        scroll,
        row: super::row_id(selected),
        viewport: None,
        selected: None,
    }
}

struct Reveal {
    scroll: Id,
    row: Id,
    viewport: Option<(Rectangle, Vector)>,
    selected: Option<Rectangle>,
}

impl<T: 'static> Operation<T> for Reveal {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&self.row) {
            self.selected = Some(bounds);
        }
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _content: Rectangle,
        translation: Vector,
        _state: &mut dyn operation::Scrollable,
    ) {
        if id == Some(&self.scroll) {
            self.viewport = Some((bounds, translation));
        }
    }

    fn finish(&self) -> operation::Outcome<T> {
        let Some(((viewport, translation), row)) = self.viewport.zip(self.selected) else {
            return operation::Outcome::None;
        };
        let top = row.y - viewport.y;
        let bottom = top + row.height;
        let offset = if top < translation.y {
            top
        } else if bottom > translation.y + viewport.height {
            bottom - viewport.height
        } else {
            return operation::Outcome::None;
        };
        operation::Outcome::Chain(Box::new(operation::scrollable::scroll_to(
            self.scroll.clone(),
            operation::scrollable::AbsoluteOffset {
                x: None,
                y: Some(offset.max(0.0)),
            },
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Scroll(Option<operation::scrollable::AbsoluteOffset<Option<f32>>>);
    impl operation::Scrollable for Scroll {
        fn snap_to(&mut self, _: operation::scrollable::RelativeOffset<Option<f32>>) {
            panic!("absolute operation required")
        }
        fn scroll_to(&mut self, offset: operation::scrollable::AbsoluteOffset<Option<f32>>) {
            self.0 = Some(offset);
        }
        fn scroll_by(
            &mut self,
            _: operation::scrollable::AbsoluteOffset,
            _: Rectangle,
            _: Rectangle,
        ) {
            panic!("absolute operation required")
        }
    }

    #[test]
    fn overflow_reveal_uses_measured_rows_and_current_scroll_and_rejects_stale_modal_ids() {
        let selected = TabId::allocate().unwrap();
        let id = Id::unique();
        let viewport = Rectangle {
            x: 20.0,
            y: 100.0,
            width: 600.0,
            height: 300.0,
        };
        let content = Rectangle {
            height: 1200.0,
            ..viewport
        };
        for (row_y, translation, expected) in [
            (780.0, 0.0, Some(427.0)),
            (180.0, 300.0, Some(80.0)),
            (480.0, 300.0, None),
        ] {
            let mut reveal = reveal::<()>(id.clone(), selected);
            let mut scroll = Scroll::default();
            reveal.scrollable(
                Some(&id),
                viewport,
                content,
                Vector::new(0.0, translation),
                &mut scroll,
            );
            reveal.container(
                Some(&super::super::row_id(selected)),
                Rectangle {
                    y: row_y,
                    height: 47.0,
                    ..viewport
                },
            );
            match (reveal.finish(), expected) {
                (operation::Outcome::None, None) => {}
                (operation::Outcome::Chain(mut op), Some(expected)) => {
                    op.scrollable(Some(&id), viewport, content, Vector::ZERO, &mut scroll);
                    assert_eq!(
                        scroll.0.unwrap(),
                        operation::scrollable::AbsoluteOffset {
                            x: None,
                            y: Some(expected)
                        }
                    );
                }
                _ => panic!("selected row was not revealed using measured bounds"),
            }
        }
        let mut old = reveal::<()>(id, selected);
        old.scrollable(
            Some(&Id::unique()),
            viewport,
            content,
            Vector::ZERO,
            &mut Scroll::default(),
        );
        old.container(Some(&super::super::row_id(selected)), content);
        assert!(matches!(old.finish(), operation::Outcome::None));
    }
}
