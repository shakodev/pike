//! Opt-in real application fixture: first_session_host MODE FRESH_OUTPUT.
//! Only its own synthetic child/input/output are recorded. No production timer.
#[path = "support/cursor_input.rs"]
mod fixture_input;

use iced::advanced::widget::{operate, operation::focusable};
use iced::widget::text;
use iced::{Element, Task};
use pike::app::{App, Control, Message};
use pike::identity::SessionId;
use pike::session::Lifecycle;
use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
    time::Duration,
};

#[derive(Clone)]
struct Config {
    mode: String,
    output: PathBuf,
}
struct Probe {
    app: App,
    config: Config,
    failures: Vec<SessionId>,
    closing: bool,
    failed: bool,
}
enum ProbeMessage {
    App(Message),
    Focus(SessionId, bool),
    Deadline,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--child") {
        child(
            args.get(2).ok_or("missing child mode")?,
            PathBuf::from(args.get(3).ok_or("missing child output directory")?),
        )?;
        return Ok(());
    }
    let mode = args
        .get(1)
        .ok_or("expected success, invalid, delayed or rapid")?
        .clone();
    if !["success", "invalid", "delayed", "rapid"].contains(&mode.as_str()) {
        return Err("unknown fixture mode".into());
    }
    let output = std::path::absolute(args.get(2).ok_or("expected fresh output directory")?)?;
    if output.exists() {
        return Err("choose a fresh output directory".into());
    }
    fs::create_dir_all(&output)?;
    let config = Config {
        mode,
        output: output.clone(),
    };
    iced::application(
        move || Probe::new(config.clone()),
        Probe::update,
        Probe::view,
    )
    .executor::<pike::executor::SharedExecutor>()
    .subscription(|probe: &Probe| probe.app.subscription().map(ProbeMessage::App))
    .title("Pike — synthetic first-session fixture")
    .window_size((960.0, 640.0))
    .exit_on_close_request(false)
    .run()?;
    if fs::read_to_string(output.join("completed.txt"))?.trim() != "passed" {
        return Err("application fixture failed".into());
    }
    Ok(())
}

fn keys() -> &'static [u8] {
    if cfg!(windows) {
        b"PIKE23_KEY\r"
    } else {
        b"PIKE23_KEY\n"
    }
}

fn child(mode: &str, output: PathBuf) -> io::Result<()> {
    fixture_input::configure(Some(&output))?;
    fs::write(output.join("child.pid"), std::process::id().to_string())?;
    if mode == "delayed" {
        std::thread::sleep(Duration::from_millis(200));
    }
    print!("\x1b[2J\x1b[HPIKE23_READY\r\n");
    io::stdout().flush()?;
    if mode != "rapid" {
        let mut bytes = vec![0; keys().len()];
        io::stdin().read_exact(&mut bytes)?;
        fs::write(output.join("captured-keys.bin"), &bytes)?;
        if fixture_input::command(&mut bytes.as_slice())?.as_deref() != Some("PIKE23_KEY") {
            return Err(io::Error::other("synthetic key bytes differ"));
        }
    }
    print!("PIKE23_FINAL\r\n");
    io::stdout().flush()?;
    std::process::exit(7);
}

impl Probe {
    fn record(&self, value: &str) {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.config.output.join("events.txt"))
            .unwrap();
        writeln!(file, "{value}").unwrap();
    }

    fn new(config: Config) -> (Self, Task<ProbeMessage>) {
        let profile = iced_term::settings::BackendSettings {
            program: if config.mode == "invalid" {
                config
                    .output
                    .join("missing-shell")
                    .to_string_lossy()
                    .into_owned()
            } else {
                std::env::current_exe()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            },
            args: if config.mode == "invalid" {
                vec![]
            } else {
                vec![
                    "--child".into(),
                    config.mode.clone(),
                    config.output.to_string_lossy().into_owned(),
                ]
            },
            ..Default::default()
        };
        let (app, task) = App::with_settings(profile);
        let probe = Self {
            app,
            config,
            failures: vec![],
            closing: false,
            failed: false,
        };
        probe.record(&format!(
            "starting session={} entries={}",
            probe.app.active_session().unwrap().get(),
            probe.app.model().sessions().len()
        ));
        (
            probe,
            Task::batch([
                task.map(ProbeMessage::App),
                Task::perform(
                    async {
                        tokio::time::sleep(Duration::from_secs(10)).await;
                    },
                    |_| ProbeMessage::Deadline,
                ),
            ]),
        )
    }

    fn close(&mut self) -> Task<ProbeMessage> {
        self.closing = true;
        self.app.update(Message::CloseWindow).map(ProbeMessage::App)
    }

    fn update(&mut self, message: ProbeMessage) -> Task<ProbeMessage> {
        let mut task = match message {
            ProbeMessage::Deadline => {
                self.failed = true;
                self.record("fixture deadline expired; cleanup still required");
                return self.close();
            }
            ProbeMessage::Focus(id, focused) => {
                if self.closing {
                    return Task::none();
                }
                self.record(&format!(
                    "focus session={} widget_focused={focused}",
                    id.get()
                ));
                if !focused || self.app.active_session() != Some(id) {
                    self.failed = true;
                    return self.close();
                }
                if self.config.mode != "rapid" {
                    return self
                        .app
                        .update(Message::Terminal((
                            id,
                            iced_term::Event::BackendCall(
                                id.get(),
                                iced_term::BackendCommand::Write(keys().to_vec()),
                            ),
                        )))
                        .map(ProbeMessage::App);
                }
                Task::none()
            }
            ProbeMessage::App(message) => {
                let launched = matches!(&message, Message::Launched(_, Ok(_)));
                let done = matches!(&message, Message::ShutdownComplete);
                let task = self.app.update(message).map(ProbeMessage::App);
                if done {
                    self.record(&format!(
                        "shutdown entries={}",
                        self.app.model().sessions().len()
                    ));
                    fs::write(
                        self.config.output.join("completed.txt"),
                        if self.failed { "failed\n" } else { "passed\n" },
                    )
                    .unwrap();
                    return task;
                }
                if launched
                    && let Some(id) = self.app.active_session()
                    && let Some(terminal) = self.app.terminal(id)
                {
                    self.record(&format!(
                        "attached session={} font_measurements={}",
                        id.get(),
                        terminal.work_stats().font_measurements
                    ));
                    task.chain(
                        operate(focusable::is_focused(terminal.widget_id().clone()))
                            .map(move |focused| ProbeMessage::Focus(id, focused)),
                    )
                } else {
                    task
                }
            }
        };
        if self.closing {
            return task;
        }
        if let Some(id) = self.app.active_session() {
            match self.app.model().sessions().get(id).unwrap().lifecycle() {
                Lifecycle::Failed(_) if self.config.mode == "invalid" => {
                    if self.failures.contains(&id) {
                        return task;
                    }
                    self.failures.push(id);
                    self.record(&format!(
                        "failed session={} entries={}",
                        id.get(),
                        self.app.model().sessions().len()
                    ));
                    if self.failures.len() == 1 {
                        task = Task::batch([
                            task,
                            self.app
                                .update(Message::Control(Control::Retry(id)))
                                .map(ProbeMessage::App),
                        ]);
                    } else {
                        assert_ne!(self.failures[0], id);
                        let target = self.app.active_target().unwrap();
                        drop(self.app.update(Message::Control(Control::Close(target))));
                        self.record(&format!(
                            "empty entries={}",
                            self.app.model().sessions().len()
                        ));
                        task = Task::batch([task, self.close()]);
                    }
                }
                Lifecycle::Exited { code } => {
                    let terminal = self.app.terminal(id).unwrap();
                    let final_text: String = terminal
                        .renderable_content()
                        .unwrap()
                        .viewport
                        .cells()
                        .iter()
                        .map(|cell| cell.c)
                        .collect();
                    let completion = terminal.shutdown_handle().completion().unwrap();
                    let valid = code == Some(7)
                        && final_text.contains("PIKE23_FINAL")
                        && completion.output_drained
                        && terminal.input_status().closed;
                    self.failed |= !valid;
                    self.record(&format!("retained session={} code={code:?} final_sentinel={} completion={completion:?}",id.get(),final_text.contains("PIKE23_FINAL")));
                    fs::write(self.config.output.join("final-synthetic.txt"), final_text).unwrap();
                    if self.config.mode != "rapid"
                        && fs::read(self.config.output.join("captured-keys.bin"))
                            .ok()
                            .as_deref()
                            != Some(keys())
                    {
                        self.failed = true;
                    }
                    task = Task::batch([task, self.close()]);
                }
                Lifecycle::Failed(_) => {
                    self.failed = true;
                    self.record("unexpected launch failure");
                    task = Task::batch([task, self.close()]);
                }
                _ => {}
            }
        }
        task
    }

    fn view(&self) -> Element<'_, ProbeMessage> {
        if self.failed && self.closing {
            text("Fixture failed — waiting for real cleanup").into()
        } else {
            self.app.view().map(ProbeMessage::App)
        }
    }
}
