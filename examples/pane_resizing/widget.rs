//! Independent expected geometry comes from terminal focusable widget bounds.
use iced::advanced::{
    Layout, Shell, clipboard, layout, mouse,
    widget::{Id, Operation, Tree, operation::Focusable},
};
use iced::{Event, Point, Rectangle, Size};
use pike::app::{App, Message};
use std::collections::HashMap;

fn dispatch(
    app: &App,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    size: Size,
    events: Vec<(Event, Point)>,
) -> Vec<Message> {
    let mut view = app.view();
    tree.diff(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(tree, renderer, &layout::Limits::new(size, size));
    let mut messages = vec![];
    for (event, point) in events {
        view.as_widget_mut().update(
            tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Available(point),
            renderer,
            &mut clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(size),
        );
    }
    messages
}

pub fn key(
    app: &App,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    size: Size,
    vertical: bool,
) -> Vec<Message> {
    use iced::keyboard::{self, key::Named};
    let key = keyboard::Key::Named(if vertical {
        Named::ArrowUp
    } else {
        Named::ArrowRight
    });
    dispatch(
        app,
        renderer,
        tree,
        size,
        vec![(
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::ALT | keyboard::Modifiers::SHIFT,
                text: None,
                repeat: false,
            }),
            Point::ORIGIN,
        )],
    )
}

/// Exercise one built parent view throughout composition. The live terminal
/// getter must change while that parent remains borrowed, without app rebuild.
pub fn composition_priority(
    app: &App,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    size: Size,
) -> bool {
    use iced::advanced::input_method::Event as Ime;
    let mut view = app.view();
    tree.diff(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(tree, renderer, &layout::Limits::new(size, size));
    let id = app.active_session().unwrap();
    let terminal = app.terminal(id).unwrap();
    let key = iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowRight);
    let arrow = Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::ALT | iced::keyboard::Modifiers::SHIFT,
        text: None,
        repeat: false,
    });
    let redraw = Event::Window(iced::window::Event::RedrawRequested(
        std::time::Instant::now(),
    ));
    let mut messages = vec![];
    for (index, event) in [
        redraw.clone(),
        redraw,
        Event::InputMethod(Ime::Opened),
        Event::InputMethod(Ime::Preedit("ni".into(), Some(0..2))),
        Event::InputMethod(Ime::Preedit(String::new(), None)),
        arrow.clone(),
        Event::InputMethod(Ime::Commit(String::new())),
        arrow.clone(),
        Event::InputMethod(Ime::Closed),
        arrow,
    ]
    .into_iter()
    .enumerate()
    {
        view.as_widget_mut().update(
            tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(size),
        );
        if (2..=7).contains(&index) && !terminal.is_composing() {
            return false;
        }
    }
    messages
        .iter()
        .filter(|m| matches!(m, Message::Control(pike::app::Control::ResizeKey(_, _))))
        .count()
        == 1
        && !messages.iter().any(|m| {
            matches!(
                m,
                Message::Terminal((
                    _,
                    iced_term::Event::BackendCall(_, iced_term::BackendCommand::Write(_))
                ))
            )
        })
        && !terminal.is_composing()
}

fn divider_points(
    app: &App,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    size: Size,
    nested: bool,
) -> (Point, Point) {
    // The actual grid area is recovered from the focusable terminal rectangles
    // plus the real28px headers/1px borders. No window-chrome guess is used.
    let terminals = bounds(app, renderer, tree, size);
    let left = terminals
        .values()
        .map(|r| r.x - 1.0)
        .fold(f32::INFINITY, f32::min);
    let top = terminals
        .values()
        .map(|r| r.y - 29.0)
        .fold(f32::INFINITY, f32::min);
    let right = terminals
        .values()
        .map(|r| r.x + r.width + 1.0)
        .fold(0.0, f32::max);
    let bottom = terminals
        .values()
        .map(|r| r.y + r.height + 1.0)
        .fold(0.0, f32::max);
    let tab = app
        .model()
        .workspace()
        .tab(app.model().workspace().active().unwrap())
        .unwrap();
    let regions =
        tab.grid()
            .layout()
            .split_regions(4.0, 0.0, Size::new(right - left, bottom - top));
    let root = *tab.grid().layout().splits().next().unwrap();
    let (axis, region, ratio) = regions
        .iter()
        .find_map(|(split, region)| ((*split != root) == nested).then_some(*region))
        .unwrap();
    let line = axis.split_line_bounds(region, ratio, 4.0);
    let start = Point::new(left + line.center().x, top + line.center().y);
    let end = match axis {
        iced::widget::pane_grid::Axis::Vertical => Point::new(start.x + 64.0, start.y),
        _ => Point::new(start.x, start.y + 48.0),
    };
    (start, end)
}

pub fn drag(
    app: &App,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    size: Size,
    nested: bool,
) -> Vec<Message> {
    let (start, end) = divider_points(app, renderer, tree, size, nested);
    dispatch(
        app,
        renderer,
        tree,
        size,
        vec![
            (
                Event::Mouse(mouse::Event::CursorMoved { position: start }),
                start,
            ),
            (
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                start,
            ),
            (
                Event::Mouse(mouse::Event::CursorMoved { position: end }),
                end,
            ),
            (
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                end,
            ),
        ],
    )
}

pub fn tap(
    app: &App,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    size: Size,
    outside: bool,
) -> Vec<Message> {
    let (point, _) = divider_points(app, renderer, tree, size, false);
    dispatch(
        app,
        renderer,
        tree,
        size,
        vec![
            (
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                point,
            ),
            (
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                if outside {
                    Point::new(-10.0, -10.0)
                } else {
                    point
                },
            ),
        ],
    )
}

/// Called once when constructing the persistent driver tree, never per event.
pub fn prime(app: &App, renderer: &iced::Renderer, tree: &mut Tree, size: Size) {
    let mut view = app.view();
    tree.diff(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(tree, renderer, &layout::Limits::new(size, size));
    let terminal = app.terminal(app.active_session().unwrap()).unwrap();
    view.as_widget_mut().operate(
        tree,
        Layout::new(&node),
        renderer,
        &mut iced::advanced::widget::operation::focusable::focus::<()>(
            terminal.widget_id().clone(),
        ),
    );
}

pub fn focused(app: &App, renderer: &iced::Renderer, tree: &mut Tree, size: Size) -> bool {
    let mut view = app.view();
    tree.diff(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(tree, renderer, &layout::Limits::new(size, size));
    let terminal = app.terminal(app.active_session().unwrap()).unwrap();
    let mut operation =
        iced::advanced::widget::operation::focusable::is_focused(terminal.widget_id().clone());
    view.as_widget_mut().operate(
        tree,
        Layout::new(&node),
        renderer,
        &mut iced::advanced::widget::operation::black_box(&mut operation),
    );
    matches!(
        operation.finish(),
        iced::advanced::widget::operation::Outcome::Some(true)
    )
}

pub fn text(
    app: &App,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    size: Size,
    text: &str,
) -> Vec<Message> {
    use iced::keyboard as k;
    let events = text
        .chars()
        .map(Some)
        .chain([None])
        .map(|c| {
            let key = c
                .map(|c| k::Key::Character(c.to_string().into()))
                .unwrap_or(k::Key::Named(k::key::Named::Enter));
            (
                Event::Keyboard(k::Event::KeyPressed {
                    key: key.clone(),
                    modified_key: key,
                    physical_key: k::key::Physical::Unidentified(k::key::NativeCode::Unidentified),
                    location: k::Location::Standard,
                    modifiers: k::Modifiers::empty(),
                    text: c.map(|c| c.to_string().into()),
                    repeat: false,
                }),
                Point::ORIGIN,
            )
        })
        .collect();
    dispatch(app, renderer, tree, size, events)
}

pub fn blur_priority(app: &App, renderer: &iced::Renderer, tree: &mut Tree, size: Size) -> bool {
    use iced::keyboard as k;
    let key = k::Key::Named(k::key::Named::ArrowRight);
    let arrow = Event::Keyboard(k::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: k::key::Physical::Unidentified(k::key::NativeCode::Unidentified),
        location: k::Location::Standard,
        modifiers: k::Modifiers::ALT | k::Modifiers::SHIFT,
        text: None,
        repeat: false,
    });
    let messages = dispatch(
        app,
        renderer,
        tree,
        size,
        vec![
            (Event::Window(iced::window::Event::Unfocused), Point::ORIGIN),
            (arrow, Point::ORIGIN),
            (Event::Window(iced::window::Event::Focused), Point::ORIGIN),
        ],
    );
    !messages.iter().any(|m| {
        matches!(
            m,
            Message::Control(pike::app::Control::ResizeKey(_, _))
                | Message::Terminal((
                    _,
                    iced_term::Event::BackendCall(_, iced_term::BackendCommand::Write(_))
                ))
        )
    })
}

pub fn bounds(
    app: &App,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    size: Size,
) -> HashMap<Id, Rectangle> {
    struct Bounds(HashMap<Id, Rectangle>);
    impl Operation for Bounds {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
            visit(self);
        }
        fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _: &mut dyn Focusable) {
            if let Some(id) = id {
                self.0.insert(id.clone(), bounds);
            }
        }
    }
    let mut view = app.view();
    tree.diff(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(tree, renderer, &layout::Limits::new(size, size));
    let mut operation = Bounds(HashMap::new());
    view.as_widget_mut()
        .operate(tree, Layout::new(&node), renderer, &mut operation);
    operation.0
}
