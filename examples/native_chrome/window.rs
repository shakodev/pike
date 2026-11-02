//! One real native window, one controlled PTY, two finite measurement deadlines.
//! The raw event observer returns None, so counting redraws causes no messages.
use iced::{Element, Subscription, Task};
use pike::app::{App, CloseAction, Control, Message};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static REDRAWS: AtomicU64 = AtomicU64::new(0);

pub fn run(output: PathBuf) -> iced::Result {
    let destination = output.clone();
    iced::application(
        move || {
            let profile = iced_term::settings::BackendSettings {
                program: std::env::current_exe()
                    .expect("fixture executable")
                    .to_string_lossy()
                    .into_owned(),
                args: vec!["--child".into()],
                ..Default::default()
            };
            let (app, task) = App::with_settings(profile);
            (
                Probe {
                    app,
                    output: destination.clone(),
                    start: None,
                    counters: [0; 5],
                    cpu: String::new(),
                    redraws: 0,
                },
                Task::batch([
                    task.map(Event::App),
                    Task::perform(
                        async { tokio::time::sleep(Duration::from_secs(3)).await },
                        |_| Event::Begin,
                    ),
                ]),
            )
        },
        Probe::update,
        Probe::view,
    )
    .executor::<pike::executor::SharedExecutor>()
    .subscription(Probe::subscription)
    .theme(|p: &Probe| p.app.theme())
    .window_size((1200.0, 800.0))
    .title("Pike — controlled native chrome idle fixture")
    .exit_on_close_request(false)
    .run()?;
    assert!(
        output.join("native-idle.txt").is_file(),
        "native measurement did not complete"
    );
    Ok(())
}

enum Event {
    App(Message),
    Begin,
    End,
}

struct Probe {
    app: App,
    output: PathBuf,
    start: Option<Instant>,
    counters: [u64; 5],
    cpu: String,
    redraws: u64,
}

fn cpu() -> String {
    let result = std::process::Command::new("/bin/ps")
        .args(["-o", "time=,rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("native process counters");
    assert!(result.status.success());
    String::from_utf8(result.stdout).expect("ps UTF-8")
}

impl Probe {
    fn update(&mut self, event: Event) -> Task<Event> {
        match event {
            Event::App(message) => self.app.update(message).map(Event::App),
            Event::Begin => {
                assert!(
                    self.app
                        .active_session()
                        .and_then(|id| self.app.terminal(id))
                        .is_some(),
                    "controlled PTY did not attach"
                );
                self.counters = counters(&self.app);
                self.cpu = cpu();
                self.redraws = REDRAWS.load(Ordering::Relaxed);
                self.start = Some(Instant::now());
                Task::perform(
                    async { tokio::time::sleep(Duration::from_secs(24)).await },
                    |_| Event::End,
                )
            }
            Event::End => {
                let elapsed = self
                    .start
                    .expect("measurement began")
                    .elapsed()
                    .as_secs_f64();
                let redraws = REDRAWS.load(Ordering::Relaxed) - self.redraws;
                let final_cpu = cpu();
                let deltas: Vec<_> = counters(&self.app)
                    .into_iter()
                    .zip(self.counters)
                    .map(|(a, b)| a - b)
                    .collect();
                fs::write(self.output.join("native-idle.txt"), format!("platform={} arch={} renderer=wgpu native_window=1200x800 controlled_ptys=1\nsettle_s=3 interval_s={elapsed:.3} observed_native_redraws={redraws}\nps_before_time_rss={}ps_after_time_rss={}counter_order=font_measurements,pty_resizes,grid_resizes,cache_invalidations,snapshots\ncounter_delta={deltas:?}\n", std::env::consts::OS, std::env::consts::ARCH, self.cpu, final_cpu)).expect("measurement report");
                let close = self.app.update(Message::CloseWindow);
                let confirm = self.app.close_confirmation().map(|(token, _)| token);
                let confirm = confirm
                    .map(|token| {
                        self.app.update(Message::Control(Control::CloseAction(
                            CloseAction::Confirm(token),
                        )))
                    })
                    .unwrap_or_else(Task::none);
                Task::batch([close, confirm]).map(Event::App)
            }
        }
    }
    fn subscription(&self) -> Subscription<Event> {
        Subscription::batch([
            self.app.subscription().map(Event::App),
            iced::event::listen_raw(|event, _, _| {
                if matches!(
                    event,
                    iced::Event::Window(iced::window::Event::RedrawRequested(_))
                ) {
                    REDRAWS.fetch_add(1, Ordering::Relaxed);
                }
                None::<Event>
            }),
        ])
    }
    fn view(&self) -> Element<'_, Event> {
        self.app.view().map(Event::App)
    }
}

pub fn counters(app: &App) -> [u64; 5] {
    app.model()
        .sessions()
        .iter()
        .filter_map(|(id, _)| app.terminal(id))
        .fold([0; 5], |mut sum, terminal| {
            let work = terminal.work_stats();
            for (total, value) in sum.iter_mut().zip([
                work.font_measurements,
                work.pty_resizes,
                work.grid_resizes,
                work.cache_invalidations,
                terminal.snapshot_stats().snapshots,
            ]) {
                *total += value;
            }
            sum
        })
}
