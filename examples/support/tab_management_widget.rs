//! Synthetic pointer/keyboard/IME delivery through the actual public App view.
use iced::advanced::widget::{Tree, operation};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::keyboard::{self, Key, Modifiers};
use iced::{Event, Point, Rectangle, Size};
use pike::app::{App, Message};

pub const SIZE: Size = Size::new(1100.0, 720.0);

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
    fn centers(layout: Layout<'_>, found: &mut Vec<Point>) {
        if Rectangle::with_size(SIZE).contains(layout.bounds().center()) {
            found.push(layout.bounds().center());
        }
        for child in layout.children() {
            centers(child, found);
        }
    }
    let mut points = Vec::new();
    centers(root, &mut points);
    for position in points {
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
    None
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
