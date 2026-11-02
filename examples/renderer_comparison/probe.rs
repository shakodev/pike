#[path = "metrics.rs"]
mod metrics;
use crate::options::Options;
use iced::{Element, Subscription, Task};
use iced_term::{AlacrittyEvent, BackendCommand, Event};
use pike::{
    app::{App, CloseAction, Control, Message},
    identity::{SessionId, TabId},
    session::Lifecycle,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    time::{Duration, Instant},
};

pub fn run(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let output = options.output.clone();
    let exe = toml::Value::String(std::env::current_exe()?.to_string_lossy().into_owned());
    let config = output.join("settings.toml");
    fs::write(
        &config,
        format!(
            "version=1\ndefault_profile='fixture'\nhistory=2000\ntheme='dark'\n[font]\nfamily='monospace'\nsize=14.0\n[profiles.fixture]\nshell={exe}\nargs=['--child']\n"
        ),
    )?;
    fs::write(
        output.join("graphics.toml"),
        "actual_adapter='unavailable: pinned lean profile disables iced/sysinfo'\n",
    )?;
    iced::application(
        move || Probe::new(options.clone(), config.clone()),
        Probe::update,
        Probe::view,
    )
    .executor::<pike::executor::SharedExecutor>()
    .subscription(Probe::subscription)
    .theme(|p: &Probe| p.app.theme())
    .window_size((1600.0, 1000.0))
    .title("Pike renderer comparison — synthetic")
    .exit_on_close_request(false)
    .run()?;
    if !output.join("completed.txt").is_file() {
        return Err("comparison failed; inspect bounded fixture events".into());
    }
    Ok(())
}

enum Msg {
    App(Message),
    Settle,
    End,
    Deadline,
    Probe,
    Advance,
    Captured(iced::window::Screenshot),
}
#[derive(PartialEq)]
enum Stage {
    Build,
    Prepare,
    Settle,
    Measure,
    Drain,
    Close,
}

struct Probe {
    app: App,
    options: Options,
    start: Instant,
    stage: Stage,
    events: File,
    remaining: usize,
    base: Option<TabId>,
    ready: BTreeMap<SessionId, u32>,
    prepared: BTreeSet<SessionId>,
    done: BTreeSet<SessionId>,
    final_seen: BTreeSet<SessionId>,
    completed: BTreeSet<SessionId>,
    began: Option<Instant>,
    probe: Option<(SessionId, usize, Instant, Option<Instant>)>,
    probes: usize,
    latency: File,
    failed: bool,
    focus_requested: bool,
}

impl Probe {
    fn new(options: Options, config: std::path::PathBuf) -> (Self, Task<Msg>) {
        let (app, task) = App::with_config_path(config);
        let mut events =
            File::create(options.output.join("events.csv")).expect("fresh fixture trace");
        writeln!(events, "ms,event,session,a,b").expect("fixture header");
        let mut latency =
            File::create(options.output.join("latency.csv")).expect("fresh latency trace");
        writeln!(
            latency,
            "nonce,session,snapshot_ms,readback_ms,width,height,scale"
        )
        .expect("latency header");
        let deadline = options.seconds + options.settle + 100;
        (
            Self {
                app,
                options,
                start: Instant::now(),
                stage: Stage::Build,
                events,
                remaining: 4096,
                base: None,
                ready: BTreeMap::new(),
                prepared: BTreeSet::new(),
                done: BTreeSet::new(),
                final_seen: BTreeSet::new(),
                completed: BTreeSet::new(),
                began: None,
                probe: None,
                probes: 0,
                latency,
                failed: false,
                focus_requested: false,
            },
            Task::batch([task.map(Msg::App), delay(deadline, Msg::Deadline)]),
        )
    }

    fn note(
        &mut self,
        event: &str,
        session: u64,
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
    ) {
        assert!(self.remaining > 0, "bounded comparison trace exhausted");
        self.remaining -= 1;
        writeln!(
            self.events,
            "{:.3},{event},{session},{a},{b}",
            self.start.elapsed().as_secs_f64() * 1000.0
        )
        .expect("fixture event");
        self.events.flush().expect("fixture flush");
    }

    fn send(&mut self, id: SessionId, text: String) -> Task<Msg> {
        let raw = self.app.terminal(id).expect("fixture terminal").id;
        // Select through real App controls before synthetic input; hidden PTYs
        // never receive fake protocol replies to bypass keyboard ownership.
        let target = self.app.model().target(id).expect("fixture pane");
        let activate = self
            .app
            .update(Message::Control(Control::ActivateTab(target.tab)));
        let focus = self.app.update(Message::Control(Control::Focus(target)));
        let write = self.app.update(Message::Terminal((
            id,
            Event::BackendCall(raw, BackendCommand::Write(format!("{text}\n").into_bytes())),
        )));
        Task::batch([activate, focus, write]).map(Msg::App)
    }

    fn update(&mut self, message: Msg) -> Task<Msg> {
        match message {
            Msg::Advance => self.advance(),
            Msg::App(message) => {
                if let Message::WindowFocused(value) = &message {
                    self.note("native_focus", 0, value, 0);
                }
                if let Message::Terminal((id, Event::BackendCall(_, command))) = &message {
                    match command {
                        BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Title(title)) => {
                            self.title(*id, title)
                        }
                        BackendCommand::EventBridge(
                            iced_term::EventBridgeEvent::BackendCompleted(result),
                        ) => {
                            self.note(
                                "completion",
                                id.get(),
                                result.output_drained,
                                result.exit_code.unwrap_or(-1),
                            );
                            if result.reason != iced_term::ExitReason::ChildExit
                                || !result.output_drained
                                || result.exit_code != Some(0)
                            {
                                self.failed = true;
                            }
                            self.completed.insert(*id);
                        }
                        _ => {}
                    }
                }
                let task = self.app.update(message).map(Msg::App);
                Task::batch([task, self.advance()])
            }
            Msg::Settle => {
                self.stage = Stage::Measure;
                metrics::counters(&self.app, &self.options.output, "before")
                    .expect("before counters");
                metrics::reset_frames();
                self.began = Some(Instant::now());
                self.note(
                    "measure_begin",
                    0,
                    self.options.visible,
                    self.options.hidden,
                );
                let mut tasks = vec![delay(self.options.seconds, Msg::End)];
                if self.options.probes > 0 {
                    tasks.push(Task::done(Msg::Probe));
                }
                Task::batch(tasks)
            }
            Msg::Probe if self.stage == Stage::Measure && self.probes < self.options.probes => {
                let id = self.ids()[0];
                self.probes += 1;
                self.probe = Some((id, self.probes, Instant::now(), None));
                self.send(id, format!("echo {}", self.probes))
            }
            Msg::Probe => Task::none(),
            Msg::Captured(shot) => {
                if let Some((id, nonce, sent, Some(snapshot))) = self.probe.take() {
                    writeln!(
                        self.latency,
                        "{nonce},{},{:.3},{:.3},{},{},{}",
                        id.get(),
                        snapshot.duration_since(sent).as_secs_f64() * 1000.0,
                        sent.elapsed().as_secs_f64() * 1000.0,
                        shot.size.width,
                        shot.size.height,
                        shot.scale_factor
                    )
                    .expect("latency sample");
                    self.latency.flush().expect("latency flush");
                    if nonce == 1 || nonce == self.options.probes {
                        metrics::screenshot(
                            &self.options.output.join(format!("synthetic-{nonce}.ppm")),
                            &shot,
                        )
                        .expect("synthetic capture");
                    }
                }
                Task::perform(
                    async { tokio::time::sleep(Duration::from_millis(100)).await },
                    |_| Msg::Probe,
                )
            }
            Msg::End => {
                self.note(
                    "measure_end",
                    0,
                    self.began.expect("measurement began").elapsed().as_millis(),
                    self.probes,
                );
                metrics::counters(&self.app, &self.options.output, "after")
                    .expect("after counters");
                metrics::save_frames(&self.options.output).expect("frame histogram");
                self.stage = Stage::Drain;
                self.advance()
            }
            Msg::Deadline => {
                self.failed = true;
                self.note("deadline", 0, 0, 0);
                self.close()
            }
        }
    }

    fn title(&mut self, id: SessionId, title: &str) {
        let Some(value) = title.strip_prefix("pike38:") else {
            return;
        };
        if let Some(pid) = value
            .strip_prefix("ready:")
            .and_then(|s| s.parse::<u32>().ok())
        {
            if self.ready.insert(id, pid).is_none() {
                self.note("ready", id.get(), pid, 0);
            }
        } else if value == "prepared" {
            self.prepared.insert(id);
        } else if value == "final" {
            self.final_seen.insert(id);
        } else if let Some(done) = value.strip_prefix("done:") {
            let fields: Vec<_> = done.split(':').take(3).collect();
            if fields.len() == 2
                && let (Ok(bytes), Ok(micros)) =
                    (fields[0].parse::<u64>(), fields[1].parse::<u64>())
                && self.done.insert(id)
            {
                self.note("throughput", id.get(), bytes, micros);
            }
        }
    }

    fn ids(&self) -> Vec<SessionId> {
        self.app
            .model()
            .sessions()
            .iter()
            .map(|(id, _)| id)
            .collect()
    }

    fn advance(&mut self) -> Task<Msg> {
        if self.failed {
            return self.close();
        }
        if self.app.settings_error().is_some()
            || self
                .app
                .model()
                .sessions()
                .iter()
                .any(|(_, e)| matches!(e.lifecycle(), Lifecycle::Failed(_)))
        {
            self.failed = true;
            self.note("fixture_failure", 0, 0, 0);
            return self.close();
        }
        let ids = self.ids();
        if self.stage == Stage::Build {
            if ids.is_empty()
                || ids
                    .iter()
                    .any(|id| !self.ready.contains_key(id) || self.app.terminal(*id).is_none())
            {
                return Task::none();
            }
            if !self.focus_requested {
                self.focus_requested = true;
                if self.options.synthetic_focus {
                    self.note("synthetic_window_focus", 0, true, 0);
                    return self
                        .app
                        .update(Message::WindowFocused(true))
                        .map(Msg::App)
                        .chain(Task::done(Msg::Advance));
                }
                return iced::window::latest()
                    .then(|id| id.map_or_else(Task::none, iced::window::gain_focus))
                    .chain(Task::done(Msg::Advance));
            }
            if self
                .app
                .active_session()
                .and_then(|id| self.app.terminal(id))
                .and_then(|t| t.renderable_content())
                .is_none_or(|c| !c.focused)
            {
                return Task::none();
            }
            self.base = self.base.or(self.app.model().workspace().active());
            if ids.len() < self.options.visible {
                let (source, direction) = self.options.splits()[ids.len() - 1];
                if self.app.terminal(ids[source]).is_none_or(|t| {
                    t.snapshot_stats().columns < 20 || t.snapshot_stats().screen_lines < 5
                }) {
                    return Task::none();
                }
                let target = self.app.model().target(ids[source]).expect("source pane");
                return self
                    .app
                    .update(Message::Control(Control::Split(target, direction)))
                    .map(Msg::App);
            }
            if ids.len() < self.options.visible + self.options.hidden {
                return self
                    .app
                    .update(Message::Control(Control::NewTab))
                    .map(Msg::App);
            }
            self.stage = Stage::Prepare;
            let mut tasks = vec![];
            for id in ids {
                tasks.push(self.send(
                    id,
                    format!(
                        "prepare {}",
                        if self.options.saturated {
                            "saturated"
                        } else {
                            "empty"
                        }
                    ),
                ));
            }
            tasks.push(
                self.app
                    .update(Message::Control(Control::ActivateTab(self.base.unwrap())))
                    .map(Msg::App),
            );
            return Task::batch(tasks);
        }
        if self.stage == Stage::Prepare
            && self.prepared.len() == ids.len()
            && ids
                .iter()
                .take(self.options.visible)
                .all(|id| metrics::marker(&self.app, *id, "PIKE38 READY"))
        {
            metrics::counters(&self.app, &self.options.output, "prepared")
                .expect("prepared counters");
            for id in &ids {
                let terminal = self.app.terminal(*id).unwrap();
                let s = terminal.snapshot_stats();
                if terminal.is_visible()
                    != (ids.iter().position(|i| i == id).unwrap() < self.options.visible)
                    || s.history_limit.lines() != 2000
                    || (self.options.saturated && s.history_lines != 2000)
                    || (!self.options.saturated && s.history_lines != 0)
                {
                    self.failed = true;
                }
            }
            if self.failed {
                self.note("invalid_history_or_visibility", 0, 0, 0);
                return self.close();
            }
            self.stage = Stage::Settle;
            self.note(
                "scenario_ready",
                0,
                self.options.visible,
                self.options.hidden,
            );
            let mut tasks = Vec::new();
            if self.options.flood {
                let epoch = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
                    + (self.options.settle as u128 + 2) * 1000;
                for id in self.ids() {
                    tasks.push(self.send(id, format!("flood {} {epoch}", self.options.seconds)));
                }
            }
            tasks.push(
                self.app
                    .update(Message::Control(Control::ActivateTab(self.base.unwrap())))
                    .map(Msg::App),
            );
            let first = self.ids()[0];
            let target = self.app.model().target(first).unwrap();
            tasks.push(
                self.app
                    .update(Message::Control(Control::Focus(target)))
                    .map(Msg::App),
            );
            tasks.push(delay(self.options.settle + 2, Msg::Settle));
            return Task::batch(tasks);
        }
        if self.stage == Stage::Measure
            && let Some((id, nonce, _, snapshot)) = &mut self.probe
            && snapshot.is_none()
            && metrics::marker(&self.app, *id, &format!("PIKE38 ECHO {nonce:03}"))
        {
            *snapshot = Some(Instant::now());
            return iced::window::latest().then(|id| {
                id.map_or_else(Task::none, |id| {
                    iced::window::screenshot(id).map(Msg::Captured)
                })
            });
        }
        if self.stage == Stage::Drain && (!self.options.flood || self.done.len() == ids.len()) {
            self.stage = Stage::Close;
            return Task::batch(ids.into_iter().map(|id| self.send(id, "exit".into())));
        }
        if self.stage == Stage::Close && self.completed.len() == ids.len() {
            if self.final_seen.len() != ids.len() {
                self.failed = true;
            }
            if !self.failed {
                self.note("finished", 0, self.completed.len(), 0);
                fs::write(self.options.output.join("completed.txt"), "passed\n")
                    .expect("completion marker");
            }
            return self.close();
        }
        Task::none()
    }

    fn close(&mut self) -> Task<Msg> {
        let close = self.app.update(Message::CloseWindow);
        let confirm = self
            .app
            .close_confirmation()
            .map(|(token, _)| token)
            .map(|token| {
                self.app.update(Message::Control(Control::CloseAction(
                    CloseAction::Confirm(token),
                )))
            })
            .unwrap_or_else(Task::none);
        Task::batch([close, confirm]).map(Msg::App)
    }

    fn subscription(&self) -> Subscription<Msg> {
        Subscription::batch([
            self.app.subscription().map(Msg::App),
            iced::event::listen_raw(|event, _, _| {
                metrics::observe(&event);
                None::<Msg>
            }),
        ])
    }
    fn view(&self) -> Element<'_, Msg> {
        self.app.view().map(Msg::App)
    }
}

fn delay(seconds: u64, message: Msg) -> Task<Msg> {
    Task::perform(
        async move { tokio::time::sleep(Duration::from_secs(seconds)).await },
        move |_| message,
    )
}
