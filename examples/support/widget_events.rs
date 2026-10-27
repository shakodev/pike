//! Exercise the public widget API with the same tree the renderer uses.
use crate::check;
use iced::advanced::widget::Tree;
use iced::advanced::widget::operation::{self, Operation, Outcome};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::{Event, Point, Rectangle};
use iced_term::{BackendCommand, Terminal, TerminalView};
use std::io;

fn dispatch(
    terminal: &Terminal,
    tree: &mut Tree,
    renderer: &iced::Renderer,
    bounds: Rectangle,
    event: Event,
    cursor: Point,
) -> Vec<iced_term::Event> {
    let mut view = TerminalView::show(terminal);
    tree.diff(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(
            tree,
            renderer,
            &layout::Limits::new(bounds.size(), bounds.size()),
        )
        .move_to(bounds.position());
    let mut events = Vec::new();
    view.as_widget_mut().update(
        tree,
        &event,
        Layout::new(&node),
        mouse::Cursor::Available(cursor),
        renderer,
        &mut clipboard::Null,
        &mut Shell::new(&mut events),
        &bounds,
    );
    events
}

fn focused(
    terminal: &Terminal,
    tree: &mut Tree,
    renderer: &iced::Renderer,
    bounds: Rectangle,
) -> bool {
    let mut view = TerminalView::show(terminal);
    let node = view
        .as_widget_mut()
        .layout(
            tree,
            renderer,
            &layout::Limits::new(bounds.size(), bounds.size()),
        )
        .move_to(bounds.position());
    let mut operation = operation::focusable::is_focused(terminal.widget_id().clone());
    view.as_widget_mut().operate(
        tree,
        Layout::new(&node),
        renderer,
        &mut operation::black_box(&mut operation),
    );
    matches!(operation.finish(), Outcome::Some(true))
}

pub fn verify(
    renderer: &iced::Renderer,
    terminals: &[Terminal],
    bounds: Rectangle,
) -> io::Result<()> {
    let mut tree = Tree::empty();
    let cursor = Point::new(bounds.x + 10.0, bounds.y + 10.0);
    let click = || Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    for index in 0..32 {
        let terminal = &terminals[index % 2];
        let events = dispatch(
            terminal,
            &mut tree,
            renderer,
            bounds,
            Event::Mouse(mouse::Event::CursorMoved { position: cursor }),
            cursor,
        );
        check(
            matches!(events.as_slice(),
            [iced_term::Event::BackendCall(id, BackendCommand::Resize(Some(size), None))]
                if *id == terminal.id && *size == bounds.size()),
            "equal-sized replacement inherited a drag or suppressed its initial resize",
        )?;
        check(
            !focused(terminal, &mut tree, renderer, bounds),
            "replacement inherited keyboard focus",
        )?;
        dispatch(terminal, &mut tree, renderer, bounds, click(), cursor);
        check(
            focused(terminal, &mut tree, renderer, bounds),
            "fixture click did not focus the terminal",
        )?;
    }
    let terminal = &terminals[1];
    let moved = Rectangle::new(Point::new(280.0, 160.0), bounds.size());
    let cursor = Point::new(moved.x + 10.0, moved.y + 10.0);
    let movement = || Event::Mouse(mouse::Event::CursorMoved { position: cursor });
    check(
        dispatch(terminal, &mut tree, renderer, moved, movement(), cursor).is_empty(),
        "moved pane continued its old drag or resized an unchanged grid",
    )?;
    check(
        focused(terminal, &mut tree, renderer, moved),
        "same-session move lost keyboard focus",
    )?;
    dispatch(terminal, &mut tree, renderer, moved, click(), cursor);
    dispatch(
        terminal,
        &mut tree,
        renderer,
        moved,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        Point::ORIGIN,
    );
    check(
        dispatch(terminal, &mut tree, renderer, moved, movement(), cursor).is_empty(),
        "outside button release left a drag active",
    )?;
    dispatch(terminal, &mut tree, renderer, moved, click(), cursor);
    dispatch(
        terminal,
        &mut tree,
        renderer,
        moved,
        Event::Window(iced::window::Event::Unfocused),
        cursor,
    );
    check(
        dispatch(terminal, &mut tree, renderer, moved, movement(), cursor).is_empty(),
        "window focus loss left a drag active",
    )?;
    let before = terminal.work_stats();
    check(
        dispatch(
            terminal,
            &mut tree,
            renderer,
            moved,
            Event::Window(iced::window::Event::Rescaled(1.5)),
            cursor,
        )
        .is_empty(),
        "DPI invalidation unexpectedly resized the terminal",
    )?;
    let after = terminal.work_stats();
    check(
        after.cache_invalidations == before.cache_invalidations + 1
            && after.font_measurements == before.font_measurements
            && after.pty_resizes == before.pty_resizes
            && after.grid_resizes == before.grid_resizes,
        "DPI invalidation did unnecessary session or font work",
    )
}
