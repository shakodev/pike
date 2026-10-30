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
    if let Some(terminal) = app.active_session().and_then(|id| app.terminal(id)) {
        view.as_widget_mut().operate(
            tree,
            Layout::new(&node),
            renderer,
            &mut iced::advanced::widget::operation::focusable::focus::<()>(
                terminal.widget_id().clone(),
            ),
        );
    }
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

pub fn drag(
    app: &App,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    size: Size,
    nested: bool,
) -> Vec<Message> {
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
