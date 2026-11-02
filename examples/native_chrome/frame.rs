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
    chrome: Option<Rectangle>,
    notices: Option<Rectangle>,
    controls: Vec<Rectangle>,
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
        for (name, destination) in [
            ("workspace-chrome", &mut self.chrome),
            ("workspace-notices", &mut self.notices),
        ] {
            if id == Some(&Id::from(name)) {
                *destination = Some(bounds - self.translation);
            }
        }
        if [
            "new-tab-control",
            "command-menu-control",
            "tab-overflow-control",
        ]
        .iter()
        .any(|name| id == Some(&Id::from(*name)))
        {
            self.controls.push(bounds - self.translation);
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
        step(host, size)?;
    }
    if host.app.tab_dialog_token().is_none() && !crate::scenes::prompt_ready(host) {
        // A real terminal resize can scroll an old wrapped prompt into history.
        // Ask the controlled child to paint a fresh prompt at the new geometry;
        // do not accept stale or blank snapshots as readiness evidence.
        let active = host.app.active_target();
        let ids: Vec<_> = host
            .app
            .model()
            .workspace()
            .active()
            .and_then(|tab| host.app.model().workspace().tab(tab))
            .into_iter()
            .flat_map(|tab| tab.grid().iter().map(|(_, pane)| pane.session))
            .filter(|id| host.app.terminal(*id).is_some_and(|t| t.is_visible()))
            .collect();
        for id in ids {
            if marker(host, id) || host.app.terminal(id).is_none() {
                continue;
            }
            let Some(target) = host.app.model().target(id) else {
                continue;
            };
            host.update(Message::Control(pike::app::Control::Focus(target)));
            step(host, size)?;
            let raw = host.app.terminal(id).unwrap().id;
            host.update(Message::Terminal((
                id,
                iced_term::Event::BackendCall(
                    raw,
                    iced_term::BackendCommand::Write(b"r\n".to_vec()),
                ),
            )));
            host.until(|h| step(h, size).is_ok() && marker(h, id))?;
        }
        if let Some(target) = active {
            host.update(Message::Control(pike::app::Control::Focus(target)));
        }
        step(host, size)?;
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
    if !metrics.visible.is_empty() && !crate::scenes::prompt_ready(host) {
        let states: Vec<_> = host
            .app
            .model()
            .sessions()
            .iter()
            .map(|(id, _)| {
                let term = host.app.terminal(id);
                (
                    id.get(),
                    term.map(|t| t.snapshot_stats()),
                    term.and_then(|t| t.renderable_content()).map(|c| {
                        c.viewport
                            .display_iter()
                            .map(|i| i.cell.c)
                            .collect::<String>()
                            .contains("PIKE33")
                    }),
                )
            })
            .collect();
        fs::write(
            output.join(format!("{name}-readiness.txt")),
            format!("{states:?}\nvisible={:?}\n", metrics.visible),
        )?;
        return Err(io::Error::other(format!(
            "current visible snapshot lacks controlled prompt before {name}"
        )));
    }
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
        if header.height != 26.0 {
            return Err(io::Error::other(format!(
                "header is not 26 DIP in {name}: {header:?}"
            )));
        }
    }
    if let Some(strip) = metrics.tab_strip
        && strip.height != 34.0
    {
        return Err(io::Error::other(format!(
            "strip is not 34 DIP in {name}: {strip:?}"
        )));
    }
    if let Some(chrome) = metrics.chrome {
        assert_eq!(chrome.height, 34.0, "root strip in {name}");
        for control in &metrics.controls {
            assert_eq!(
                control.size(),
                Size::new(26.0, 26.0),
                "hit target in {name}"
            );
            assert!(
                viewport.intersection(control) == Some(*control),
                "unreachable control in {name}"
            );
        }
        let top = metrics
            .notices
            .map_or(chrome.y + chrome.height, |row| row.y + row.height);
        assert!(
            metrics.visible.iter().all(|r| r.y >= top),
            "host chrome overlaps terminal in {name}"
        );
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
    // Retain generated red pixels even if a later clipping/legibility check
    // fails; main still requests and awaits actual App session cleanup.
    report::screenshot(
        &output.join(format!("{name}.ppm")),
        iced::window::Screenshot::new(rgba.clone(), dimensions, scale),
    )?;
    let mut green = 0;
    for (index, pixel) in rgba.chunks_exact(4).enumerate() {
        if pixel[1] > 90
            && pixel[1] > pixel[0].saturating_add(10)
            && pixel[1] > pixel[2].saturating_add(30)
        {
            green += 1;
            let point = Point::new(
                ((index % dimensions.width as usize) as f32 + 0.5) / scale,
                ((index / dimensions.width as usize) as f32 + 0.5) / scale,
            );
            if !metrics
                .visible
                .iter()
                .any(|bounds| bounds.expand(1.0 / scale).contains(point))
            {
                return Err(io::Error::other(format!(
                    "controlled prompt escaped actual terminal viewport in {name} at {point:?}"
                )));
            }
        }
    }
    if !metrics.visible.is_empty() && green <= 20 {
        return Err(io::Error::other(format!(
            "no legible controlled prompt in {name}"
        )));
    }
    fs::write(
        output.join(format!("{name}.txt")),
        format!(
            "logical={size:?} scale={scale} physical={dimensions:?}\nstrip={:?}\nchrome={:?}\nnotices={:?}\ncontrol_targets={:?}\nheaders={:?}\nvisible_terminals={:?}\ngreen_prompt_pixels={green}\n",
            metrics.tab_strip,
            metrics.chrome,
            metrics.notices,
            metrics.controls,
            headers.headers,
            metrics.visible
        ),
    )?;
    Ok(())
}

fn marker(host: &Host, id: pike::identity::SessionId) -> bool {
    host.app
        .terminal(id)
        .and_then(|t| t.renderable_content())
        .is_some_and(|c| {
            c.viewport
                .display_iter()
                .map(|i| i.cell.c)
                .collect::<String>()
                .contains("PIKE33")
        })
}

fn step(host: &mut Host, size: Size) -> io::Result<()> {
    let viewport = Rectangle::with_size(size);
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
    Ok(())
}
