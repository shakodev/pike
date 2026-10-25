#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod options;
#[path = "../support/protocol.rs"]
mod protocol;
mod report;

use iced::widget::{PaneGrid, button, column, container, pane_grid, row, text};
use iced::{Element, Fill, Subscription, Task, window};
use iced_term::{AlacrittyEvent, BackendCommand, Command, Terminal, TerminalView};
use options::{Mode, Options};
use protocol::{Marker, ProbeState, route_marker};
use std::collections::BTreeMap;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::parse()?;
    iced::application(
        move || App::new(options.clone()).map_err(|error| error.to_string()),
        update,
        view,
    )
    .executor::<pike::executor::SharedExecutor>()
    .title("Pike compatibility harness")
    .window_size((960.0, 640.0))
    .exit_on_close_request(false)
    .subscription(|state: &Result<App, String>| match state {
        Ok(app) => app.subscription(),
        Err(_) => window::close_requests().map(|_| Message::Close),
    })
    .run()?;
    Ok(())
}

fn update(state: &mut Result<App, String>, message: Message) -> Task<Message> {
    match state {
        Ok(app) => app.update(message),
        Err(_) => iced::exit(),
    }
}

fn view(state: &Result<App, String>) -> Element<'_, Message> {
    match state {
        Ok(app) => app.view(),
        Err(error) => text(format!("Harness startup failed: {error}")).into(),
    }
}

struct App {
    options: Options,
    panes: pane_grid::State<u64>,
    terminals: BTreeMap<u64, Terminal>,
    states: BTreeMap<u64, ProbeState>,
    focused: u64,
    report: report::Report,
    automated: bool,
    closing: bool,
    error: Option<String>,
}

#[derive(Clone, Debug)]
enum Message {
    Terminal(iced_term::Event),
    Focus(pane_grid::Pane),
    Resize(pane_grid::ResizeEvent),
    Swap,
    WindowResize,
    Write(&'static str),
    Step(u8),
    Captured(&'static str, window::Screenshot),
    Close,
    Timeout,
    Failed(&'static str),
}

impl App {
    fn new(options: Options) -> std::io::Result<Self> {
        let mut report = report::Report::new(&options.output)?;
        let fixture = std::env::current_exe()?.with_file_name("pty_fixture.exe");
        if !fixture.is_file() {
            return Err(std::io::Error::other(
                "build --examples first: pty_fixture.exe is missing",
            ));
        }
        let (mut panes, first) = pane_grid::State::new(1);
        if options.sessions == 2 {
            panes.split(pane_grid::Axis::Vertical, first, 2);
        }
        let mut terminals = BTreeMap::new();
        let mut states = BTreeMap::new();
        for id in 1..=options.sessions {
            let (program, args) = options::launch(&options.shell, &fixture, id)?;
            let terminal = Terminal::new(
                id,
                iced_term::settings::Settings {
                    backend: iced_term::settings::BackendSettings {
                        program,
                        args,
                        working_directory: Some(std::env::current_dir()?),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )?;
            report.record("spawned", id, 0, 0)?;
            terminals.insert(id, terminal);
            states.insert(id, ProbeState::default());
        }
        Ok(Self {
            options,
            panes,
            terminals,
            states,
            focused: 1,
            report,
            automated: false,
            closing: false,
            error: None,
        })
    }

    fn note(&mut self, event: &str, id: u64, a: impl std::fmt::Display, b: impl std::fmt::Display) {
        if let Err(error) = self.report.record(event, id, a, b) {
            self.error = Some(error.to_string());
        }
    }

    fn focus(&mut self, id: u64) -> Task<Message> {
        self.focused = id;
        self.terminals
            .get(&id)
            .map(|term| TerminalView::focus(term.widget_id().clone()))
            .unwrap_or_else(Task::none)
    }

    fn write(&mut self, id: u64, command: &'static str) {
        if let Some(term) = self.terminals.get_mut(&id) {
            term.handle(Command::ProxyToBackend(BackendCommand::Write(
                command.as_bytes().to_vec(),
            )));
        }
    }

    fn capture(&self, name: &'static str) -> Task<Message> {
        window::latest().and_then(move |id| {
            window::screenshot(id).map(move |shot| Message::Captured(name, shot))
        })
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Terminal(iced_term::Event::BackendCall(id, command)) => {
                if !self.terminals.contains_key(&id) {
                    return Task::none();
                }
                match &command {
                    BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Title(title)) => {
                        if let Some(marker) = route_marker(&mut self.states, id, title) {
                            match marker {
                                Marker::Ready(_, pid) => {
                                    self.note("ready", id, pid, 0);
                                    self.write(id, "size\r");
                                }
                                Marker::Size(_, cols, rows) => self.note("size", id, cols, rows),
                                Marker::Final(_) => self.note("final", id, 0, 0),
                            }
                        }
                    }
                    BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::ChildExit(code)) => {
                        if let Some(state) = self.states.get_mut(&id) {
                            state.exit_code = Some(*code);
                        }
                        self.note("child_exit", id, code, 0);
                    }
                    BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup) => {
                        if let Some(state) = self.states.get_mut(&id) {
                            state.wakeups = state.wakeups.saturating_add(1);
                        }
                    }
                    BackendCommand::Resize(Some(size), _) => {
                        if let Some(state) = self.states.get_mut(&id) {
                            state.resizes = state.resizes.saturating_add(1);
                        }
                        self.note("layout", id, size.width, size.height);
                    }
                    BackendCommand::ProcessAlacrittyEvent(event) => {
                        let kind = match event {
                            AlacrittyEvent::ColorRequest(..) => Some("color_query"),
                            AlacrittyEvent::TextAreaSizeRequest(..) => Some("size_query"),
                            AlacrittyEvent::ResetTitle => Some("reset_title"),
                            AlacrittyEvent::Bell => Some("bell"),
                            _ => None,
                        };
                        if let Some(kind) = kind {
                            self.note(kind, id, 0, 0);
                        }
                    }
                    _ => {}
                }
                let action = self
                    .terminals
                    .get_mut(&id)
                    .map(|term| term.handle(Command::ProxyToBackend(command)));
                if action == Some(iced_term::actions::Action::Shutdown) {
                    self.note("exit", id, self.focused, self.states[&id].final_seen);
                    self.terminals.remove(&id); // Cancel this session's broken upstream EOF stream.
                    if self.terminals.is_empty() {
                        return self.finish();
                    }
                    if self.options.mode == Mode::Smoke && id == 2 {
                        return later(Message::Step(7));
                    }
                }
                if !self.automated && self.states.values().all(|state| state.pid.is_some()) {
                    self.automated = true;
                    let focus = self.focus(1);
                    if self.options.mode == Mode::Smoke {
                        for id in 1..=self.options.sessions {
                            self.write(id, "render\r");
                        }
                        return Task::batch([focus, later(Message::Step(0))]);
                    }
                    return focus;
                }
            }
            Message::Focus(pane) => {
                if let Some(id) = self.panes.get(pane) {
                    return self.focus(*id);
                }
            }
            Message::Resize(event) => self.panes.resize(event.split, event.ratio.clamp(0.1, 0.9)),
            Message::Swap => {
                let panes: Vec<_> = self.panes.iter().map(|(pane, _)| *pane).collect();
                if panes.len() == 2 {
                    self.panes.swap(panes[0], panes[1]);
                    self.note("swap", 0, 1, 2);
                    return self.focus(2);
                }
            }
            Message::WindowResize => {
                return window::latest()
                    .and_then(|id| window::resize(id, iced::Size::new(1100.0, 700.0)));
            }
            Message::Write(command) => {
                for id in 1..=self.options.sessions {
                    self.write(id, command);
                }
            }
            Message::Step(step) => match step {
                0 => return self.capture("before"),
                1 => return Task::batch([self.update(Message::Swap), later(Message::Step(2))]),
                2 => return self.capture("after"),
                3 => {
                    return Task::batch([
                        self.update(Message::WindowResize),
                        later(Message::Step(4)),
                    ]);
                }
                4 => {
                    return Task::batch([
                        self.update(Message::Write("size\r")),
                        later(Message::Step(5)),
                    ]);
                }
                5 => {
                    // Native ConPTY may consume/reply to VT queries itself.
                    // Keep query probes explicit so their input responses do
                    // not contaminate the fixture's normal exit command.
                    return later(Message::Step(6));
                }
                6 => {
                    let focus = self.focus(1);
                    self.write(self.options.sessions, "exit\r");
                    return Task::batch([focus, later(Message::Timeout)]);
                }
                7 => self.write(1, "exit\r"),
                _ => return Task::done(Message::Failed("invalid smoke step")),
            },
            Message::Captured(name, shot) => {
                self.note("screenshot", 0, shot.size.width, shot.scale_factor);
                if let Err(error) =
                    report::screenshot(&self.options.output.join(format!("{name}.ppm")), shot)
                {
                    self.error = Some(error.to_string());
                }
                if self.options.mode == Mode::Smoke {
                    return later(Message::Step(if name == "before" { 1 } else { 3 }));
                }
            }
            Message::Close => {
                if self.closing {
                    return Task::none();
                }
                self.closing = true;
                for id in 1..=self.options.sessions {
                    self.write(id, "exit\r");
                }
                return later(Message::Timeout);
            }
            Message::Timeout => {
                self.note("timeout", 0, self.terminals.len(), 0);
                return self.finish();
            }
            Message::Failed(error) => {
                self.error = Some(error.into());
                return self.finish();
            }
        }
        Task::none()
    }

    fn finish(&mut self) -> Task<Message> {
        for id in 1..=self.options.sessions {
            let state = &self.states[&id];
            let (wakeups, resizes) = (state.wakeups, state.resizes);
            self.note("counts", id, wakeups, resizes);
        }
        self.note("finished", 0, self.terminals.len(), self.error.is_some());
        iced::exit()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            Subscription::batch(self.terminals.values().map(Terminal::subscription))
                .map(Message::Terminal),
            window::close_requests().map(|_| Message::Close),
        ])
    }

    fn view(&self) -> Element<'_, Message> {
        let panes = PaneGrid::new(&self.panes, |_, id, _| {
            let content: Element<'_, Message> = match self.terminals.get(id) {
                Some(term) => TerminalView::show(term).map(Message::Terminal),
                None => text(format!(
                    "Session {id} exited (code {:?}, final marker {})",
                    self.states[id].exit_code, self.states[id].final_seen
                ))
                .into(),
            };
            pane_grid::Content::new(content).title_bar(
                pane_grid::TitleBar::new(text(format!(
                    "Session {id}{}",
                    if self.focused == *id { " [focus]" } else { "" }
                )))
                .padding(4),
            )
        })
        .width(Fill)
        .height(Fill)
        .spacing(4)
        .on_click(Message::Focus)
        .on_resize(8, Message::Resize);
        container(
            column![
                row![
                    button("Swap equal panes").on_press(Message::Swap),
                    button("Resize window").on_press(Message::WindowResize),
                    button("Dimensions").on_press(Message::Write("size\r")),
                    button("VT probes").on_press(Message::Write("probe\r")),
                    button("Exit fixtures").on_press(Message::Close)
                ]
                .spacing(8),
                text(
                    self.error
                        .as_deref()
                        .unwrap_or("Research harness: synthetic child output only")
                ),
                panes,
            ]
            .spacing(6),
        )
        .padding(8)
        .into()
    }
}

// One-shot delays use the application's shared timer driver. No periodic timer
// exists during idle measurement. Do not use smoke delays in Pike's app.
fn later(message: Message) -> Task<Message> {
    let duration = if matches!(message, Message::Timeout) {
        Duration::from_secs(3)
    } else {
        Duration::from_millis(250)
    };
    Task::perform(
        async move { tokio::time::sleep(duration).await },
        move |()| message.clone(),
    )
}
