//! Exercise the public widget API with the same tree the renderer uses.
use crate::check;
use iced::advanced::widget::Tree;
use iced::advanced::widget::operation::{self, Operation, Outcome};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::{Event, Point, Rectangle};
use iced_term::{BackendCommand, Terminal, TerminalView};
use std::io;

fn dispatch_raw(
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
    drop(view);
    events
}

fn dispatch(
    terminal: &mut Terminal,
    tree: &mut Tree,
    renderer: &iced::Renderer,
    bounds: Rectangle,
    event: Event,
    cursor: Point,
) -> Vec<iced_term::Event> {
    let mut events = dispatch_raw(terminal, tree, renderer, bounds, event, cursor);
    // Route session focus commands as an application does. This probe's
    // returned messages check geometry/gestures; terminal_protocol checks the
    // resulting focus bytes and duplicate suppression under real PTY input.
    events.retain(|event| {
        if let iced_term::Event::BackendCall(_, command @ BackendCommand::FocusChanged(_)) = event {
            terminal.handle(iced_term::Command::ProxyToBackend(command.clone()));
            false
        } else {
            true
        }
    });
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
    terminals: &mut [Terminal],
    bounds: Rectangle,
) -> io::Result<()> {
    let window_focus = iced_term::WindowFocus::default();
    window_focus.set_focused(true);
    for terminal in terminals.iter_mut() {
        terminal.set_window_focus(window_focus.clone());
    }
    let mut tree = Tree::empty();
    let cursor = Point::new(bounds.x + 45.0, bounds.y + 60.0);
    let click = || Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    for index in 0..32 {
        let terminal = &mut terminals[index % 2];
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
        let events = dispatch(terminal, &mut tree, renderer, bounds, click(), cursor);
        if terminal.id == 2 {
            // This fresh widget has not processed a focused CursorMoved. Its
            // first press still has to target the current cell, not grid 0,0.
            let content = terminal.renderable_content().unwrap();
            let cell = content.terminal_size;
            check(
                matches!(events.as_slice(),
                [iced_term::Event::BackendCall(id, BackendCommand::MouseReport(
                    iced_term::MouseButton::LeftButton, _, point, true))]
                if *id == terminal.id
                    && point.line.0 == (60.0 / cell.cell_height).floor() as i32
                    && point.column.0 == (45.0 / cell.cell_width).floor() as usize),
                "first mouse press after tab replacement used a stale grid position",
            )?;
        }
        check(
            focused(terminal, &mut tree, renderer, bounds),
            "fixture click did not focus the terminal",
        )?;
    }
    let terminal = &mut terminals[1];
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
    let virtual_pixels = query_area(terminal);
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
        query_area(terminal) == virtual_pixels,
        "monitor scale changed virtual-grid protocol pixels",
    )?;
    check(
        after.cache_invalidations == before.cache_invalidations + 1
            && after.font_measurements == before.font_measurements
            && after.pty_resizes == before.pty_resizes
            && after.grid_resizes == before.grid_resizes,
        "DPI invalidation did unnecessary session or font work",
    )?;
    verify_window_focus(renderer, terminals, bounds)
}

fn query_area(terminal: &mut Terminal) -> (u32, u32) {
    let area = std::sync::Arc::new(std::sync::Mutex::new(None));
    let recorded = area.clone();
    terminal.handle(iced_term::Command::ProxyToBackend(
        BackendCommand::ProcessAlacrittyEvent(iced_term::AlacrittyEvent::TextAreaSizeRequest(
            std::sync::Arc::new(move |size| {
                *recorded.lock().unwrap() = Some((size.width, size.height));
                String::new()
            }),
        )),
    ));
    area.lock().unwrap().expect("area formatter was called")
}

fn focus_operation(
    terminal: &Terminal,
    tree: &mut Tree,
    renderer: &iced::Renderer,
    bounds: Rectangle,
) {
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
    let mut operation = operation::focusable::focus::<()>(terminal.widget_id().clone());
    view.as_widget_mut().operate(
        tree,
        Layout::new(&node),
        renderer,
        &mut operation::black_box(&mut operation),
    );
}

fn admitted(terminal: &Terminal) -> usize {
    let status = terminal.input_status();
    status.written_bytes + status.outstanding_bytes + status.cancelled_bytes
}

fn verify_window_focus(
    renderer: &iced::Renderer,
    terminals: &mut [Terminal],
    bounds: Rectangle,
) -> io::Result<()> {
    let window = iced_term::WindowFocus::default();
    for terminal in terminals.iter_mut().take(2) {
        terminal.set_window_focus(window.clone());
        terminal.set_focused(false);
        check(
            terminal
                .renderable_content()
                .unwrap()
                .terminal_mode
                .contains(iced_term::TermMode::FOCUS_IN_OUT),
            "focus fixture mode is not enabled",
        )?;
    }
    let mut tree = Tree::empty();
    let cursor = bounds.center();
    let redraw = || {
        Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))
    };
    let a = &mut terminals[0];
    focus_operation(a, &mut tree, renderer, bounds);
    let before = admitted(a);
    dispatch(a, &mut tree, renderer, bounds, redraw(), cursor);
    check(
        admitted(a) == before,
        "unknown window focus synthesized a report",
    )?;
    dispatch(
        a,
        &mut tree,
        renderer,
        bounds,
        Event::Window(iced::window::Event::Focused),
        cursor,
    );
    check(
        admitted(a) == before + 3,
        "real window focus did not emit one CSI-I",
    )?;
    dispatch(
        a,
        &mut tree,
        renderer,
        bounds,
        Event::Window(iced::window::Event::Unfocused),
        cursor,
    );
    check(
        admitted(a) == before + 6,
        "real window blur did not emit one CSI-O",
    )?;

    // Exact review reproduction: A unfocused -> ID B -> programmatic focus ->
    // redraw, with NO Window::Focused. Also recreate the entire widget tree.
    let b = &mut terminals[1];
    let before = admitted(b);
    focus_operation(b, &mut tree, renderer, bounds);
    dispatch(b, &mut tree, renderer, bounds, redraw(), cursor);
    check(
        admitted(b) == before,
        "ID replacement synthesized focus-in while window was unfocused",
    )?;
    let mut fresh = Tree::empty();
    focus_operation(b, &mut fresh, renderer, bounds);
    dispatch(b, &mut fresh, renderer, bounds, redraw(), cursor);
    check(
        admitted(b) == before,
        "new widget tree invented window focus",
    )?;

    let a = &mut terminals[0];
    let mut fresh_a = Tree::empty();
    focus_operation(a, &mut fresh_a, renderer, bounds);
    dispatch(
        a,
        &mut fresh_a,
        renderer,
        bounds,
        Event::Window(iced::window::Event::Focused),
        cursor,
    );
    a.set_focused(false);
    let b = &mut terminals[1];
    let mut focused_tree = Tree::empty();
    focus_operation(b, &mut focused_tree, renderer, bounds);
    dispatch(b, &mut focused_tree, renderer, bounds, redraw(), cursor);
    check(
        admitted(b) == before + 3,
        "new view lost actual shared window focus",
    )?;
    dispatch(b, &mut focused_tree, renderer, bounds, redraw(), cursor);
    check(admitted(b) == before + 3, "redraw repeated a focus report")?;
    dispatch(
        b,
        &mut focused_tree,
        renderer,
        bounds,
        Event::Window(iced::window::Event::Unfocused),
        cursor,
    );
    check(
        admitted(b) == before + 6,
        "new view did not report its real blur",
    )
}
