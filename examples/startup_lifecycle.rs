//! Opt-in native Iced/ConPTY startup probe. Never launches a user's shell.
// Compile the production pool source so deterministic delay injection remains
// crate-private, with no probe feature or additional production API/dependency.
#[path = "../src/launch.rs"]
#[allow(dead_code)] // The probe deliberately exercises cancellation without UI attachment.
mod launch;
#[path = "compatibility/report.rs"]
mod report;

use iced::widget::{column, container, text};
use iced::{Element, Fill, Subscription, Task, window};
use iced_term::{ExitReason, PreparedTerminal, ShutdownHandle};
use launch::{CancelLaunch, LaunchError, LaunchLimits, LaunchPool, PreparedSession};
use pike::executor;
use std::io::{self, Write};
use std::num::NonZeroU16;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(10);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--fixture") {
        std::fs::write(&args[2], std::process::id().to_string())?;
        println!("PIKE_SYNTHETIC_STARTUP_FIXTURE");
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        return Ok(());
    }
    let mode = args
        .get(1)
        .ok_or("expected target_close, window_close or failure")?;
    if !["target_close", "window_close", "failure"].contains(&mode.as_str()) {
        return Err("unknown scenario".into());
    }
    let directory = std::path::absolute(args.get(2).ok_or("expected fresh output directory")?)?;
    if directory.exists() {
        return Err("choose a fresh output directory".into());
    }
    std::fs::create_dir_all(&directory)?;
    let mode = mode.clone();
    iced::application(
        move || Probe::new(mode.clone(), directory.clone()),
        Probe::update,
        Probe::view,
    )
    .executor::<executor::SharedExecutor>()
    .subscription(Probe::subscription)
    .title("Pike synthetic startup probe")
    .window_size((640.0, 320.0))
    .exit_on_close_request(false)
    .run()?;
    Ok(())
}

struct Probe {
    cycle: usize,
    mode: String,
    directory: PathBuf,
    pool: LaunchPool,
    report: report::Report,
    cancels: Vec<CancelLaunch>,
    releases: Vec<mpsc::Sender<()>>,
    handles: Arc<Mutex<Vec<ShutdownHandle>>>,
    max_dispatch: Duration,
    began: Option<Instant>,
    released: Option<Instant>,
    last_update: Option<Instant>,
    updates: usize,
    max_update_gap: Duration,
    received: usize,
    drained: bool,
    finished: bool,
    close_requested: bool,
}

enum Message {
    Begin,
    WorkersReady,
    Cancel,
    Release,
    Launched(Result<Box<PreparedSession>, LaunchError>),
    Drained,
    Tick,
    Captured(&'static str, window::Screenshot),
    Exit,
    Next,
    Timeout,
}

fn after(duration: Duration, message: Message) -> Task<Message> {
    Task::perform(tokio::time::sleep(duration), move |_| message)
}

impl Probe {
    fn new(mode: String, directory: PathBuf) -> (Self, Task<Message>) {
        let report = report::Report::new(&directory).unwrap();
        std::fs::write(directory.join("timing.csv"), "cycle,scenario,requested,admitted,rejected,max_dispatch_us,updates,max_update_gap_ms,recovery_ms,backends\n").unwrap();
        let pool = LaunchPool::new(LaunchLimits {
            jobs: NonZeroU16::new(2).unwrap(),
            sessions: NonZeroU16::new(2).unwrap(),
        });
        (
            Self {
                cycle: 0,
                mode,
                directory,
                pool,
                report,
                cancels: Vec::new(),
                releases: Vec::new(),
                handles: Arc::new(Mutex::new(Vec::new())),
                max_dispatch: Duration::ZERO,
                began: None,
                released: None,
                last_update: None,
                updates: 0,
                max_update_gap: Duration::ZERO,
                received: 0,
                drained: false,
                finished: false,
                close_requested: false,
            },
            Task::batch([
                after(Duration::from_secs(1), Message::Begin),
                after(Duration::from_secs(15), Message::Timeout),
            ]),
        )
    }

    fn note(&mut self, event: &str) {
        self.report
            .record(event, 0, self.received, self.updates)
            .unwrap();
    }

    fn phase(&mut self, phase: &str) {
        self.note(phase);
        let phase = if phase == "finished" {
            phase.into()
        } else {
            format!("{phase}-{}", self.cycle)
        };
        std::fs::write(self.directory.join("phase.txt"), phase).unwrap();
    }

    fn capture(&self, name: &'static str) -> Task<Message> {
        window::latest().and_then(move |id| {
            window::screenshot(id).map(move |shot| Message::Captured(name, shot))
        })
    }

    fn begin(&mut self) -> Task<Message> {
        self.began = Some(Instant::now());
        self.last_update = self.began;
        let ui_thread = std::thread::current().id();
        let mut tasks = Vec::new();
        let mut entered = Vec::new();
        for index in 0..32 {
            let (release, gate) = mpsc::channel();
            let (ready, observed) = tokio::sync::oneshot::channel();
            let handles = self.handles.clone();
            let directory = self.directory.clone();
            let fail = self.mode == "failure";
            let start = Instant::now();
            let launch = self.pool.start_with(move |id| {
                assert_ne!(std::thread::current().id(), ui_thread);
                let _ = ready.send(());
                gate.recv_timeout(DEADLINE)
                    .expect("UI must release preparation workers");
                if fail {
                    return Err(io::Error::other("synthetic private launch failure"));
                }
                let marker = directory.join(format!("{}.pid", id.get()));
                let prepared = PreparedTerminal::new(
                    id.get(),
                    iced_term::settings::BackendSettings {
                        program: std::env::current_exe()?
                            .to_str()
                            .ok_or_else(|| io::Error::other("non-Unicode fixture path"))?
                            .into(),
                        args: vec![
                            "--fixture".into(),
                            marker
                                .to_str()
                                .ok_or_else(|| io::Error::other("non-Unicode marker path"))?
                                .into(),
                        ],
                        ..Default::default()
                    },
                )?;
                // Probe-only wait ensures the synthetic PID is observable before
                // returning a guaranteed late success, even on a slow machine.
                let wait = Instant::now();
                while std::fs::read_to_string(&marker)
                    .ok()
                    .is_none_or(|pid| pid.is_empty())
                {
                    if wait.elapsed() > DEADLINE {
                        return Err(io::Error::other("fixture readiness deadline"));
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                handles.lock().unwrap().push(prepared.shutdown_handle());
                Ok(prepared)
            });
            self.max_dispatch = self.max_dispatch.max(start.elapsed());
            if index < 2 {
                let launch = launch.expect("two requests must be admitted");
                self.cancels.push(launch.cancellation());
                self.releases.push(release);
                entered.push(observed);
                tasks.push(Task::perform(launch.receive(), Message::Launched));
            } else {
                assert!(matches!(launch, Err(LaunchError::Busy)));
            }
        }
        tasks.push(Task::perform(
            async move {
                for ready in entered {
                    ready.await.unwrap();
                }
            },
            |_| Message::WorkersReady,
        ));
        Task::batch(tasks)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Begin => return self.begin(),
            Message::WorkersReady => {
                self.phase("baseline");
                return Task::batch([
                    if self.cycle == 0 {
                        self.capture("starting.ppm")
                    } else {
                        Task::none()
                    },
                    after(Duration::from_millis(250), Message::Cancel),
                    after(Duration::from_millis(500), Message::Release),
                ]);
            }
            Message::Cancel => {
                self.phase("canceled");
                if self.mode == "window_close" {
                    self.close_requested = true;
                    return Task::perform(self.pool.shutdown(), |_| Message::Drained);
                }
                for cancel in &self.cancels {
                    cancel.cancel();
                }
            }
            Message::Release => {
                self.phase("released");
                self.released = Some(Instant::now());
                for release in self.releases.drain(..) {
                    release.send(()).unwrap();
                }
                if self.mode != "window_close" {
                    return Task::perform(self.pool.shutdown(), |_| Message::Drained);
                }
            }
            Message::Launched(result) => {
                let expected = if self.mode == "failure" {
                    LaunchError::Spawn(io::ErrorKind::Other)
                } else {
                    LaunchError::Canceled
                };
                assert_eq!(result.unwrap_err(), expected);
                self.received += 1;
                self.note("result");
            }
            Message::Drained => {
                self.drained = true;
            }
            Message::Tick if self.began.is_some() && !self.finished => {
                let now = Instant::now();
                if let Some(previous) = self.last_update.replace(now) {
                    self.max_update_gap = self.max_update_gap.max(now - previous);
                }
                self.updates += 1;
            }
            Message::Tick => {}
            Message::Captured(name, shot) => {
                report::screenshot(&self.directory.join(name), shot).unwrap()
            }
            Message::Exit => {
                self.phase("finished");
                return iced::exit();
            }
            Message::Next => {
                self.cycle += 1;
                self.pool = LaunchPool::new(LaunchLimits {
                    jobs: NonZeroU16::new(2).unwrap(),
                    sessions: NonZeroU16::new(2).unwrap(),
                });
                self.max_dispatch = Duration::ZERO;
                self.released = None;
                self.updates = 0;
                self.max_update_gap = Duration::ZERO;
                self.received = 0;
                self.drained = false;
                self.finished = false;
                self.close_requested = false;
                self.phase("starting");
                return self.begin();
            }
            Message::Timeout => {
                panic!("startup probe deadline; native cleanup was not acknowledged")
            }
        }
        if self.drained && self.received == 2 && !self.finished {
            let mut handles = self.handles.lock().unwrap();
            assert_eq!(handles.len(), if self.mode == "failure" { 0 } else { 2 });
            for handle in handles.iter() {
                assert_eq!(handle.completion().unwrap().reason, ExitReason::Requested);
            }
            assert!(
                self.updates > 1,
                "Iced must process updates during preparation"
            );
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(self.directory.join("timing.csv"))
                .unwrap();
            writeln!(
                file,
                "{},{},32,2,30,{:.3},{},{:.3},{:.3},{}",
                self.cycle,
                self.mode,
                self.max_dispatch.as_secs_f64() * 1e6,
                self.updates,
                self.max_update_gap.as_secs_f64() * 1e3,
                self.released.unwrap().elapsed().as_secs_f64() * 1e3,
                handles.len()
            )
            .unwrap();
            // Release observation/notifier ownership too before measuring recovery.
            handles.clear();
            drop(handles);
            self.cancels.clear();
            self.finished = true;
            self.phase("recovered");
            return Task::batch([
                if self.cycle == 2 {
                    self.capture("recovered.ppm")
                } else {
                    Task::none()
                },
                after(
                    Duration::from_secs(1),
                    if self.cycle == 2 {
                        Message::Exit
                    } else {
                        Message::Next
                    },
                ),
            ]);
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            if self.began.is_some() && !self.finished {
                iced::time::every(Duration::from_millis(5)).map(|_| Message::Tick)
            } else {
                Subscription::none()
            },
            window::close_requests().map(|_| Message::Cancel),
        ])
    }

    fn view(&self) -> Element<'_, Message> {
        container(
            column![
                text("Pike synthetic startup probe").size(24),
                text(format!("Scenario: {}", self.mode)),
                text(if self.finished {
                    "Native cleanup acknowledged"
                } else if self.close_requested {
                    "Closing window; waiting for native cleanup"
                } else {
                    "Starting terminals; UI remains active"
                }),
                text(format!(
                    "32 requested / 2 admitted / 30 rejected; UI updates: {}",
                    self.updates
                )),
            ]
            .spacing(16),
        )
        .padding(24)
        .width(Fill)
        .height(Fill)
        .into()
    }
}
