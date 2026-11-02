//! Actual native Iced loop + production App + controlled native child processes.
//! This isolated manifest alone enables the finite compositor fault hook.
use iced::{Element, Task};
use iced_winit::graphics::compositor::SurfaceError;
use pike::app::{App, Control, Message};
use pike::session::Lifecycle;
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

type Handles = Arc<Mutex<Vec<iced_term::ShutdownHandle>>>;

struct Probe {
    app: App,
    mode: String,
    handles: Handles,
    shutdown: Arc<AtomicBool>,
    observed: BTreeSet<pike::identity::SessionId>,
    started_second: bool,
    armed: bool,
}
enum ProbeMessage {
    App(Message),
    Deadline,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments.get(1).is_some_and(|s| s == "--child") {
        let output = PathBuf::from(arguments.get(2).ok_or("child output")?);
        fs::write(
            output.join(format!("{}.pid", std::process::id())),
            std::process::id().to_string(),
        )?;
        println!("SYNTHETIC_RENDERER34");
        let mut byte = [0];
        while io::stdin().read(&mut byte)? != 0 {}
        return Ok(());
    }
    let mode = arguments
        .get(1)
        .and_then(|s| s.to_str())
        .ok_or("expected first-frame or attached")?
        .to_owned();
    if !matches!(
        mode.as_str(),
        "pre-ready" | "first-frame" | "attached" | "preparing" | "config-cancel"
    ) {
        return Err("unknown mode".into());
    }
    let output = std::path::absolute(PathBuf::from(arguments.get(2).ok_or("fresh output")?))?;
    if output.exists() {
        return Err("output exists".into());
    }
    fs::create_dir_all(&output)?;
    let handles = Handles::default();
    let retained = handles.clone();
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_observer = shutdown.clone();
    if mode == "first-frame" {
        iced_winit::error_fixtures::inject(SurfaceError::OutOfMemory, 1);
    }
    let boot_output = output.clone();
    let result = iced::application(
        move || {
            Probe::new(
                mode.clone(),
                boot_output.clone(),
                retained.clone(),
                shutdown_observer.clone(),
            )
        },
        Probe::update,
        Probe::view,
    )
    .executor::<pike::executor::SharedExecutor>()
    .subscription(|probe: &Probe| probe.app.subscription().map(ProbeMessage::App))
    .title("Pike synthetic renderer failure")
    .window_size((640., 400.))
    .exit_on_close_request(false)
    .run();
    let handles = handles.lock().unwrap();
    let completions_at_return = handles.iter().filter(|h| h.completion().is_some()).count();
    let count = handles.len();
    let pids: Vec<u32> = fs::read_dir(&output)?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "pid"))
        .map(|e| fs::read_to_string(e.path()).unwrap().parse().unwrap())
        .collect();
    let live_at_return = pids.iter().filter(|pid| alive(**pid)).count();
    let began = Instant::now();
    while handles.iter().any(|h| h.completion().is_none())
        && began.elapsed() < Duration::from_secs(5)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    let live_after_wait = pids.iter().filter(|pid| alive(**pid)).count();
    let cleanup_acknowledged = shutdown.load(Ordering::SeqCst);
    let summary = format!(
        "run_error={} handles={} completions_at_return={} children={} live_at_return={} live_after_wait={} pool_cleanup={}\n",
        result.is_err(),
        count,
        completions_at_return,
        pids.len(),
        live_at_return,
        live_after_wait,
        cleanup_acknowledged
    );
    fs::write(output.join("result.txt"), &summary)?;
    print!("{summary}");
    if result.is_ok()
        || completions_at_return != count
        || live_at_return != 0
        || live_after_wait != 0
        || ((!pids.is_empty() || count > 0) && !cleanup_acknowledged)
    {
        return Err(
            "renderer failure returned before proven native disposal; inspect result.txt".into(),
        );
    }
    Ok(())
}

impl Probe {
    fn view(&self) -> Element<'_, ProbeMessage> {
        self.app.view().map(ProbeMessage::App)
    }

    fn new(
        mode: String,
        output: PathBuf,
        handles: Handles,
        shutdown: Arc<AtomicBool>,
    ) -> (Self, Task<ProbeMessage>) {
        let profile = iced_term::settings::BackendSettings {
            program: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            args: vec!["--child".into(), output.to_string_lossy().into_owned()],
            ..Default::default()
        };
        let (mut app, task) = if mode == "pre-ready" {
            App::with_settings(profile)
        } else if mode == "config-cancel" {
            let path = output.join("settings.toml");
            fs::write(
                &path,
                format!(
                    "version=1\n[profiles.default]\nshell={:?}\nargs=[\"--child\",{:?}]\n",
                    profile.program,
                    output.to_string_lossy()
                ),
            )
            .unwrap();
            App::boot_with_config_path(path)
        } else {
            App::boot_with_settings(profile)
        };
        if mode == "pre-ready" {
            // Drive the actual boot receive while the one production executor
            // prepares its real child, before the native window's first frame.
            let message = wait_boot(task);
            drop(app.update(message));
            let id = app.active_session().unwrap();
            handles
                .lock()
                .unwrap()
                .push(app.terminal(id).unwrap().shutdown_handle());
            iced_winit::error_fixtures::inject(SurfaceError::OutOfMemory, 1);
            return (
                Self {
                    app,
                    mode,
                    handles,
                    shutdown,
                    observed: BTreeSet::new(),
                    started_second: false,
                    armed: true,
                },
                Task::none(),
            );
        }
        if mode == "first-frame" {
            for control in [Control::New, Control::NewTab, Control::NewProfile(0, 0)] {
                assert_eq!(app.update(Message::Control(control)).units(), 0);
            }
            drop(app.update(Message::SettingsLoaded(Ok(
                pike::settings::Settings::default(),
            ))));
            assert!(app.model().sessions().is_empty());
            assert!(!app.settings_pending());
        }
        let task = Task::batch([
            task.map(ProbeMessage::App),
            Task::perform(tokio::time::sleep(Duration::from_secs(10)), |_| {
                ProbeMessage::Deadline
            }),
        ]);
        (
            Self {
                app,
                mode,
                handles,
                shutdown,
                observed: BTreeSet::new(),
                started_second: false,
                armed: false,
            },
            task,
        )
    }

    fn update(&mut self, message: ProbeMessage) -> Task<ProbeMessage> {
        let ready = matches!(
            &message,
            ProbeMessage::App(Message::RendererEvent(
                iced_runtime::system::RendererEvent::Ready
            ))
        );
        if matches!(&message, ProbeMessage::App(Message::ShutdownComplete)) {
            self.shutdown.store(true, Ordering::SeqCst);
        }
        let task = match message {
            ProbeMessage::App(message) => self.app.update(message),
            ProbeMessage::Deadline => self.app.update(Message::RendererFailed(
                iced_runtime::system::RendererFailure::Other,
            )),
        };
        let ids: Vec<_> = self
            .app
            .model()
            .sessions()
            .iter()
            .filter(|(_, entry)| entry.lifecycle() == Lifecycle::Running)
            .map(|(id, _)| id)
            .collect();
        for id in &ids {
            if self.observed.insert(*id) {
                self.handles
                    .lock()
                    .unwrap()
                    .push(self.app.terminal(*id).unwrap().shutdown_handle());
            }
        }
        if !self.armed && ready && matches!(self.mode.as_str(), "preparing" | "config-cancel") {
            self.armed = true;
            let starting = self
                .app
                .model()
                .sessions()
                .iter()
                .filter(|(_, entry)| entry.lifecycle() == Lifecycle::Starting)
                .count();
            println!(
                "fault_at_arm_starting={} settings_pending={}",
                starting,
                self.app.settings_pending()
            );
            iced_winit::error_fixtures::inject(SurfaceError::OutOfMemory, 1);
        }
        if self.mode == "attached" && !self.armed {
            if ids.len() == 1 && !self.started_second {
                self.started_second = true;
                return Task::batch([
                    task.map(ProbeMessage::App),
                    self.app
                        .update(Message::Control(Control::NewTab))
                        .map(ProbeMessage::App),
                ]);
            }
            if ids.len() == 2 {
                self.armed = true;
                iced_winit::error_fixtures::inject(SurfaceError::OutOfMemory, 1);
            }
        }
        task.map(ProbeMessage::App)
    }
}

fn wait_boot(task: Task<Message>) -> Message {
    use std::task::{Context, Poll, Wake, Waker};
    struct WakeThread(std::thread::Thread);
    impl Wake for WakeThread {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(WakeThread(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut stream = iced_runtime::task::into_stream(task).unwrap();
    let began = Instant::now();
    loop {
        match stream.as_mut().poll_next(&mut context) {
            Poll::Ready(Some(iced_runtime::Action::Output(message))) => return message,
            Poll::Ready(Some(_)) => panic!("unexpected boot action"),
            Poll::Ready(None) => panic!("missing boot result"),
            Poll::Pending => {
                assert!(began.elapsed() < Duration::from_secs(10));
                std::thread::park_timeout(Duration::from_millis(10));
            }
        }
    }
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "stat="])
        .output()
        .unwrap()
        .status
        .success()
}
#[cfg(windows)]
fn alive(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    // SAFETY: Only the numeric PID written by this controlled child is opened;
    // the owned process handle is queried and closed before returning.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 259;
        let ok = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        ok != 0 && code == 259
    }
}
