//! Finite real App / native-child geometry probe. All timers and traces are
//! fixture-only. Synthetic events and application zoom are not physical input/DPI.
#[path = "support/cursor_input.rs"]
mod fixture_input;
#[path = "pane_resizing/widget.rs"]
mod widget;

use iced::advanced::{renderer::Headless, widget::Tree};
use iced::{Element, Executor, Size, Task};
use iced_term::{
    BackendCommand, Event,
    settings::{BackendSettings, FontSettings},
};
use pike::{
    app::{App, Control, Direction, Message},
    identity::SessionId,
    session::Lifecycle,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

static FRAME_TRACE: std::sync::OnceLock<(PathBuf, Instant, std::sync::Mutex<Option<Instant>>)> =
    std::sync::OnceLock::new();
static CURRENT_CASE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

// Observe actual redraw delivery without emitting an app message or requesting
// another frame. This avoids an observer-driven redraw loop in the fixture.
fn trace_frame(
    event: iced::Event,
    _: iced::event::Status,
    _: iced::window::Id,
) -> Option<ProbeMessage> {
    if let iced::Event::Window(iced::window::Event::RedrawRequested(at)) = event {
        let (path, start, previous) = FRAME_TRACE.get().unwrap();
        let now = Instant::now();
        let interval = previous
            .lock()
            .unwrap()
            .replace(at)
            .map(|prior| at.saturating_duration_since(prior).as_micros())
            .unwrap_or(0);
        writeln!(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap(),
            "{},{},{},{}",
            now.duration_since(*start).as_micros(),
            CURRENT_CASE.load(std::sync::atomic::Ordering::Relaxed),
            interval,
            now.saturating_duration_since(at).as_micros()
        )
        .unwrap();
    }
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--child") {
        return child(PathBuf::from(args.get(2).ok_or("missing output")?)).map_err(Into::into);
    }
    let output = std::path::absolute(args.get(1).ok_or("expected fresh output directory")?)?;
    fs::create_dir(&output)?;
    FRAME_TRACE
        .set((
            output.join("frames.csv"),
            Instant::now(),
            std::sync::Mutex::new(None),
        ))
        .unwrap();
    let executor = pike::executor::SharedExecutor::new()?;
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or("graphics device unavailable")?;
    drop(executor);
    let renderer = std::cell::RefCell::new(Some(renderer));
    let destination = output.clone();
    iced::application(
        move || Probe::new(destination.clone(), renderer.borrow_mut().take().unwrap()),
        Probe::update,
        Probe::view,
    )
    .executor::<pike::executor::SharedExecutor>()
    .subscription(|probe: &Probe| {
        iced::Subscription::batch([
            probe.app.subscription().map(ProbeMessage::App),
            iced::event::listen_raw(trace_frame),
        ])
    })
    .scale_factor(|probe: &Probe| probe.scale)
    .window_size((1100.0, 720.0))
    .title("Pike — pane resizing fixture")
    .exit_on_close_request(false)
    .run()?;
    if fs::read_to_string(output.join("completed.txt"))?.trim() != "passed" {
        return Err("fixture failed; inspect events.txt".into());
    }
    Ok(())
}

fn child(output: PathBuf) -> io::Result<()> {
    let pid = std::process::id();
    let directory = output.join(pid.to_string());
    fs::create_dir(&directory)?;
    fixture_input::configure(Some(&directory))?;
    let writer_dir = directory.clone();
    // Finite output producer, terminated by the existing backend close path.
    std::thread::spawn(move || {
        for n in 0..1200 {
            println!("PID={pid} T={n}");
            let _ = io::stdout().flush();
            let _ = fs::write(writer_dir.join("counter.txt"), n.to_string());
            std::thread::sleep(Duration::from_millis(30));
        }
    });
    while let Some(command) = fixture_input::command(&mut io::stdin())? {
        if !command.starts_with("q ") {
            return Err(io::Error::other(
                "unexpected child input (possible shortcut leakage)",
            ));
        }
        let (cols, rows) = native_size()?;
        writeln!(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(directory.join("sizes.csv"))?,
            "{},{cols},{rows}",
            &command[2..]
        )?;
    }
    Ok(())
}

#[cfg(unix)]
fn native_size() -> io::Result<(u16, u16)> {
    let result = std::process::Command::new("/bin/stty")
        .arg("size")
        .stdin(std::process::Stdio::inherit())
        .output()?;
    let values = String::from_utf8_lossy(&result.stdout)
        .split_whitespace()
        .map(str::parse::<u16>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(io::Error::other)?;
    if !result.status.success() || values.len() != 2 {
        return Err(io::Error::other("stty size failed"));
    }
    Ok((values[1], values[0]))
}

#[cfg(windows)]
fn native_size() -> io::Result<(u16, u16)> {
    use windows_sys::Win32::System::Console::{
        CONSOLE_SCREEN_BUFFER_INFO, GetConsoleScreenBufferInfo, GetStdHandle, STD_OUTPUT_HANDLE,
    };
    // SAFETY: This console geometry record contains only integer fields, for
    // which the all-zero bit pattern is valid, and is overwritten by the API.
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: Borrow the controlled child's console handle and write to an
    // initialized stack record; neither ownership nor handle lifetime changes.
    if unsafe { GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((
        info.dwSize.X as u16,
        (info.srWindow.Bottom - info.srWindow.Top + 1) as u16,
    ))
}

enum ProbeMessage {
    App(Message),
    Tick,
    Sample,
    Size(Size),
    Verify,
    FocusAfterDrag(bool),
    Screenshot(iced::window::Screenshot),
    Frame,
    Deadline,
}

struct Probe {
    app: App,
    output: PathBuf,
    renderer: iced::Renderer,
    tree: Tree,
    start: Instant,
    scale: f32,
    size: Size,
    ids: Vec<SessionId>,
    pids: BTreeMap<SessionId, u32>,
    widgets: BTreeMap<SessionId, iced::widget::Id>,
    case: usize,
    expected: BTreeMap<SessionId, (usize, usize)>,
    counts: BTreeMap<SessionId, u64>,
    canonical: Option<String>,
    closing: bool,
    failed: bool,
}

impl Probe {
    fn new(output: PathBuf, renderer: iced::Renderer) -> (Self, Task<ProbeMessage>) {
        let (app, task) = App::with_settings(BackendSettings {
            program: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            args: vec!["--child".into(), output.to_string_lossy().into_owned()],
            ..Default::default()
        });
        (
            Self {
                app,
                output,
                renderer,
                tree: Tree::empty(),
                start: FRAME_TRACE.get().unwrap().1,
                scale: 1.0,
                size: Size::new(1100.0, 720.0),
                ids: vec![],
                pids: BTreeMap::new(),
                widgets: BTreeMap::new(),
                case: 0,
                expected: BTreeMap::new(),
                counts: BTreeMap::new(),
                canonical: None,
                closing: false,
                failed: false,
            },
            Task::batch([
                task.map(ProbeMessage::App),
                delay(250, ProbeMessage::Tick),
                delay(30_000, ProbeMessage::Deadline),
            ]),
        )
    }
    fn record(&self, file: &str, line: impl std::fmt::Display) {
        writeln!(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.output.join(file))
                .unwrap(),
            "{line}"
        )
        .unwrap();
    }
    fn apply(&mut self, message: Message) -> Task<ProbeMessage> {
        let tag = match &message {
            Message::Terminal((_, Event::BackendCall(_, BackendCommand::Resize(..)))) => "resize",
            Message::Terminal(_) => "terminal",
            Message::GridSize(..) => "layout",
            Message::Control(_) => "control",
            _ => "lifecycle",
        };
        let start = Instant::now();
        let task = self.app.update(message).map(ProbeMessage::App);
        let micros = start.elapsed().as_micros();
        self.record(
            "dispatch.csv",
            format_args!(
                "{},{},{tag},{micros}",
                self.start.elapsed().as_micros(),
                self.case
            ),
        );
        for id in &self.ids {
            if let Some(terminal) = self.app.terminal(*id) {
                let count = terminal.work_stats().pty_resizes;
                if self.counts.insert(*id, count) != Some(count) {
                    self.record(
                        "resize.csv",
                        format_args!(
                            "{},{},{},{count}",
                            self.start.elapsed().as_micros(),
                            self.case,
                            id.get()
                        ),
                    );
                }
            }
        }
        task
    }
    fn close(&mut self, failure: Option<&str>) -> Task<ProbeMessage> {
        if let Some(reason) = failure {
            self.failed = true;
            self.record("events.txt", reason);
        }
        self.closing = true;
        let request = self.apply(Message::CloseWindow);
        let confirm = self.app.close_confirmation().map(|(token, _)| {
            self.apply(Message::Control(Control::CloseAction(
                pike::app::CloseAction::Confirm(token),
            )))
        });
        Task::batch([request, confirm.unwrap_or_else(Task::none)])
    }
    fn update(&mut self, message: ProbeMessage) -> Task<ProbeMessage> {
        match message {
            ProbeMessage::Frame => Task::none(),
            ProbeMessage::Deadline if !self.closing => {
                self.close(Some("30 second deadline expired"))
            }
            ProbeMessage::Deadline => Task::none(),
            ProbeMessage::App(message) => {
                let done = matches!(&message, Message::ShutdownComplete);
                let task = self.apply(message);
                if done {
                    fs::write(
                        self.output.join("completed.txt"),
                        if self.failed { "failed\n" } else { "passed\n" },
                    )
                    .unwrap();
                }
                task
            }
            ProbeMessage::Tick => self.tick(),
            ProbeMessage::Sample => iced::window::oldest()
                .and_then(iced::window::size)
                .map(ProbeMessage::Size),
            ProbeMessage::Size(size) => self.sample(size),
            ProbeMessage::Verify => self.verify(),
            ProbeMessage::FocusAfterDrag(focused) => {
                if !focused {
                    self.close(Some("actual terminal widget focus lost after divider drag"))
                } else {
                    self.record(
                        "events.txt",
                        "actual widget focus restored after divider drag",
                    );
                    delay(1, ProbeMessage::Sample)
                }
            }
            ProbeMessage::Screenshot(shot) => {
                let mut file = io::BufWriter::new(
                    fs::File::create(self.output.join("constrained.ppm")).unwrap(),
                );
                write!(file, "P6\n{} {}\n255\n", shot.size.width, shot.size.height).unwrap();
                for pixel in shot.rgba.chunks_exact(4) {
                    file.write_all(&pixel[..3]).unwrap();
                }
                file.flush().unwrap();
                delay(50, ProbeMessage::Tick)
            }
        }
    }
    fn tick(&mut self) -> Task<ProbeMessage> {
        if self.closing {
            return Task::none();
        }
        if self
            .app
            .model()
            .sessions()
            .iter()
            .any(|(_, e)| matches!(e.lifecycle(), Lifecycle::Failed(_)))
        {
            return self.close(Some("launch failure"));
        }
        for (id, _) in self.app.model().sessions().iter() {
            if self.pids.contains_key(&id) {
                continue;
            }
            let Some(terminal) = self.app.terminal(id) else {
                continue;
            };
            let Some(content) = terminal.renderable_content() else {
                continue;
            };
            let text: String = content.viewport.cells().iter().map(|c| c.c).collect();
            if let Some(pid) = text
                .split("PID=")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse().ok())
            {
                self.ids.push(id);
                self.pids.insert(id, pid);
                self.widgets.insert(id, terminal.widget_id().clone());
                self.record("events.txt", format_args!("session={} pid={pid}", id.get()));
            }
        }
        if self.ids.len() != self.app.model().sessions().len() || self.ids.is_empty() {
            return delay(150, ProbeMessage::Tick);
        }
        if self.ids.len() < 4 {
            let target = if self.ids.len() == 3 {
                self.app.model().target(self.ids[0]).unwrap()
            } else {
                self.app.active_target().unwrap()
            };
            let direction = if self.ids.len() == 1 {
                Direction::Right
            } else {
                Direction::Down
            };
            let task = self.apply(Message::Control(Control::Split(target, direction)));
            return Task::batch([task, delay(250, ProbeMessage::Tick)]);
        }
        self.record("events.txt", format_args!("begin-case={}", self.case));
        CURRENT_CASE.store(self.case, std::sync::atomic::Ordering::Relaxed);
        match self.case {
            0 => widget::prime(&self.app, &self.renderer, &mut self.tree, self.size),
            1 | 2 => {
                let mut tasks = vec![];
                if self.case == 1 {
                    for outside in [false, true] {
                        let tap = widget::tap(
                            &self.app,
                            &self.renderer,
                            &mut self.tree,
                            self.size,
                            outside,
                        );
                        if tap
                            .iter()
                            .any(|m| matches!(m, Message::Control(Control::Resize(..))))
                            || !widget::focused(
                                &self.app,
                                &self.renderer,
                                &mut self.tree,
                                self.size,
                            )
                        {
                            return self
                                .close(Some("no-motion divider tap lost persistent widget focus"));
                        }
                        tasks.extend(tap.into_iter().map(|message| self.apply(message)));
                    }
                    let typing = widget::text(
                        &self.app,
                        &self.renderer,
                        &mut self.tree,
                        self.size,
                        "q tap",
                    );
                    if !typing.iter().any(|m| {
                        matches!(
                            m,
                            Message::Terminal((_, Event::BackendCall(_, BackendCommand::Write(_))))
                        )
                    }) {
                        return self
                            .close(Some("typing after divider tap produced no shell input"));
                    }
                    tasks.extend(typing.into_iter().map(|message| self.apply(message)));
                    self.record("events.txt", "persistent no-motion tap and outside release retained focus; widget query sent without refocus");
                }
                let messages = widget::drag(
                    &self.app,
                    &self.renderer,
                    &mut self.tree,
                    self.size,
                    self.case == 2,
                );
                if !messages
                    .iter()
                    .any(|m| matches!(m, Message::Control(Control::Resize(..))))
                {
                    return self.close(Some("actual pane_grid drag did not publish resize"));
                }
                if !widget::focused(&self.app, &self.renderer, &mut self.tree, self.size) {
                    return self.close(Some("persistent widget lost focus after drag/release"));
                }
                tasks.extend(messages.into_iter().map(|message| self.apply(message)));
                let widget = self
                    .app
                    .terminal(self.app.active_session().unwrap())
                    .unwrap()
                    .widget_id()
                    .clone();
                return Task::batch(tasks)
                    .chain(delay(350, ProbeMessage::Frame))
                    .chain(
                        iced::advanced::widget::operate(
                            iced::advanced::widget::operation::focusable::is_focused(widget),
                        )
                        .map(ProbeMessage::FocusAfterDrag),
                    );
            }
            3 | 4 => {
                drop(self.apply(Message::WindowFocused(true)));
                if self.case == 3
                    && !widget::blur_priority(&self.app, &self.renderer, &mut self.tree, self.size)
                {
                    return self.close(Some("same-view blur allowed resize or shell input"));
                }
                drop(self.apply(Message::WindowFocused(true)));
                if self.case == 3
                    && !widget::composition_priority(
                        &self.app,
                        &self.renderer,
                        &mut self.tree,
                        self.size,
                    )
                {
                    return self.close(Some(
                        "live IME ownership did not retain priority through empty preedit/commit",
                    ));
                }
                let messages = widget::key(
                    &self.app,
                    &self.renderer,
                    &mut self.tree,
                    self.size,
                    self.case == 4,
                );
                if messages
                    .iter()
                    .filter(|m| matches!(m, Message::Control(Control::ResizeKey(_, _))))
                    .count()
                    != 1
                    || messages.iter().any(|m| {
                        matches!(
                            m,
                            Message::Terminal((_, Event::BackendCall(_, BackendCommand::Write(_))))
                        )
                    })
                {
                    return self.close(Some("resize key leaked or was not captured before child"));
                }
                for message in messages {
                    drop(self.apply(message));
                }
                if self.case == 4 {
                    self.canonical = Some(self.canonical());
                }
            }
            5 => {
                for id in self.ids.clone() {
                    self.app.change_terminal_font(
                        id,
                        FontSettings {
                            size: 18.0,
                            ..Default::default()
                        },
                    );
                }
            }
            6 => self.scale = 1.25,
            7 => self.scale = 1.5,
            8 => self.scale = 2.0,
            9 => {
                self.scale = 1.0;
                return Task::batch([
                    resize(Size::new(320.0, 240.0)),
                    delay(500, ProbeMessage::Sample),
                ]);
            }
            10 => {
                let tab = self.app.model().workspace().active().unwrap();
                drop(self.apply(Message::GridSize(tab, Size::ZERO)));
                let restore = delay(250, ProbeMessage::Sample);
                return Task::batch([
                    iced::window::oldest().and_then(|id| iced::window::minimize(id, true)),
                    restore,
                ]);
            }
            11 => {
                for id in self.ids.clone() {
                    self.app.change_terminal_font(id, FontSettings::default());
                }
                return Task::batch([
                    iced::window::oldest().and_then(|id| iced::window::minimize(id, false)),
                    resize(Size::new(1100.0, 720.0)),
                    delay(500, ProbeMessage::Sample),
                ]);
            }
            12 => {
                return Task::batch([
                    iced::window::oldest().and_then(|id| iced::window::maximize(id, true)),
                    delay(500, ProbeMessage::Sample),
                ]);
            }
            13 => {
                return Task::batch([
                    iced::window::oldest().and_then(|id| iced::window::maximize(id, false)),
                    resize(Size::new(1100.0, 720.0)),
                    delay(500, ProbeMessage::Sample),
                ]);
            }
            _ => return self.close(None),
        }
        delay(400, ProbeMessage::Sample)
    }
    fn canonical(&self) -> String {
        format!(
            "{:?}",
            self.app
                .model()
                .workspace()
                .tab(self.app.model().workspace().active().unwrap())
                .unwrap()
                .grid()
                .layout()
        )
    }
    fn sample(&mut self, size: Size) -> Task<ProbeMessage> {
        self.size = size;
        // Synthetic admission lets the controlled query run while minimized;
        // this does not claim physical window focus or keyboard verification.
        drop(self.apply(Message::WindowFocused(true)));
        let bounds = widget::bounds(&self.app, &self.renderer, &mut self.tree, size);
        self.expected.clear();
        let mut tasks = vec![];
        for id in self.ids.clone() {
            let terminal = self.app.terminal(id).unwrap();
            let cell = terminal.renderable_content().unwrap().terminal_size;
            let Some(bounds) = bounds.get(terminal.widget_id()) else {
                return self.close(Some("actual terminal widget bounds absent"));
            };
            let columns = (f64::from(bounds.width) / f64::from(cell.cell_width))
                .floor()
                .min(512.0) as usize;
            let rows = (f64::from(bounds.height) / f64::from(cell.cell_height))
                .floor()
                .min(512.0) as usize;
            self.expected.insert(id, (columns, rows));
            self.record(
                "geometry.csv",
                format_args!(
                    "{},{},{},{},{},{},{},{},{},{}",
                    self.case,
                    self.scale,
                    id.get(),
                    self.pids[&id],
                    bounds.width,
                    bounds.height,
                    cell.cell_width,
                    cell.cell_height,
                    columns,
                    rows
                ),
            );
            tasks.push(self.apply(Message::Control(Control::Focus(
                self.app.model().target(id).unwrap(),
            ))));
            // Independent controlled child reports native dimensions after this
            // admitted command. No adapter counters are used as expected values.
            let suffix = if cfg!(windows) { '\r' } else { '\n' };
            tasks.push(self.apply(Message::Terminal((
                id,
                Event::BackendCall(
                    id.get(),
                    BackendCommand::Write(format!("q {}{suffix}", self.case).into_bytes()),
                ),
            ))));
        }
        tasks.push(delay(250, ProbeMessage::Verify));
        Task::batch(tasks)
    }
    fn verify(&mut self) -> Task<ProbeMessage> {
        if self.case == 1 {
            let id = self.ids.last().unwrap();
            let text = fs::read_to_string(
                self.output
                    .join(self.pids[id].to_string())
                    .join("sizes.csv"),
            )
            .unwrap_or_default();
            if !text.lines().any(|line| line.starts_with("tap,")) {
                return self.close(Some("child did not accept widget typing after divider tap"));
            }
            self.record(
                "events.txt",
                "native child acknowledged public-widget query after divider tap",
            );
        }
        for id in &self.ids {
            let terminal = self.app.terminal(*id).unwrap();
            if terminal.widget_id() != &self.widgets[id] || terminal.input_status().closed {
                return self.close(Some("session/widget/process continuity lost"));
            }
            let path = self
                .output
                .join(self.pids[id].to_string())
                .join("sizes.csv");
            let text = fs::read_to_string(path).unwrap_or_default();
            let line = text
                .lines()
                .find(|line| line.starts_with(&format!("{},", self.case)));
            let Some(line) = line else {
                return self.close(Some(
                    "child did not acknowledge query (or received shortcut bytes)",
                ));
            };
            let expected = self.expected[id];
            if line != format!("{},{},{}", self.case, expected.0, expected.1) {
                self.record(
                    "events.txt",
                    format_args!("case={} observed={line} expected={expected:?}", self.case),
                );
                return self.close(Some(
                    "native child dimensions differ from actual widget cell geometry",
                ));
            }
            if expected.0 < 20 || expected.1 < 5 {
                return self.close(Some("presentation fell below20x5"));
            }
        }
        // Repeated equivalent valid geometry and minimize/invalid proposals
        // must never reach the native notifier as additional resize work.
        for id in self.ids.clone() {
            let terminal = self.app.terminal(id).unwrap();
            let before = terminal.work_stats().pty_resizes;
            let bounds = widget::bounds(&self.app, &self.renderer, &mut self.tree, self.size)
                [terminal.widget_id()];
            for _ in 0..20 {
                drop(self.apply(Message::Terminal((
                    id,
                    Event::BackendCall(id.get(), BackendCommand::Resize(Some(bounds.size()), None)),
                ))));
            }
            if self.app.terminal(id).unwrap().work_stats().pty_resizes != before {
                return self.close(Some("identical layout caused native resize storm"));
            }
        }
        if self.case > 4 && self.canonical.as_ref() != Some(&self.canonical()) {
            return self.close(Some("window/font/DPI changed canonical ratios"));
        }
        self.record(
            "events.txt",
            format_args!(
                "passed-case={} exact-native-dimensions=4 identities=4",
                self.case
            ),
        );
        let screenshot = self.case == 9;
        self.case += 1;
        if screenshot {
            iced::window::oldest()
                .and_then(iced::window::screenshot)
                .map(ProbeMessage::Screenshot)
        } else {
            delay(50, ProbeMessage::Tick)
        }
    }
    fn view(&self) -> Element<'_, ProbeMessage> {
        self.app.view().map(ProbeMessage::App)
    }
}

fn delay(ms: u64, message: ProbeMessage) -> Task<ProbeMessage> {
    Task::perform(
        async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            message
        },
        |message| message,
    )
}
fn resize(size: Size) -> Task<ProbeMessage> {
    iced::window::oldest().and_then(move |id| iced::window::resize(id, size))
}
