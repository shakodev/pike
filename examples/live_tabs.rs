//! Finite native actual-App tab fixture. Only controlled child data is retained.
//! Run: cargo run --locked --example live_tabs -- FRESH_OUTPUT_DIRECTORY
#[path = "support/cursor_input.rs"]
#[allow(dead_code)]
mod fixture_input;
#[path = "support/live_tabs_widget.rs"]
mod widget;

use iced::advanced::renderer::Headless;
use iced::advanced::widget::{Id, operate, operation::focusable};
use iced::{Element, Executor, Subscription, Task};
use iced_term::{BackendCommand as B, Event, SelectionType, TermMode};
use pike::app::{App, CloseAction, Control, Direction, Message};
use pike::identity::{SessionId, TabId};
use pike::session::Lifecycle;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--child") {
        child(Path::new(args.get(2).ok_or("missing fixture output")?))?;
        return Ok(());
    }
    let output = std::path::absolute(args.get(1).ok_or("expected fresh output directory")?)?;
    fs::create_dir(&output)?;
    let executor = pike::executor::SharedExecutor::new()?;
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or("fixture requires a native graphics device")?;
    drop(executor);
    let renderer = std::cell::RefCell::new(Some(renderer));
    let destination = output.clone();
    iced::application(
        move || Probe::new(destination.clone(), renderer.borrow_mut().take().unwrap()),
        Probe::update,
        Probe::view,
    )
    .executor::<pike::executor::SharedExecutor>()
    .subscription(|p: &Probe| {
        Subscription::batch([
            p.app.subscription().map(Msg::App),
            // This finite fixture's driver is not a production polling task.
            iced::time::every(Duration::from_millis(25)).map(|_| Msg::Tick),
        ])
    })
    .window_size((1100.0, 720.0))
    .title("Pike — live tabs synthetic fixture")
    .exit_on_close_request(false)
    .run()?;
    if fs::read_to_string(output.join("completed.txt"))?.trim() != "passed" {
        return Err("tab fixture failed; inspect events.txt".into());
    }
    Ok(())
}

fn child(output: &Path) -> io::Result<()> {
    let pid = std::process::id();
    let directory = output.join(pid.to_string());
    fs::create_dir(&directory)?;
    fixture_input::configure(Some(&directory))?;
    #[cfg(unix)]
    if !std::process::Command::new("/bin/stty")
        .args(["raw", "-echo"])
        .status()?
        .success()
    {
        return Err(io::Error::other("synthetic child raw input failed"));
    }
    fs::write(
        directory.join("cwd.txt"),
        std::env::current_dir()?.to_string_lossy().as_bytes(),
    )?;
    fs::write(
        directory.join("profile.txt"),
        std::env::var("PIKE29_PROFILE").unwrap_or_default(),
    )?;
    let input_dir = directory.clone();
    std::thread::spawn(move || -> io::Result<()> {
        let mut all = Vec::new();
        let mut bytes = [0; 128];
        loop {
            let count = io::stdin().read(&mut bytes)?;
            if count == 0 {
                return Ok(());
            }
            if all.len() + count > 1024 {
                return Err(io::Error::other("synthetic input exceeded bound"));
            }
            all.extend_from_slice(&bytes[..count]);
            fs::write(input_dir.join("input.bin"), &all)?;
        }
    });
    for line in 0..100 {
        print!("PIKE29_HISTORY_{line}\r\n");
    }
    print!("\x1b[2J\x1b[HPIKE29_PID={pid}\x1b[?1h\x1b[?2004h");
    io::stdout().flush()?;
    fs::write(directory.join("ready.txt"), "ready")?;
    let start = Instant::now();
    let mut background = false;
    loop {
        if !background && directory.join("background.go").exists() {
            for line in 0..64 {
                print!("\x1b[3;1HPIKE29_BACKGROUND_{line:02}");
            }
            print!("\x07\x1b[2;3H\x1b[6n\x1b[4;5H\x1b[6n");
            io::stdout().flush()?;
            fs::write(
                directory.join("background.txt"),
                "64 updates and two cursor queries",
            )?;
            background = true;
        }
        if directory.join("exit.go").exists() {
            print!("\x1b[5;1HPIKE29_FINAL_{pid}");
            io::stdout().flush()?;
            std::process::exit(37);
        }
        if start.elapsed() > Duration::from_secs(25) {
            return Err(io::Error::other("synthetic child deadline"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Remembered {
    widget: Id,
    pid: u32,
    history: usize,
    columns: usize,
    lines: usize,
    selection: Option<String>,
    mode: TermMode,
}

struct Probe {
    app: App,
    renderer: iced::Renderer,
    output: PathBuf,
    start: Instant,
    stage: u8,
    first: Option<SessionId>,
    second: Option<SessionId>,
    hidden_snapshots: u64,
    hidden_resizes: u64,
    hidden_invalidations: u64,
    saved: BTreeMap<SessionId, Remembered>,
    nested: Option<String>,
    closing: bool,
    failed: bool,
    awaiting_focus: Option<SessionId>,
    input_session: Option<SessionId>,
}

enum Msg {
    App(Message),
    Tick,
    Focus(SessionId, bool),
}

impl Probe {
    fn new(output: PathBuf, renderer: iced::Renderer) -> (Self, Task<Msg>) {
        let profile = iced_term::settings::BackendSettings {
            program: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            args: vec!["--child".into(), output.to_string_lossy().into_owned()],
            env: [("PIKE29_PROFILE".into(), "inherited-tab-profile".into())].into(),
            working_directory: Some(output.clone()),
            ..Default::default()
        };
        let (app, task) = App::with_settings(profile);
        (
            Self {
                app,
                renderer,
                output,
                start: Instant::now(),
                stage: 0,
                first: None,
                second: None,
                hidden_snapshots: 0,
                hidden_resizes: 0,
                hidden_invalidations: 0,
                saved: BTreeMap::new(),
                nested: None,
                closing: false,
                failed: false,
                awaiting_focus: None,
                input_session: None,
            },
            task.map(Msg::App),
        )
    }

    fn record(&self, message: impl AsRef<str>) {
        writeln!(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.output.join("events.txt"))
                .unwrap(),
            "{}",
            message.as_ref()
        )
        .unwrap();
    }

    fn control(&mut self, control: Control) -> Task<Msg> {
        self.app.update(Message::Control(control)).map(Msg::App)
    }

    fn tab(&self, id: SessionId) -> TabId {
        self.app.model().target(id).unwrap().tab
    }

    fn ready(&self, id: SessionId) -> bool {
        self.app
            .terminal(id)
            .and_then(|t| t.renderable_content())
            .is_some_and(|c| {
                c.terminal_mode
                    .contains(TermMode::APP_CURSOR | TermMode::BRACKETED_PASTE)
                    && c.viewport
                        .cells()
                        .iter()
                        .map(|c| c.c)
                        .collect::<String>()
                        .contains("PIKE29_PID=")
            })
    }

    fn remember(&mut self, id: SessionId) {
        let terminal = self.app.terminal(id).unwrap();
        let content = terminal.renderable_content().unwrap();
        let text: String = content.viewport.cells().iter().map(|cell| cell.c).collect();
        let pid = text
            .split("PIKE29_PID=")
            .nth(1)
            .unwrap()
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let stats = terminal.snapshot_stats();
        assert!(stats.history_lines > 0);
        self.saved.insert(
            id,
            Remembered {
                widget: terminal.widget_id().clone(),
                pid,
                history: stats.history_lines,
                columns: stats.columns,
                lines: stats.screen_lines,
                selection: terminal.copy_selection(|s| s),
                mode: content.terminal_mode,
            },
        );
        self.record(format!(
            "remembered session={} pid={pid} dimensions={}x{} history={}",
            id.get(),
            stats.columns,
            stats.screen_lines,
            stats.history_lines
        ));
    }

    fn verify(&self, id: SessionId, dimensions: bool) -> bool {
        let old = &self.saved[&id];
        let terminal = self.app.terminal(id).unwrap();
        let stats = terminal.snapshot_stats();
        let Some(content) = terminal.renderable_content() else {
            return false;
        };
        terminal.widget_id() == &old.widget
            && stats.history_lines >= old.history
            && content.terminal_mode == old.mode
            && terminal.copy_selection(|s| s) == old.selection
            && (!dimensions || (stats.columns == old.columns && stats.screen_lines == old.lines))
            && fs::read_to_string(self.output.join(old.pid.to_string()).join("profile.txt"))
                .unwrap()
                == "inherited-tab-profile"
            && fs::read_to_string(self.output.join(old.pid.to_string()).join("cwd.txt")).unwrap()
                == self.output.to_string_lossy()
    }

    fn activate(&mut self, id: SessionId) -> Task<Msg> {
        let tab = self.app.model().workspace().tab(self.tab(id)).unwrap();
        let focused = tab.grid().get(tab.focused()).unwrap().session;
        let before = self.app.terminal(id).unwrap().snapshot_stats().snapshots;
        let started = Instant::now();
        let task = self.control(Control::ActivateTab(self.tab(id)));
        let dispatch_us = started.elapsed().as_micros();
        let after = self.app.terminal(id).unwrap().snapshot_stats().snapshots;
        if after != before + 1 {
            return self.finish(Some("activation did not refresh exactly once"));
        }
        self.record(format!(
            "activation session={} refreshes=1 dispatch_us={dispatch_us}",
            id.get()
        ));
        self.awaiting_focus = Some(focused);
        task.chain(
            operate(focusable::is_focused(
                self.app.terminal(focused).unwrap().widget_id().clone(),
            ))
            .map(move |is_focused| Msg::Focus(focused, is_focused)),
        )
    }

    fn finish(&mut self, error: Option<&str>) -> Task<Msg> {
        if let Some(error) = error {
            self.failed = true;
            self.record(error);
        }
        if self.closing {
            return Task::none();
        }
        self.closing = true;
        let task = self.app.update(Message::CloseWindow).map(Msg::App);
        let confirmation = self
            .app
            .close_confirmation()
            .map(|(token, _)| self.control(Control::CloseAction(CloseAction::Confirm(token))))
            .unwrap_or_else(Task::none);
        Task::batch([task, confirmation])
    }

    fn update(&mut self, message: Msg) -> Task<Msg> {
        if self.start.elapsed() > Duration::from_secs(25) {
            self.record("failed: finite fixture cleanup deadline expired");
            fs::write(self.output.join("completed.txt"), "failed").unwrap();
            return iced::exit();
        }
        let task = match message {
            Msg::App(message) => {
                let done = matches!(&message, Message::ShutdownComplete);
                let task = self.app.update(message).map(Msg::App);
                if done {
                    self.record("native-cleanup acknowledged");
                    fs::write(
                        self.output.join("completed.txt"),
                        if self.failed { "failed" } else { "passed" },
                    )
                    .unwrap();
                }
                task
            }
            Msg::Focus(id, focused) => {
                if !self.closing && (!focused || self.app.active_session() != Some(id)) {
                    return self.finish(Some("actual native widget focus differs"));
                }
                self.record(format!(
                    "actual-widget-focus session={} focused={focused}",
                    id.get()
                ));
                self.awaiting_focus = None;
                Task::none()
            }
            Msg::Tick => Task::none(),
        };
        if self.closing {
            return task;
        }
        if self.start.elapsed() > Duration::from_secs(20) {
            return self.finish(Some("fixture deadline expired"));
        }
        // Owning-window focus is synthetic; physical activation is not asserted.
        drop(self.app.update(Message::WindowFocused(true)));
        let next = self.advance();
        Task::batch([task, next])
    }

    fn view(&self) -> Element<'_, Msg> {
        self.app.view().map(Msg::App)
    }

    fn advance(&mut self) -> Task<Msg> {
        if self.awaiting_focus.is_some() {
            return Task::none();
        }
        match self.stage {
            0 => {
                let Some(id) = self.app.active_session() else {
                    return Task::none();
                };
                if !self.ready(id)
                    || self.app.terminal(id).unwrap().snapshot_stats().columns < 40
                    || self.app.terminal(id).unwrap().work_stats().grid_resizes == 0
                {
                    return Task::none();
                }
                for command in [
                    B::SelectStart(SelectionType::Simple, (0.0, 0.0)),
                    B::SelectUpdate((80.0, 0.0)),
                ] {
                    drop(self.app.update(Message::Terminal((
                        id,
                        Event::BackendCall(id.get(), command),
                    ))));
                }
                self.remember(id);
                assert!(self.saved[&id].selection.is_some());
                self.first = Some(id);
                self.stage = 1;
                let message = widget::click_new_tab(&self.app, &self.renderer);
                self.record("actual-app-button New tab clicked");
                let task = self.app.update(message).map(Msg::App);
                let second = self.app.active_session().unwrap();
                self.second = Some(second);
                assert!(self.app.terminal(second).is_none());
                self.record(format!(
                    "pending-tab session={} switch-away-before-attachment=true",
                    second.get()
                ));
                let select = self.control(Control::ActivateTab(self.tab(id)));
                Task::batch([task, select])
            }
            1 => {
                let second = self.second.unwrap();
                let Some(terminal) = self.app.terminal(second) else {
                    return Task::none();
                };
                if terminal.is_visible()
                    || terminal.snapshot_stats().snapshots != 0
                    || terminal.snapshot_stats().viewport_cells != 0
                    || self.app.active_session() != self.first
                {
                    return self.finish(Some(
                        "hidden late attachment stole focus or built a snapshot",
                    ));
                }
                self.record("hidden-attachment snapshots=0 viewport-cells=0 focus-preserved=true");
                self.stage = 2;
                self.activate(second)
            }
            2 => {
                let second = self.second.unwrap();
                if !self.ready(second) {
                    return Task::none();
                }
                let first = self.first.unwrap();
                let stats = self.app.terminal(second).unwrap().snapshot_stats();
                if stats.columns != self.saved[&first].columns
                    || stats.screen_lines != self.saved[&first].lines
                {
                    return Task::none();
                }
                self.remember(second);
                self.stage = 3;
                let task = self.activate(first);
                let hidden = self.app.terminal(second).unwrap();
                self.hidden_snapshots = hidden.snapshot_stats().snapshots;
                self.hidden_resizes = hidden.work_stats().grid_resizes;
                self.hidden_invalidations = hidden.work_stats().cache_invalidations;
                fs::write(
                    self.output
                        .join(self.saved[&second].pid.to_string())
                        .join("background.go"),
                    "go",
                )
                .unwrap();
                task
            }
            3 => {
                let first = self.first.unwrap();
                let second = self.second.unwrap();
                let bytes = fs::read(
                    self.output
                        .join(self.saved[&second].pid.to_string())
                        .join("input.bin"),
                )
                .unwrap_or_default();
                let activity = self
                    .app
                    .model()
                    .workspace()
                    .tab(self.tab(second))
                    .unwrap()
                    .activity();
                if bytes.len() < 12 || !activity.unread || !activity.bell {
                    return Task::none();
                }
                if bytes != b"\x1b[2;3R\x1b[4;5R" {
                    return self.finish(Some("hidden query reply bytes differ"));
                }
                let hidden = self.app.terminal(second).unwrap();
                if hidden.snapshot_stats().snapshots != self.hidden_snapshots
                    || hidden.snapshot_stats().viewport_cells != 0
                    || hidden.work_stats().grid_resizes != self.hidden_resizes
                    || hidden.work_stats().cache_invalidations != self.hidden_invalidations
                {
                    return self.finish(Some("hidden output performed presentation work"));
                }
                if !self.verify(first, true) {
                    return self.finish(Some("same-size return lost session state"));
                }
                if widget::visible_terminals(&self.app, &self.renderer) != vec![first] {
                    return self.finish(Some("hidden terminal remained in actual App widget tree"));
                }
                let before = hidden.input_status();
                for command in [
                    B::Write(b"inactive".to_vec()),
                    B::Resize(Some(iced::Size::new(31.0, 17.0)), None),
                    B::FocusChanged(true),
                ] {
                    drop(self.app.update(Message::Terminal((
                        second,
                        Event::BackendCall(second.get(), command),
                    ))));
                }
                if self.app.terminal(second).unwrap().input_status() != before
                    || self.app.terminal(second).unwrap().work_stats().grid_resizes
                        != self.hidden_resizes
                {
                    return self.finish(Some(
                        "stale hidden widget message changed input or dimensions",
                    ));
                }
                self.record("hidden-output updates=64 queries=2 exact-replies=true unread=true bell=true snapshots=0 grid-resizes=0 cache-invalidations=0 inactive-input=0");
                self.stage = 4;
                self.activate(second)
            }
            4 => {
                let second = self.second.unwrap();
                if !self.verify(second, true) {
                    return self.finish(Some(
                        "same-size activation lost identity/mode/history/dimensions",
                    ));
                }
                if self
                    .app
                    .model()
                    .workspace()
                    .tab(self.tab(second))
                    .unwrap()
                    .activity()
                    != Default::default()
                {
                    return self.finish(Some("activation did not acknowledge activity"));
                }
                self.stage = 5;
                let target = self.app.active_target().unwrap();
                self.control(Control::Split(target, Direction::Right))
            }
            5 => {
                let second = self.second.unwrap();
                let tab = self.app.model().workspace().tab(self.tab(second)).unwrap();
                if tab.grid().len() != 2
                    || self.app.active_session() == Some(second)
                    || !self.ready(self.app.active_session().unwrap())
                {
                    return Task::none();
                }
                self.stage = 6;
                self.control(Control::Split(
                    self.app.active_target().unwrap(),
                    Direction::Down,
                ))
            }
            6 => {
                let second = self.second.unwrap();
                let tab = self.app.model().workspace().tab(self.tab(second)).unwrap();
                if tab.grid().len() != 3
                    || !tab.grid().iter().all(|(_, pane)| {
                        self.ready(pane.session)
                            && self
                                .app
                                .terminal(pane.session)
                                .unwrap()
                                .work_stats()
                                .grid_resizes
                                > 0
                    })
                {
                    return Task::none();
                }
                self.nested = Some(format!("{:?}", tab.grid().layout()));
                let sessions: Vec<_> = tab.grid().iter().map(|(_, pane)| pane.session).collect();
                for id in sessions {
                    if !self.saved.contains_key(&id) {
                        self.remember(id);
                    }
                }
                self.stage = 7;
                self.activate(self.first.unwrap())
            }
            7 => {
                if !self.verify(self.first.unwrap(), true) {
                    return self.finish(Some("return from nested tab lost original state"));
                }
                self.stage = 8;
                // Activate by remembered focused session, rather than first pane.
                let tab = self
                    .app
                    .model()
                    .workspace()
                    .tab(self.tab(self.second.unwrap()))
                    .unwrap();
                let focused = tab.grid().get(tab.focused()).unwrap().session;
                self.activate(focused)
            }
            8 => {
                let second = self.second.unwrap();
                let tab = self.app.model().workspace().tab(self.tab(second)).unwrap();
                if Some(format!("{:?}", tab.grid().layout())) != self.nested {
                    return self.finish(Some("nested tab switch changed ratios or pane IDs"));
                }
                if !tab
                    .grid()
                    .iter()
                    .all(|(_, pane)| self.verify(pane.session, pane.session != second))
                {
                    return self.finish(Some(
                        "nested return lost a session identity/mode/history/dimensions",
                    ));
                }
                let id = self.app.active_session().unwrap();
                self.input_session = Some(id);
                let events =
                    widget::encode(self.app.terminal(id).unwrap(), &self.renderer, "active29");
                for event in events {
                    drop(self.app.update(Message::Terminal((id, event))));
                }
                self.record(format!("nested-return remembered-session={} layout-preserved=true public-widget-input=active29", id.get()));
                self.stage = 9;
                self.activate(self.first.unwrap())
            }
            9 => {
                let second = self.second.unwrap();
                let active = self.input_session.unwrap();
                let input = fs::read(
                    self.output
                        .join(self.saved[&active].pid.to_string())
                        .join("input.bin"),
                )
                .unwrap_or_default();
                if input.len() < b"active29\r".len() {
                    return Task::none();
                }
                if input != b"active29\r" {
                    return self.finish(Some("public-widget input bytes differ"));
                }
                if fs::read(
                    self.output
                        .join(self.saved[&second].pid.to_string())
                        .join("input.bin"),
                )
                .unwrap()
                    != b"\x1b[2;3R\x1b[4;5R"
                {
                    return self.finish(Some("active input reached inactive session"));
                }
                self.record("public-widget exact-active-input=true inactive-bytes=0");
                fs::write(
                    self.output
                        .join(self.saved[&second].pid.to_string())
                        .join("exit.go"),
                    "go",
                )
                .unwrap();
                self.hidden_snapshots = self
                    .app
                    .terminal(second)
                    .unwrap()
                    .snapshot_stats()
                    .snapshots;
                self.stage = 10;
                Task::none()
            }
            10 => {
                let second = self.second.unwrap();
                if self.app.model().sessions().get(second).unwrap().lifecycle()
                    != (Lifecycle::Exited { code: Some(37) })
                {
                    return Task::none();
                }
                let hidden = self.app.terminal(second).unwrap();
                if self.app.active_session() != self.first
                    || hidden.snapshot_stats().snapshots != self.hidden_snapshots
                    || hidden.shutdown_handle().completion().is_none()
                    || !self
                        .app
                        .model()
                        .workspace()
                        .tab(self.tab(second))
                        .unwrap()
                        .activity()
                        .exited
                {
                    return self.finish(Some(
                        "hidden natural exit stole focus or missed cleanup/activity",
                    ));
                }
                self.record("hidden-natural-exit code=37 native-cleanup=true focus-preserved=true snapshots=0 exit-flag=true");
                self.stage = 11;
                self.activate(second)
            }
            11 => {
                let second = self.second.unwrap();
                let text: String = self
                    .app
                    .terminal(second)
                    .unwrap()
                    .renderable_content()
                    .unwrap()
                    .viewport
                    .cells()
                    .iter()
                    .map(|c| c.c)
                    .collect();
                if !text.contains("PIKE29_FINAL_") {
                    return self.finish(Some("hidden final output missing on activation"));
                }
                self.record("passed same-size=true nested=true identity-mode-history-selection=true hidden-query-output-exit=true pending-spawn=true");
                self.finish(None)
            }
            _ => Task::none(),
        }
    }
}
