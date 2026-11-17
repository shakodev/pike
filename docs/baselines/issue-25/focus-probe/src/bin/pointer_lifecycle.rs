//! Synthetic native-renderer events through a persistent real App Tree and
//! native PTYs. Both mode-aware input and canonical geometry are observed.
use iced::advanced::{
    Layout, Shell, clipboard, layout, mouse,
    renderer::Headless,
    widget::{
        Id, Operation, Tree,
        operation::{Focusable, Outcome},
    },
};
use iced::futures::StreamExt;
use iced::{Event, Executor, Point, Rectangle, Size, Task};
use pike::{
    app::{App, Control, Direction, Message},
    executor::SharedExecutor,
};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

struct Host {
    app: App,
    tree: Tree,
    renderer: iced::Renderer,
    executor: SharedExecutor,
    size: Size,
    writes: usize,
    selects: usize,
    mouse_reports: usize,
    selection_updates: usize,
}
impl Host {
    fn task(&mut self, task: Task<Message>) {
        let Some(mut stream) = iced_runtime::task::into_stream(task) else {
            return;
        };
        while let Some(action) = self.executor.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), stream.next())
                .await
                .unwrap()
        }) {
            match action {
                iced_runtime::Action::Output(message) => {
                    let task = self.app.update(message);
                    self.task(task);
                }
                iced_runtime::Action::Widget(mut operation) => loop {
                    let mut view = self.app.view();
                    self.tree.diff(view.as_widget());
                    let node = view.as_widget_mut().layout(
                        &mut self.tree,
                        &self.renderer,
                        &layout::Limits::new(self.size, self.size),
                    );
                    view.as_widget_mut().operate(
                        &mut self.tree,
                        Layout::new(&node),
                        &self.renderer,
                        operation.as_mut(),
                    );
                    match operation.finish() {
                        Outcome::Chain(next) => operation = next,
                        _ => break,
                    }
                },
                iced_runtime::Action::Exit => (),
                _ => panic!("unexpected task effect"),
            }
        }
    }
    fn message(&mut self, message: Message) {
        let task = self.app.update(message);
        self.task(task);
    }
    fn event(&mut self, event: Event, point: Option<Point>) {
        let messages = {
            let mut view = self.app.view();
            self.tree.diff(view.as_widget());
            let node = view.as_widget_mut().layout(
                &mut self.tree,
                &self.renderer,
                &layout::Limits::new(self.size, self.size),
            );
            let mut messages = vec![];
            view.as_widget_mut().update(
                &mut self.tree,
                &event,
                Layout::new(&node),
                point
                    .map(mouse::Cursor::Available)
                    .unwrap_or(mouse::Cursor::Unavailable),
                &self.renderer,
                &mut clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(self.size),
            );
            messages
        };
        for message in messages {
            if matches!(
                &message,
                Message::Terminal((
                    _,
                    iced_term::Event::BackendCall(_, iced_term::BackendCommand::Write(_))
                ))
            ) {
                self.writes += 1;
            }
            if matches!(
                &message,
                Message::Terminal((
                    _,
                    iced_term::Event::BackendCall(
                        _,
                        iced_term::BackendCommand::SelectStart(..)
                            | iced_term::BackendCommand::MouseReport(..)
                    )
                ))
            ) {
                self.selects += 1;
            }
            if matches!(
                &message,
                Message::Terminal((
                    _,
                    iced_term::Event::BackendCall(_, iced_term::BackendCommand::MouseReport(..))
                ))
            ) {
                self.mouse_reports += 1;
            }
            if matches!(
                &message,
                Message::Terminal((
                    _,
                    iced_term::Event::BackendCall(_, iced_term::BackendCommand::SelectUpdate(..))
                ))
            ) {
                self.selection_updates += 1;
            }
            self.message(message);
        }
    }
    fn redraw(&mut self) {
        self.event(
            Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
            None,
        );
    }
    fn states(&mut self) -> HashMap<Id, (Rectangle, bool)> {
        struct States(HashMap<Id, (Rectangle, bool)>);
        impl Operation for States {
            fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
                visit(self);
            }
            fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
                if let Some(id) = id {
                    self.0.insert(id.clone(), (bounds, state.is_focused()));
                }
            }
        }
        let mut operation = States(HashMap::new());
        let mut view = self.app.view();
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.size, self.size),
        );
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            &mut operation,
        );
        operation.0
    }
    fn key(
        &mut self,
        key: iced::keyboard::Key,
        modifiers: iced::keyboard::Modifiers,
        text: Option<&str>,
    ) {
        self.event(
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: iced::keyboard::key::Physical::Unidentified(
                    iced::keyboard::key::NativeCode::Unidentified,
                ),
                location: iced::keyboard::Location::Standard,
                modifiers,
                text: text.map(Into::into),
                repeat: false,
            }),
            None,
        );
    }
}

fn canonical(host: &Host) -> String {
    let workspace = host.app.model().workspace();
    format!(
        "{:?}",
        workspace
            .tab(workspace.active().unwrap())
            .unwrap()
            .grid()
            .layout()
    )
}
fn admitted(host: &Host) -> usize {
    let workspace = host.app.model().workspace();
    workspace
        .tab(workspace.active().unwrap())
        .unwrap()
        .grid()
        .iter()
        .map(|(_, pane)| {
            let input = host.app.terminal(pane.session).unwrap().input_status();
            input.written_bytes + input.outstanding_bytes + input.cancelled_bytes
        })
        .sum()
}
fn press(host: &mut Host, point: Point) {
    host.event(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Some(point),
    );
}
fn release(host: &mut Host, point: Point) {
    host.event(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        Some(point),
    );
}
fn moved(host: &mut Host, point: Point) {
    host.event(
        Event::Mouse(mouse::Event::CursorMoved { position: point }),
        Some(point),
    );
}
fn divider(host: &mut Host) -> Point {
    let mut bounds: Vec<_> = host.states().values().map(|s| s.0).collect();
    bounds.sort_by(|a, b| a.x.total_cmp(&b.x));
    Point::new(bounds[0].x + bounds[0].width + 3.0, bounds[0].center_y())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mouse_mode = std::env::args().any(|arg| arg == "--mouse-report");
    let executor = SharedExecutor::new()?;
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or("no renderer")?;
    let script = if mouse_mode {
        "printf '\\033[?1000h\\033[?1006h'; exec sleep 60"
    } else {
        "exec sleep 60"
    };
    let (app, boot) = executor.enter(|| {
        App::with_settings(iced_term::settings::BackendSettings {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
            ..Default::default()
        })
    });
    let mut host = Host {
        app,
        renderer,
        executor,
        tree: Tree::empty(),
        size: Size::new(960.0, 600.0),
        writes: 0,
        selects: 0,
        mouse_reports: 0,
        selection_updates: 0,
    };
    host.task(boot);
    host.message(Message::WindowFocused(true));
    host.redraw();
    host.redraw();
    let first = host.app.active_target().unwrap();
    let split = host.executor.enter(|| {
        host.app
            .update(Message::Control(Control::Split(first, Direction::Right)))
    });
    host.task(split);
    host.redraw();
    host.redraw();
    let active = host.app.active_session().unwrap();
    let active_id = host.app.terminal(active).unwrap().widget_id().clone();
    let sessions: Vec<_> = host
        .app
        .model()
        .workspace()
        .tab(first.tab)
        .unwrap()
        .grid()
        .iter()
        .map(|(_, pane)| pane.session)
        .collect();
    if mouse_mode {
        use iced::futures::stream;
        let mut events = stream::select_all(
            iced::advanced::subscription::into_recipes(host.app.subscription())
                .into_iter()
                .map(|recipe| recipe.stream(stream::pending().boxed())),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !sessions.iter().all(|id| {
            host.app
                .terminal(*id)
                .unwrap()
                .renderable_content()
                .unwrap()
                .terminal_mode
                .contains(iced_term::TermMode::MOUSE_REPORT_CLICK | iced_term::TermMode::SGR_MOUSE)
        }) {
            let event = host.executor.block_on(async {
                tokio::time::timeout_at(deadline.into(), events.next())
                    .await
                    .unwrap()
                    .unwrap()
            });
            host.message(event);
        }
        println!("mouse-report mode confirmed for both native sessions");
    }
    let widget_ids: Vec<_> = sessions
        .iter()
        .map(|id| host.app.terminal(*id).unwrap().widget_id().clone())
        .collect();
    let before_ratio = canonical(&host);
    let before_input = admitted(&host);
    let edge_bounds = host.states()[&active_id].0;
    let edge = Point::new(edge_bounds.x + 1.0, edge_bounds.center_y());
    let before_commands = host.selects;
    press(&mut host, edge);
    let end = Point::new(edge.x + 30.0, edge.y);
    moved(&mut host, end);
    release(&mut host, end);
    assert_eq!(
        host.selects, before_commands,
        "divider gesture leaked terminal selection/mouse command"
    );
    assert_eq!(
        admitted(&host),
        before_input,
        "divider gesture admitted PTY bytes"
    );
    assert_ne!(canonical(&host), before_ratio, "real drag must resize");
    assert!(
        host.states()[&active_id].1,
        "divider lost live terminal focus"
    );
    println!("leeway press/move/release: no child input, real drag changed ratio, focus preserved");

    // Cancel on raw blur even when release occurs away and is never delivered.
    let point = divider(&mut host);
    press(&mut host, point);
    host.event(Event::Window(iced::window::Event::Unfocused), None);
    host.message(Message::WindowFocused(false));
    host.event(Event::Window(iced::window::Event::Focused), None);
    host.message(Message::WindowFocused(true));
    let before = canonical(&host);
    moved(&mut host, Point::new(point.x + 45.0, point.y));
    assert_eq!(canonical(&host), before, "blur left child resize armed");
    println!("raw blur/refocus without release: unheld movement preserves canonical ratio");

    // Owner loss is latched by diff, including restoration before next event.
    let point = divider(&mut host);
    press(&mut host, point);
    host.message(Message::Control(Control::Focus(first)));
    host.states();
    host.message(Message::Control(Control::Focus(
        host.app.model().target(active).unwrap(),
    )));
    let before = canonical(&host);
    moved(&mut host, Point::new(point.x + 45.0, point.y));
    assert_eq!(
        canonical(&host),
        before,
        "owner transfer left child resize armed"
    );
    println!("owner transfer/restoration: unheld movement preserves canonical ratio");

    // App focus loss can arrive separately from the raw event subscription.
    let point = divider(&mut host);
    press(&mut host, point);
    host.message(Message::WindowFocused(false));
    host.states();
    host.message(Message::WindowFocused(true));
    let before = canonical(&host);
    moved(&mut host, Point::new(point.x + 45.0, point.y));
    assert_eq!(canonical(&host), before, "app blur left child resize armed");
    println!("app blur/refocus: unheld movement preserves canonical ratio");

    // Cancellation must not consume the next ordinary pane click.
    let point = divider(&mut host);
    press(&mut host, point);
    host.message(Message::Control(Control::Focus(first)));
    let bounds = host.states()[&host
        .app
        .terminal(host.app.model().workspace().pane(first).unwrap().session)
        .unwrap()
        .widget_id()
        .clone()]
        .0;
    let center = Point::new(bounds.center_x(), bounds.center_y());
    let before_commands = host.selects;
    let before_mouse = host.mouse_reports;
    let before_updates = host.selection_updates;
    let before_input = admitted(&host);
    press(&mut host, center);
    moved(&mut host, Point::new(center.x + 20.0, center.y));
    release(&mut host, Point::new(center.x + 20.0, center.y));
    assert!(
        host.selects > before_commands,
        "ordinary click was swallowed"
    );
    if mouse_mode {
        assert!(
            host.mouse_reports >= before_mouse + 2,
            "ordinary mouse press/release did not pass through"
        );
        assert!(
            admitted(&host) > before_input,
            "ordinary mouse reports did not reach PTY admission"
        );
    } else {
        assert_eq!(
            host.selects,
            before_commands + 1,
            "normal click must start one selection"
        );
        assert!(
            host.selection_updates > before_updates,
            "ordinary drag did not extend selection"
        );
        assert_eq!(admitted(&host), before_input);
    }
    println!("ordinary pane click/drag after cancellation: terminal input passed through");

    let point = divider(&mut host);
    let before = canonical(&host);
    press(&mut host, point);
    moved(&mut host, Point::new(point.x - 25.0, point.y));
    release(&mut host, Point::new(point.x - 25.0, point.y));
    assert_ne!(
        canonical(&host),
        before,
        "fresh press must enable resize after cancellation"
    );
    assert_eq!(sessions.len(), host.app.model().sessions().len());
    for (id, widget_id) in sessions.iter().zip(widget_ids) {
        assert_eq!(host.app.terminal(*id).unwrap().widget_id(), &widget_id);
        assert!(!host.app.terminal(*id).unwrap().input_status().closed);
    }
    println!("fresh divider press resizes; stable sessions/widgets remain live");
    host.key(
        iced::keyboard::Key::Character("x".into()),
        iced::keyboard::Modifiers::empty(),
        Some("x"),
    );
    assert_eq!(host.writes, 1, "plain key after drag must reach terminal");
    host.message(Message::CloseWindow);
    assert_eq!(host.app.model().sessions().len(), 0);
    println!("cleanup sessions=0");
    Ok(())
}
