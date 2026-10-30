//! Synthetic pointer/keyboard/IME delivery through the actual public App view.
use iced::advanced::widget::{Id, Operation, Tree, operation};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::keyboard::{self, Key, Modifiers};
use iced::{Event, Point, Rectangle, Vector};
use pike::app::{App, Message};

pub use crate::host_viewport::SIZE;

/// The messages are emitted by real hit-testing, never manufactured from the
/// predicate. Only points inside the native fixture viewport are attempted.
pub fn click(
    app: &App,
    renderer: &iced::Renderer,
    accepts: impl Fn(&Message) -> bool,
) -> Option<Message> {
    let mut view = app.view();
    let mut tree = Tree::new(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &layout::Limits::new(SIZE, SIZE));
    let root = Layout::new(&node);
    // Operations expose scroll translations. Raw layout centers alone would
    // incorrectly claim a pointer hit on an offscreen overflow-list entry.
    for _ in 0..4 {
        let mut targets = Targets::default();
        view.as_widget_mut()
            .operate(&mut tree, root, renderer, &mut targets);
        for position in targets.points {
            let mut messages = Vec::new();
            for event in [
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                view.as_widget_mut().update(
                    &mut tree,
                    &Event::Mouse(event),
                    root,
                    mouse::Cursor::Available(position),
                    renderer,
                    &mut clipboard::Null,
                    &mut Shell::new(&mut messages),
                    &Rectangle::with_size(SIZE),
                );
            }
            if let Some(message) = messages.into_iter().find(|message| accepts(message)) {
                return Some(message);
            }
        }
        for position in targets.scrolls {
            view.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: -8.0, y: -8.0 },
                }),
                root,
                mouse::Cursor::Available(position),
                renderer,
                &mut clipboard::Null,
                &mut Shell::new(&mut Vec::new()),
                &Rectangle::with_size(SIZE),
            );
        }
    }
    None
}

pub struct Targets {
    points: Vec<Point>,
    pub scrolls: Vec<Point>,
    pub labels: Vec<(String, Point)>,
    pub containers: Vec<(Option<Id>, Rectangle)>,
    translation: Vector,
    viewport: Rectangle,
    pending: Option<(Vector, Rectangle)>,
}

impl Default for Targets {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            scrolls: Vec::new(),
            labels: Vec::new(),
            containers: Vec::new(),
            translation: Vector::ZERO,
            viewport: Rectangle::with_size(SIZE),
            pending: None,
        }
    }
}

impl Operation for Targets {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let old = (self.translation, self.viewport);
        if let Some((translation, bounds)) = self.pending.take() {
            self.viewport = self.viewport.intersection(&bounds).unwrap_or_default();
            self.translation += translation;
        }
        operate(self);
        (self.translation, self.viewport) = old;
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        let point = bounds.center() - self.translation;
        if self.viewport.contains(point) {
            self.points.push(point);
            self.containers
                .push((id.cloned(), bounds - self.translation));
        }
    }

    fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
        let point = bounds.center() - self.translation;
        if self.viewport.contains(point) {
            self.labels.push((text.to_owned(), point));
        }
    }

    fn scrollable(
        &mut self,
        _id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        _state: &mut dyn operation::Scrollable,
    ) {
        let bounds = bounds - self.translation;
        if (content.height > bounds.height || content.width > bounds.width)
            && let Some(visible) = self.viewport.intersection(&bounds)
        {
            self.scrolls.push(visible.center());
        }
        self.pending = Some((translation, bounds));
    }
}

/// Use a single tree for a finite event sequence, including IME preedit/commit.
/// Focus is applied through Iced's widget operation before keyboard delivery.
pub fn events(
    app: &App,
    renderer: &iced::Renderer,
    focus: Option<iced::advanced::widget::Id>,
    events: impl IntoIterator<Item = Event>,
) -> Vec<Message> {
    let mut view = app.view();
    let mut tree = Tree::new(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &layout::Limits::new(SIZE, SIZE));
    if let Some(focus) = focus {
        view.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            renderer,
            &mut operation::focusable::focus::<()>(focus),
        );
    }
    let mut messages = Vec::new();
    for event in events {
        view.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(SIZE),
        );
    }
    messages.retain(|message| {
        !matches!(
            message,
            Message::Terminal((
                _,
                iced_term::Event::BackendCall(_, iced_term::BackendCommand::Resize(..))
            ))
        )
    });
    messages
}

pub fn key(key: Key, modifiers: Modifiers, text: Option<&str>) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    })
}
