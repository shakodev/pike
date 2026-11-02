//! Actual-App native pixels and layout. No synthetic model replacement.
use crate::{host::Host, report};
use iced::advanced::{
    Layout, Shell, clipboard, layout, mouse,
    renderer::{Headless, Renderer as _, Style},
    widget::{Id, Operation, Tree, operation},
};
use iced::{Color, Event, Point, Rectangle, Size, Vector};
use pike::app::Message;
use std::{fs, io, path::Path, time::Instant};

#[derive(Default)]
struct Metrics {
    terminals: Vec<Id>,
    visible: Vec<Rectangle>,
    headers: Vec<Rectangle>,
    tab_strip: Option<Rectangle>,
    translation: Vector,
    viewport: Rectangle,
    pending: Option<(Vector, Rectangle)>,
}

impl Operation for Metrics {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let saved = (self.translation, self.viewport);
        if let Some((translation, bounds)) = self.pending.take() {
            self.viewport = self.viewport.intersection(&bounds).unwrap_or_default();
            self.translation += translation;
        }
        operate(self);
        (self.translation, self.viewport) = saved;
    }
    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&Id::from("tab-strip")) {
            self.tab_strip = Some(bounds - self.translation);
        }
        // Header IDs are matched against the exact current session set below;
        // all other containers are irrelevant to this measurement.
        if self.terminals.iter().any(|terminal| id == Some(terminal)) {
            self.headers.push(bounds - self.translation);
        }
    }
    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _: &mut dyn operation::Focusable) {
        if self.terminals.iter().any(|terminal| id == Some(terminal))
            && let Some(clip) = (bounds - self.translation).intersection(&self.viewport)
        {
            self.visible.push(clip);
        }
    }
    fn scrollable(
        &mut self,
        _: Option<&Id>,
        bounds: Rectangle,
        _: Rectangle,
        translation: Vector,
        _: &mut dyn operation::Scrollable,
    ) {
        self.pending = Some((translation, bounds - self.translation));
    }
}

pub fn capture(host: &mut Host, output: &Path, name: &str, size: Size) -> io::Result<()> {
    for (theme, mode) in [
        ("dark", iced::theme::Mode::Dark),
        ("light", iced::theme::Mode::Light),
    ] {
        host.update(Message::SystemTheme(mode));
        for scale in [1.0, 1.25, 1.5, 2.0] {
            paint(
                host,
                output,
                &format!("{name}-{theme}-{}", (scale * 100.0) as u32),
                size,
                scale,
            )?;
        }
    }
    Ok(())
}

fn paint(host: &mut Host, output: &Path, name: &str, size: Size, scale: f32) -> io::Result<()> {
    let viewport = Rectangle::with_size(size);
    // Let the actual sensor/terminal messages settle the requested viewport.
    // Three finite passes are fixture work, never a product redraw timer.
    for _ in 0..3 {
        let messages = {
            let mut view = host.app.view();
            let mut tree = Tree::new(view.as_widget());
            let node = view.as_widget_mut().layout(
                &mut tree,
                &host.renderer,
                &layout::Limits::new(size, size),
            );
            if let Some(id) = host
                .app
                .active_session()
                .and_then(|id| host.app.terminal(id))
                .map(|t| t.widget_id().clone())
            {
                view.as_widget_mut().operate(
                    &mut tree,
                    Layout::new(&node),
                    &host.renderer,
                    &mut operation::focusable::focus::<()>(id),
                );
            }
            let mut messages = Vec::new();
            view.as_widget_mut().update(
                &mut tree,
                &Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &host.renderer,
                &mut clipboard::Null,
                &mut Shell::new(&mut messages),
                &viewport,
            );
            messages
        };
        for message in messages {
            host.update(message);
        }
        host.drain()?;
    }
    let mut view = host.app.view();
    let mut tree = Tree::new(view.as_widget());
    let node =
        view.as_widget_mut()
            .layout(&mut tree, &host.renderer, &layout::Limits::new(size, size));
    let mut metrics = Metrics {
        viewport,
        ..Default::default()
    };
    for (id, _) in host.app.model().sessions().iter() {
        if let Some(term) = host.app.terminal(id) {
            metrics.terminals.push(term.widget_id().clone());
        }
    }
    if let Some(id) = host
        .app
        .active_session()
        .and_then(|id| host.app.terminal(id))
        .map(|t| t.widget_id().clone())
    {
        view.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            &host.renderer,
            &mut operation::focusable::focus::<()>(id),
        );
    }
    view.as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &host.renderer, &mut metrics);
    // Inspect header geometry separately so widget IDs can never alias a header.
    let mut headers = Metrics {
        viewport,
        terminals: host
            .app
            .model()
            .sessions()
            .iter()
            .map(|(id, _)| Id::from(format!("pane-header-{}", id.get())))
            .collect(),
        ..Default::default()
    };
    view.as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &host.renderer, &mut headers);
    for header in &headers.headers {
        assert_eq!(header.height, 26.0);
    }
    if let Some(strip) = metrics.tab_strip {
        assert_eq!(strip.height, 34.0);
    }
    view.as_widget_mut().update(
        &mut tree,
        &Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &host.renderer,
        &mut clipboard::Null,
        &mut Shell::new(&mut Vec::new()),
        &viewport,
    );
    host.renderer.reset(viewport);
    view.as_widget().draw(
        &tree,
        &mut host.renderer,
        &host.app.theme(),
        &Style::default(),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &viewport,
    );
    let dimensions = Size::new((size.width * scale) as u32, (size.height * scale) as u32);
    let rgba = Headless::screenshot(&mut host.renderer, dimensions, scale, Color::BLACK);
    let mut green = 0;
    for (index, pixel) in rgba.chunks_exact(4).enumerate() {
        if pixel[1] > 90
            && pixel[1] > pixel[0].saturating_add(30)
            && pixel[1] > pixel[2].saturating_add(30)
        {
            green += 1;
            let point = Point::new(
                ((index % dimensions.width as usize) as f32 + 0.5) / scale,
                ((index / dimensions.width as usize) as f32 + 0.5) / scale,
            );
            assert!(
                metrics
                    .visible
                    .iter()
                    .any(|bounds| bounds.expand(1.0 / scale).contains(point)),
                "controlled prompt escaped actual terminal viewport in {name} at {point:?}"
            );
        }
    }
    if !metrics.visible.is_empty() {
        assert!(green > 20, "no legible controlled prompt in {name}");
    }
    fs::write(
        output.join(format!("{name}.txt")),
        format!(
            "logical={size:?} scale={scale} physical={dimensions:?}\nstrip={:?}\nheaders={:?}\nvisible_terminals={:?}\ngreen_prompt_pixels={green}\n",
            metrics.tab_strip, headers.headers, metrics.visible
        ),
    )?;
    report::screenshot(
        &output.join(format!("{name}.ppm")),
        iced::window::Screenshot::new(rgba, dimensions, scale),
    )
}
