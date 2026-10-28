//! Opt-in actual OS -> Winit -> Iced widget -> PTY layout probe.
//! Driven only with known synthetic scan codes by tools/test-keyboard-layout.ps1.
use iced::{Element, Subscription, Task};
use iced_term::{AlacrittyEvent, BackendCommand, Command, Terminal, TerminalView};
use pike::executor::SharedExecutor;
use std::{fs, io, path::PathBuf, time::Duration};

const EXPECTED: [&[u8]; 7] = [b"a", b"a", b"a", b"\x1bb", b"@", "é".as_bytes(), b"!"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::path::absolute(
        std::env::args()
            .nth(1)
            .ok_or("expected fresh output directory")?,
    )?;
    if output.exists() {
        return Err("choose a fresh output directory".into());
    }
    fs::create_dir_all(&output)?;
    let fixture = std::env::current_exe()?
        .with_file_name(format!("input_fixture{}", std::env::consts::EXE_SUFFIX));
    let bytes: Vec<_> = EXPECTED
        .iter()
        .flat_map(|bytes| bytes.iter().copied())
        .collect();
    let terminal = Terminal::new(
        15001,
        iced_term::settings::Settings {
            backend: iced_term::settings::BackendSettings {
                program: fixture.to_str().ok_or("non-Unicode fixture path")?.into(),
                args: vec![
                    "echo".into(),
                    bytes.len().to_string(),
                    output
                        .join("raw")
                        .to_str()
                        .ok_or("non-Unicode output path")?
                        .into(),
                    "keyboard-normal".into(),
                ],
                ..Default::default()
            },
            ..Default::default()
        },
    )?;
    let expected_hash = bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    let result = format!("{},{expected_hash}\n", bytes.len());
    let directory = output.clone();
    let initial = std::cell::RefCell::new(Some(terminal));
    iced::application(
        move || {
            (
                Probe {
                    terminal: initial
                        .borrow_mut()
                        .take()
                        .expect("single probe initialization"),
                    directory: directory.clone(),
                    written: 0,
                    result: result.clone(),
                    closing: false,
                },
                Task::perform(tokio::time::sleep(Duration::from_secs(40)), |_| {
                    Message::Failed("OS-input fixture deadline".into())
                }),
            )
        },
        Probe::update,
        Probe::view,
    )
    .executor::<SharedExecutor>()
    .subscription(Probe::subscription)
    .title("Pike synthetic keyboard layout probe")
    .window_size((640.0, 400.0))
    .run()?;
    if !output.join("completed.txt").is_file() {
        return Err("OS-input fixture did not complete; see synthetic failure.txt".into());
    }
    Ok(())
}

#[derive(Debug, Clone)]
enum Message {
    Terminal(iced_term::Event),
    Focused,
    Finished(Result<(), String>),
    Closed(bool),
    Failed(String),
}

struct Probe {
    terminal: Terminal,
    directory: PathBuf,
    written: usize,
    result: String,
    closing: bool,
}

impl Probe {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Terminal(iced_term::Event::BackendCall(id, command)) => {
                if id != self.terminal.id {
                    return self.fail("event reached another session".into());
                }
                let ready = matches!(&command, BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Title(title)) if title == "pike:keys-ready");
                let wrote = if let BackendCommand::Write(bytes) = &command {
                    if EXPECTED.get(self.written).copied() != Some(bytes.as_slice()) {
                        return self.fail(format!(
                            "OS key output mismatch at synthetic event {}",
                            self.written + 1
                        ));
                    }
                    self.written += 1;
                    true
                } else {
                    false
                };
                let action = self.terminal.handle(Command::ProxyToBackend(command));
                if matches!(action, iced_term::actions::Action::InputRejected(_)) {
                    return self.fail("synthetic layout input was rejected".into());
                }
                self.terminal.refresh();
                if ready {
                    return TerminalView::focus(self.terminal.widget_id().clone())
                        .chain(Task::done(Message::Focused));
                }
                if wrote {
                    if let Err(error) = fs::write(
                        self.directory.join(format!("write-{}", self.written)),
                        "passed\n",
                    ) {
                        return self.fail(error.to_string());
                    }
                    if self.written == EXPECTED.len() {
                        let directory = self.directory.clone();
                        let expected = self.result.clone();
                        return Task::perform(
                            async move {
                                tokio::time::timeout(Duration::from_secs(10), async {
                                    while !directory.join("raw/done").is_file() {
                                        tokio::time::sleep(Duration::from_millis(10)).await;
                                    }
                                    if fs::read_to_string(directory.join("raw/result"))? != expected
                                    {
                                        return Err(io::Error::other(
                                            "native raw layout byte/hash mismatch",
                                        ));
                                    }
                                    Ok(())
                                })
                                .await
                                .map_err(|_| "raw layout fixture deadline".to_string())?
                                .map_err(|error: io::Error| error.to_string())
                            },
                            Message::Finished,
                        );
                    }
                }
            }
            Message::Focused => {
                if let Err(error) =
                    fs::write(self.directory.join("ready"), std::process::id().to_string())
                {
                    return self.fail(error.to_string());
                }
            }
            Message::Finished(Ok(())) => return self.close(true),
            Message::Finished(Err(error)) | Message::Failed(error) => return self.fail(error),
            Message::Closed(passed) => {
                if passed
                    && !self.directory.join("failure.txt").exists()
                    && let Err(error) = fs::write(
                        self.directory.join("completed.txt"),
                        "passed: actual OS layout events and native PTY bytes\n",
                    )
                {
                    let _ = fs::write(self.directory.join("failure.txt"), error.to_string());
                }
                return iced::exit();
            }
        }
        Task::none()
    }
    fn fail(&mut self, error: String) -> Task<Message> {
        let _ = fs::write(self.directory.join("failure.txt"), error);
        self.close(false)
    }
    fn close(&mut self, passed: bool) -> Task<Message> {
        if self.closing {
            return Task::none();
        }
        self.closing = true;
        self.terminal.request_close();
        let handle = self.terminal.shutdown_handle();
        Task::perform(
            async move {
                let completion = tokio::time::timeout(Duration::from_secs(10), handle.wait()).await;
                passed
                    && completion.is_ok_and(|completion| {
                        completion.reason == iced_term::ExitReason::Requested
                    })
            },
            Message::Closed,
        )
    }
    fn subscription(&self) -> Subscription<Message> {
        self.terminal.subscription().map(Message::Terminal)
    }
    fn view(&self) -> Element<'_, Message> {
        TerminalView::show(&self.terminal).map(Message::Terminal)
    }
}
