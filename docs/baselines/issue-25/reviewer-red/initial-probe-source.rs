use iced::advanced::{Layout, Shell, clipboard, layout, mouse, renderer::Headless, widget::{Id, Operation, Tree, operation::{Focusable, Outcome}}};
use iced::{Event, Executor, Point, Rectangle, Size, Task};
use iced::futures::StreamExt;
use pike::{app::{App, Control, Direction, Message}, executor::SharedExecutor};
use std::{collections::HashMap, time::{Duration, Instant}};

struct Host { app: App, tree: Tree, renderer: iced::Renderer, executor: SharedExecutor, size: Size, writes: usize }
impl Host {
    fn task(&mut self, task: Task<Message>) {
        let Some(mut stream) = iced_runtime::task::into_stream(task) else { return };
        while let Some(action) = self.executor.block_on(async { tokio::time::timeout(Duration::from_secs(10), stream.next()).await.unwrap() }) {
            match action {
                iced_runtime::Action::Output(message) => { let task = self.app.update(message); self.task(task); }
                iced_runtime::Action::Widget(mut operation) => loop {
                    let mut view = self.app.view(); self.tree.diff(view.as_widget());
                    let node = view.as_widget_mut().layout(&mut self.tree, &self.renderer, &layout::Limits::new(self.size, self.size));
                    view.as_widget_mut().operate(&mut self.tree, Layout::new(&node), &self.renderer, operation.as_mut());
                    match operation.finish() { Outcome::Chain(next) => operation = next, _ => break }
                },
                iced_runtime::Action::Exit => (),
                _ => panic!("unexpected task effect"),
            }
        }
    }
    fn message(&mut self, message: Message) { let task = self.app.update(message); self.task(task); }
    fn event(&mut self, event: Event, point: Option<Point>) {
        let messages = {
            let mut view = self.app.view(); self.tree.diff(view.as_widget());
            let node = view.as_widget_mut().layout(&mut self.tree, &self.renderer, &layout::Limits::new(self.size, self.size));
            let mut messages = vec![];
            view.as_widget_mut().update(&mut self.tree, &event, Layout::new(&node), point.map(mouse::Cursor::Available).unwrap_or(mouse::Cursor::Unavailable), &self.renderer, &mut clipboard::Null, &mut Shell::new(&mut messages), &Rectangle::with_size(self.size));
            messages
        };
        for message in messages {
            if matches!(&message, Message::Terminal((_, iced_term::Event::BackendCall(_, iced_term::BackendCommand::Write(_))))) { self.writes += 1; }
            self.message(message);
        }
    }
    fn redraw(&mut self) { self.event(Event::Window(iced::window::Event::RedrawRequested(Instant::now())), None); }
    fn states(&mut self) -> HashMap<Id, (Rectangle, bool)> {
        struct States(HashMap<Id, (Rectangle, bool)>);
        impl Operation for States {
            fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) { visit(self); }
            fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) { if let Some(id) = id { self.0.insert(id.clone(), (bounds, state.is_focused())); } }
        }
        let mut operation = States(HashMap::new()); let mut view = self.app.view(); self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(&mut self.tree, &self.renderer, &layout::Limits::new(self.size, self.size));
        view.as_widget_mut().operate(&mut self.tree, Layout::new(&node), &self.renderer, &mut operation); operation.0
    }
    fn key(&mut self, key: iced::keyboard::Key, modifiers: iced::keyboard::Modifiers, text: Option<&str>) {
        self.event(Event::Keyboard(iced::keyboard::Event::KeyPressed { key: key.clone(), modified_key: key, physical_key: iced::keyboard::key::Physical::Unidentified(iced::keyboard::key::NativeCode::Unidentified), location: iced::keyboard::Location::Standard, modifiers, text: text.map(Into::into), repeat: false }), None);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let executor = SharedExecutor::new()?;
    let renderer = executor.block_on(<iced::Renderer as Headless>::new(iced::Font::MONOSPACE, iced::Pixels(14.0), None)).ok_or("no renderer")?;
    let (app, boot) = executor.enter(|| App::with_settings(iced_term::settings::BackendSettings { program: "/bin/sh".into(), args: vec!["-c".into(), "exec sleep 30".into()], ..Default::default() }));
    let mut host = Host { app, renderer, executor, tree: Tree::empty(), size: Size::new(960.0, 600.0), writes: 0 };
    host.task(boot); host.message(Message::WindowFocused(true)); host.redraw(); host.redraw();
    let target = host.app.active_target().unwrap();
    let split_task = host.executor.enter(|| host.app.update(Message::Control(Control::Split(target, Direction::Right))));
    host.task(split_task); host.redraw(); host.redraw();
    assert_eq!(host.app.model().sessions().len(), 2);
    let active = host.app.active_session().unwrap(); let id = host.app.terminal(active).unwrap().widget_id().clone();
    let states = host.states(); assert!(states[&id].1);
    let mut rectangles: Vec<_> = states.values().map(|s| s.0).collect(); rectangles.sort_by(|a,b| a.x.total_cmp(&b.x));
    let point = Point::new(rectangles[0].x + rectangles[0].width + 3.0, rectangles[0].center_y());
    host.event(Event::Mouse(mouse::Event::CursorMoved { position: point }), Some(point));
    host.event(Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), Some(point));
    host.event(Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)), Some(point));
    println!("divider-tap focused={}", host.states()[&id].1);
    host.key(iced::keyboard::Key::Character("x".into()), iced::keyboard::Modifiers::empty(), Some("x"));
    println!("plain-key write-messages={}", host.writes);
    host.key(iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowLeft), iced::keyboard::Modifiers::ALT | iced::keyboard::Modifiers::SHIFT, None);
    println!("resize-key focused={}", host.states()[&id].1);
    host.message(Message::CloseWindow);
    println!("cleanup sessions={}", host.app.model().sessions().len());
    Ok(())
}
