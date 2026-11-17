//! Real App + public widget encoder + controlled native children, no user data.
//! Run: independent_splits FRESH_OUTPUT. Timers/counters exist only here.
#[path = "support/cursor_input.rs"]
mod fixture_input;

use iced::advanced::renderer::Headless;
use iced::advanced::widget::{
    Tree, operate,
    operation::{self, focusable},
};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::keyboard::{self, Modifiers};
use iced::{Element, Executor, Task};
use iced_term::{Terminal, TerminalView};
use pike::app::{App, Control, Direction, Message};
use pike::identity::SessionId;
use pike::session::Lifecycle;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--child") {
        return child(PathBuf::from(args.get(2).ok_or("missing output")?)).map_err(Into::into);
    }
    let output = std::path::absolute(args.get(1).ok_or("expected fresh output directory")?)?;
    fs::create_dir(&output)?;
    let bootstrap = pike::executor::SharedExecutor::new()?;
    let renderer = bootstrap
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or("public-widget fixture needs a graphics device")?;
    drop(bootstrap);
    let renderer = std::cell::RefCell::new(Some(renderer));
    let destination = output.clone();
    iced::application(
        move || Probe::new(destination.clone(), renderer.borrow_mut().take().unwrap()),
        Probe::update,
        Probe::view,
    )
    .executor::<pike::executor::SharedExecutor>()
    .subscription(|p: &Probe| p.app.subscription().map(ProbeMessage::App))
    .window_size((1100.0, 720.0))
    .title("Pike — independent split fixture")
    .exit_on_close_request(false)
    .run()?;
    if fs::read_to_string(output.join("completed.txt"))?.trim() != "passed" {
        return Err("split fixture failed; inspect events.txt".into());
    }
    Ok(())
}

fn child(output: PathBuf) -> io::Result<()> {
    let pid = std::process::id();
    let directory = output.join(pid.to_string());
    fs::create_dir(&directory)?;
    fixture_input::configure(Some(&directory))?;
    fs::write(
        directory.join("cwd.txt"),
        std::env::current_dir()?.to_string_lossy().as_bytes(),
    )?;
    fs::write(
        directory.join("profile.txt"),
        std::env::var("PIKE24_PROFILE").unwrap_or_default(),
    )?;
    for n in 0..100 {
        println!("H{pid}_{n}");
    }
    let counter_dir = directory.clone();
    std::thread::spawn(move || {
        for n in 0_u64.. {
            println!("PIKE24_PID={pid} T{n}");
            let _ = io::stdout().flush();
            let _ = fs::write(counter_dir.join("counter.txt"), n.to_string());
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    let mut bytes = [0; 7];
    io::stdin().read_exact(&mut bytes)?;
    fs::write(directory.join("input.bin"), bytes)?;
    let _command = fixture_input::command(&mut bytes.as_slice())?;
    println!("PIKE24_INPUT={pid}");
    io::stdout().flush()?;
    while io::stdin().read(&mut bytes)? != 0 {}
    Ok(())
}

struct Probe {
    app: App,
    output: PathBuf,
    renderer: iced::Renderer,
    ids: Vec<SessionId>,
    pids: BTreeMap<SessionId, u32>,
    stage: usize,
    counter: u64,
    history: usize,
    original_widget: Option<iced::widget::Id>,
    grid_measured: bool,
    sending: usize,
    input_started: bool,
    closing: bool,
    failed: bool,
}

enum ProbeMessage {
    App(Message),
    Focus(SessionId, bool),
    Screenshot(iced::window::Screenshot),
    Deadline,
}

impl Probe {
    fn new(output: PathBuf, renderer: iced::Renderer) -> (Self, Task<ProbeMessage>) {
        let profile = iced_term::settings::BackendSettings {
            program: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            args: vec!["--child".into(), output.to_string_lossy().into_owned()],
            env: [("PIKE24_PROFILE".into(), "inherited-split-profile".into())].into(),
            working_directory: Some(output.clone()),
            ..Default::default()
        };
        let (app, task) = App::with_settings(profile);
        (
            Self {
                app,
                output,
                renderer,
                ids: vec![],
                pids: BTreeMap::new(),
                stage: 0,
                counter: 0,
                history: 0,
                original_widget: None,
                grid_measured: false,
                sending: 0,
                input_started: false,
                closing: false,
                failed: false,
            },
            Task::batch([
                task.map(ProbeMessage::App),
                Task::perform(
                    async {
                        tokio::time::sleep(Duration::from_secs(15)).await;
                    },
                    |_| ProbeMessage::Deadline,
                ),
            ]),
        )
    }

    fn record(&self, line: impl AsRef<str>) {
        writeln!(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.output.join("events.txt"))
                .unwrap(),
            "{}",
            line.as_ref()
        )
        .unwrap();
    }

    fn close(&mut self, failure: Option<&str>) -> Task<ProbeMessage> {
        if let Some(reason) = failure {
            self.failed = true;
            self.record(reason);
        }
        self.closing = true;
        self.app.update(Message::CloseWindow).map(ProbeMessage::App)
    }

    fn focus(&mut self, id: SessionId) -> Task<ProbeMessage> {
        let target = self.app.model().target(id).unwrap();
        let widget = self.app.terminal(id).unwrap().widget_id().clone();
        self.app
            .update(Message::Control(Control::Focus(target)))
            .map(ProbeMessage::App)
            .chain(
                operate(focusable::is_focused(widget))
                    .map(move |focused| ProbeMessage::Focus(id, focused)),
            )
    }

    fn update(&mut self, message: ProbeMessage) -> Task<ProbeMessage> {
        let task = match message {
            ProbeMessage::Screenshot(shot) => {
                let mut image = io::BufWriter::new(
                    fs::File::create(self.output.join("four-panes.ppm")).unwrap(),
                );
                write!(image, "P6\n{} {}\n255\n", shot.size.width, shot.size.height).unwrap();
                for pixel in shot.rgba.chunks_exact(4) {
                    image.write_all(&pixel[..3]).unwrap();
                }
                image.flush().unwrap();
                return self.focus(self.ids[0]);
            }
            ProbeMessage::Deadline => return self.close(Some("fixture deadline expired")),
            ProbeMessage::Focus(id, focused) => {
                if self.closing {
                    return Task::none();
                }
                if !focused || self.app.active_session() != Some(id) {
                    self.record(format!(
                        "focus-mismatch session={} focused={focused} active={:?}",
                        id.get(),
                        self.app.active_session()
                    ));
                    return self.close(Some("actual application widget focus differs"));
                }
                self.record(format!(
                    "actual-widget-focus session={} exclusive=true",
                    id.get()
                ));
                // This harness injects synthetic input, including owning-window
                // focus. It does not assert physical desktop activation.
                drop(self.app.update(Message::WindowFocused(true)));
                let bytes = format!("p24_{:02}", self.sending);
                let events = encode(self.app.terminal(id).unwrap(), &self.renderer, &bytes);
                self.record(format!(
                    "public-widget-input session={} events={} focused={}",
                    id.get(),
                    events.len(),
                    self.app
                        .terminal(id)
                        .unwrap()
                        .renderable_content()
                        .unwrap()
                        .focused
                ));
                for event in events {
                    drop(self.app.update(Message::Terminal((id, event))));
                }
                self.record(format!(
                    "input-status session={} {:?}",
                    id.get(),
                    self.app.terminal(id).unwrap().input_status()
                ));
                self.sending += 1;
                if self.sending < self.ids.len() {
                    return self.focus(self.ids[self.sending]);
                }
                Task::none()
            }
            ProbeMessage::App(message) => {
                if matches!(&message, Message::GridSize(_, _)) {
                    self.grid_measured = true;
                }
                let done = matches!(&message, Message::ShutdownComplete);
                let task = self.app.update(message).map(ProbeMessage::App);
                if done {
                    self.record(format!(
                        "native-cleanup-acknowledged retained={}",
                        self.app.model().sessions().len()
                    ));
                    fs::write(
                        self.output.join("completed.txt"),
                        if self.failed { "failed\n" } else { "passed\n" },
                    )
                    .unwrap();
                    return task;
                }
                task
            }
        };
        if self.closing {
            return task;
        }
        let failed = self
            .app
            .model()
            .sessions()
            .iter()
            .any(|(_, entry)| matches!(entry.lifecycle(), Lifecycle::Failed(_)));
        if failed {
            return self.close(Some("unexpected launch failure"));
        }
        for (id, _) in self.app.model().sessions().iter() {
            if self.ids.contains(&id) {
                continue;
            }
            let Some(terminal) = self.app.terminal(id) else {
                continue;
            };
            let Some(content) = terminal.renderable_content() else {
                continue;
            };
            let text: String = content.viewport.cells().iter().map(|cell| cell.c).collect();
            let Some(pid) = text
                .split("PIKE24_PID=")
                .nth(1)
                .and_then(|tail| tail.split(' ').next())
                .and_then(|pid| pid.parse::<u32>().ok())
            else {
                continue;
            };
            self.record(format!(
                "attached session={} pid={pid} visible={}",
                id.get(),
                terminal.is_visible()
            ));
            if self.ids.is_empty() {
                self.original_widget = Some(terminal.widget_id().clone());
                self.history = terminal.snapshot_stats().history_lines;
            }
            self.ids.push(id);
            self.pids.insert(id, pid);
        }
        if !self.grid_measured || self.ids.len() != self.stage + 1 {
            return task;
        }
        let first = self.ids[0];
        let original = self.app.terminal(first).unwrap();
        let counter = fs::read_to_string(
            self.output
                .join(self.pids[&first].to_string())
                .join("counter.txt"),
        )
        .ok()
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap_or(0);
        if counter <= self.counter {
            return task;
        }
        if original.widget_id() != self.original_widget.as_ref().unwrap()
            || original.snapshot_stats().history_lines < self.history
        {
            return self.close(Some("original identity/history changed"));
        }
        if self.stage < 3 {
            self.record(format!(
                "counter-continuity before-split={} counter={counter} history={}",
                self.stage + 1,
                original.snapshot_stats().history_lines
            ));
            self.counter = counter;
            let direction = [Direction::Right, Direction::Down, Direction::Down][self.stage];
            let target = if self.stage == 2 {
                self.app.model().target(first).unwrap()
            } else {
                self.app.active_target().unwrap()
            };
            self.stage += 1;
            let select = self
                .app
                .update(Message::Control(Control::Focus(target)))
                .map(ProbeMessage::App);
            let message = press_split(&self.app, &self.renderer, direction);
            self.record(format!(
                "public-app-button direction={direction:?} source={}",
                self.app.active_session().unwrap().get()
            ));
            return Task::batch([
                task,
                select,
                self.app.update(message).map(ProbeMessage::App),
            ]);
        }
        if !self.input_started {
            self.input_started = true;
            return Task::batch([
                task,
                iced::window::oldest()
                    .and_then(iced::window::screenshot)
                    .map(ProbeMessage::Screenshot),
            ]);
        }
        if self.sending != 4 {
            return task;
        }
        let mut all = true;
        for (index, id) in self.ids.iter().enumerate() {
            let directory = self.output.join(self.pids[id].to_string());
            let Ok(bytes) = fs::read(directory.join("input.bin")) else {
                all = false;
                continue;
            };
            let expected = format!("p24_{index:02}{}", if cfg!(windows) { '\r' } else { '\n' });
            if bytes != expected.as_bytes()
                || fs::read_to_string(directory.join("profile.txt")).unwrap()
                    != "inherited-split-profile"
                || fs::read_to_string(directory.join("cwd.txt")).unwrap()
                    != self.output.to_string_lossy()
            {
                return self.close(Some("input/profile/launch-cwd differs"));
            }
        }
        if all {
            if self.pids.values().copied().collect::<BTreeSet<_>>().len() != 4 {
                return self.close(Some("children are not distinct"));
            }
            self.record(format!("passed distinct-sessions=4 distinct-processes=4 exact-inputs=4 inherited-cwd-profile=4 original-counter={counter} original-history={}", original.snapshot_stats().history_lines));
            return Task::batch([task, self.close(None)]);
        }
        task
    }

    fn view(&self) -> Element<'_, ProbeMessage> {
        self.app.view().map(ProbeMessage::App)
    }
}

fn press_split(app: &App, renderer: &iced::Renderer, direction: Direction) -> Message {
    let mut view = app.view();
    let mut tree = Tree::new(view.as_widget());
    let size = iced::Size::new(1100.0, 720.0);
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &layout::Limits::new(size, size));
    let root = Layout::new(&node);
    let column = root.children().next().unwrap();
    let toolbar = column.children().next().unwrap();
    let button = toolbar
        .children()
        .nth(if direction == Direction::Right { 1 } else { 2 })
        .unwrap();
    let position = button.bounds().center();
    let mut messages = vec![];
    for event in [
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        view.as_widget_mut().update(
            &mut tree,
            &iced::Event::Mouse(event),
            root,
            mouse::Cursor::Available(position),
            renderer,
            &mut clipboard::Null,
            &mut Shell::new(&mut messages),
            &iced::Rectangle::with_size(size),
        );
    }
    messages
        .into_iter()
        .find(|message| {
            matches!(message,
        Message::Control(Control::Split(_, actual)) if *actual == direction)
        })
        .expect("actual app split button must publish the requested action")
}

/// Encode synthetic physical-like key events through the public widget API.
/// The actual application focus operation is checked separately above.
fn encode(terminal: &Terminal, renderer: &iced::Renderer, text: &str) -> Vec<iced_term::Event> {
    let mut view = TerminalView::show(terminal);
    let mut tree = Tree::new(view.as_widget());
    let size = iced::Size::new(400.0, 240.0);
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &layout::Limits::new(size, size));
    view.as_widget_mut().operate(
        &mut tree,
        Layout::new(&node),
        renderer,
        &mut operation::focusable::focus::<()>(terminal.widget_id().clone()),
    );
    let mut messages = vec![];
    for character in text.chars().map(Some).chain([None]) {
        let key = character
            .map(|c| keyboard::Key::Character(c.to_string().into()))
            .unwrap_or(keyboard::Key::Named(keyboard::key::Named::Enter));
        let event = iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
            text: character.map(|c| c.to_string().into()),
            repeat: false,
        });
        view.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut clipboard::Null,
            &mut Shell::new(&mut messages),
            &iced::Rectangle::with_size(size),
        );
    }
    // This helper's local layout must not resize the application's real pane.
    messages.retain(|event| {
        !matches!(
            event,
            iced_term::Event::BackendCall(_, iced_term::BackendCommand::Resize(..))
        )
    });
    messages
}
