use crate::{SCREEN, check};
use iced::advanced::renderer::{Headless, Renderer as _, Style};
use iced::advanced::widget::{Tree, operation};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::{Color, Event, Rectangle, Size, Theme};
use iced_term::{Command, Terminal, TerminalView};
use std::io;

pub fn render(
    terminal: &Terminal,
    renderer: &mut iced::Renderer,
    tree: &mut Tree,
    bounds: Rectangle,
    viewport: Rectangle,
    scale: f32,
) -> iced::window::Screenshot {
    renderer.reset(Rectangle::with_size(SCREEN));
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
    view.as_widget().draw(
        tree,
        renderer,
        &Theme::Dark,
        &Style::default(),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &viewport,
    );
    let dimensions = Size::new(
        (SCREEN.width * scale) as u32,
        (SCREEN.height * scale) as u32,
    );
    iced::window::Screenshot::new(
        Headless::screenshot(renderer, dimensions, scale, Color::WHITE),
        dimensions,
        scale,
    )
}

pub fn focus(terminal: &Terminal, renderer: &iced::Renderer, tree: &mut Tree, bounds: Rectangle) {
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
    view.as_widget_mut().operate(
        tree,
        Layout::new(&node),
        renderer,
        &mut operation::focusable::focus::<()>(terminal.widget_id().clone()),
    );
}

pub fn event(
    terminal: &mut Terminal,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    bounds: Rectangle,
    event: Event,
) -> iced::window::RedrawRequest {
    let mut messages = Vec::new();
    let mut shell = Shell::new(&mut messages);
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
    view.as_widget_mut().update(
        tree,
        &event,
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        renderer,
        &mut clipboard::Null,
        &mut shell,
        &bounds,
    );
    let redraw = shell.redraw_request();
    drop(view);
    for iced_term::Event::BackendCall(id, command) in messages {
        assert_eq!(id, terminal.id);
        terminal.handle(Command::ProxyToBackend(command));
    }
    redraw
}

pub fn crop(shot: &iced::window::Screenshot, rect: Rectangle) -> io::Result<Vec<u8>> {
    let scale = shot.scale_factor;
    Ok(shot
        .crop(Rectangle {
            x: (rect.x * scale).ceil() as u32,
            y: (rect.y * scale).ceil() as u32,
            width: (rect.width * scale).floor() as u32,
            height: (rect.height * scale).floor() as u32,
        })
        .map_err(io::Error::other)?
        .rgba
        .to_vec())
}
pub fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.abs_diff(*b) <= 2)
}

pub fn legible(shot: &iced::window::Screenshot, cell: Rectangle) -> io::Result<()> {
    let pixels = crop(shot, cell.shrink(1.0))?;
    let colors: std::collections::HashSet<_> = pixels
        .chunks_exact(4)
        .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
        .collect();
    check(
        colors.len() >= 3,
        "covered glyph disappeared into the cursor fill",
    )
}
