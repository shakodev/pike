//! Opt-in synthetic byte/mode/stalled-reader probe on the existing PTY stack.
use iced::advanced::renderer::Headless;
use iced::advanced::subscription::into_recipes;
use iced::advanced::widget::{Tree, operation};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::futures::{StreamExt, stream};
use iced::keyboard::{self, Modifiers};
use iced::{Executor, Size};
use iced_term::actions::Action;
use iced_term::{
    BackendCommand, Command, Event, InputError, PASTE_BYTE_LIMIT, PasteError, Terminal,
};
use pike::executor::SharedExecutor;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

fn check(ok: bool, reason: &'static str) -> io::Result<()> {
    if ok {
        Ok(())
    } else {
        Err(io::Error::other(reason))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--fixture") {
        return fixture(&args[2], Path::new(&args[3])).map_err(Into::into);
    }
    let output = std::path::absolute(args.get(1).ok_or("expected fresh output directory")?)?;
    check(!output.exists(), "choose a fresh output directory")?;
    fs::create_dir_all(&output)?;
    let executor = SharedExecutor::new()?;
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or("offscreen renderer unavailable")?;
    for (id, mode) in [(16001, "bytes"), (16002, "stall"), (16003, "close")] {
        let mut terminal = Terminal::new(
            id,
            iced_term::settings::Settings {
                backend: iced_term::settings::BackendSettings {
                    program: std::env::current_exe()?
                        .to_str()
                        .ok_or("non-Unicode executable path")?
                        .into(),
                    args: vec![
                        "--fixture".into(),
                        mode.into(),
                        output.to_str().ok_or("non-Unicode output path")?.into(),
                    ],
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        let focus = iced_term::WindowFocus::default();
        focus.set_focused(true);
        terminal.set_window_focus(focus);
        terminal.handle(Command::ProxyToBackend(BackendCommand::Resize(
            Some(Size::new(640.0, 480.0)),
            None,
        )));
        let result = executor.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(15),
                run(&mut terminal, &renderer, mode, &output),
            )
            .await
        });
        let handle = terminal.shutdown_handle();
        drop(terminal);
        let completion = executor.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), handle.wait()).await
        })?;
        fs::write(
            output.join(format!("{mode}-cleanup.txt")),
            format!("{completion:?}\n"),
        )?;
        result??;
        if mode == "bytes" {
            check(completion.output_drained, "fixture output was not drained")?;
            check(
                completion.exit_code == Some(0),
                "byte fixture did not exit successfully",
            )?;
        } else {
            // Explicit cancellation is not a claim that unwritten input or
            // output was drained; the existing backend reports that honestly.
            check(
                completion.reason == iced_term::ExitReason::Requested,
                "stalled fixture was not cancelled",
            )?;
        }
    }
    fs::write(output.join("completed.txt"), "passed\n")?;
    Ok(())
}

async fn run(
    terminal: &mut Terminal,
    renderer: &iced::Renderer,
    mode: &str,
    output: &Path,
) -> io::Result<()> {
    let recipe = into_recipes(terminal.subscription())
        .pop()
        .ok_or_else(|| io::Error::other("missing subscription"))?;
    let mut events = recipe.stream(stream::pending().boxed());
    let mut phase = 0;
    let mut tree = Tree::empty();
    // Staged in normal mode below, deliberately handled after mode enable.
    let mut pending = None;
    while let Some(Event::BackendCall(id, command)) = events.next().await {
        check(id == terminal.id, "wrong session origin")?;
        let action = terminal.handle(Command::ProxyToBackend(command));
        match action {
            Action::ChangeTitle(title) if title == "pike:paste:ready" => {
                check(phase == 0, "duplicate ready")?;
                if mode == "stall" {
                    return stalled(terminal, renderer, &mut tree, output).await;
                }
                if mode == "close" {
                    return close_pending(terminal, renderer, &mut tree).await;
                }
                let before = admitted(terminal);
                terminal
                    .preflight_paste("C\r\nD")
                    .map_err(io::Error::other)?;
                check(
                    terminal.paste(None) == Action::PasteRejected(PasteError::ClipboardUnavailable),
                    "missing clipboard error",
                )?;
                check(
                    matches!(
                        terminal.paste(Some("x".repeat(PASTE_BYTE_LIMIT + 1))),
                        Action::PasteRejected(PasteError::TooLarge { .. })
                    ),
                    "oversize accepted",
                )?;
                check(
                    admitted(terminal) == before,
                    "rejected paste admitted a prefix",
                )?;
                keyboard_batch(
                    terminal,
                    renderer,
                    &mut tree,
                    Some("A\r\n界\t\x03\x1b[201~\nB".into()),
                    &[
                        key("p", Modifiers::empty(), Some("p")),
                        key("c", Modifiers::CTRL, None),
                        key("V", Modifiers::CTRL | Modifiers::SHIFT, Some("V")),
                    ],
                )?;
                let before_pending = admitted(terminal);
                pending = Some(keyboard_commands(
                    terminal,
                    renderer,
                    &mut tree,
                    Some("C\r\nD".into()),
                    &[key("V", Modifiers::CTRL | Modifiers::SHIFT, None)],
                )?);
                check(
                    admitted(terminal) == before_pending,
                    "staged paste wrote before ordered handling",
                )?;
                send(terminal, b"S")?;
                phase = 1;
            }
            Action::ChangeTitle(title) if title == "pike:paste:on" => {
                check(phase == 1, "mode-on phase mismatch")?;
                send(terminal, b"<")?;
                check(
                    matches!(
                        terminal.paste(Some("x".repeat(PASTE_BYTE_LIMIT + 1))),
                        Action::PasteRejected(PasteError::TooLarge { .. })
                    ),
                    "oversize accepted between prior input and paste",
                )?;
                handle_commands(terminal, pending.take().unwrap())?;
                send(terminal, b">")?;
                keyboard_batch(
                    terminal,
                    renderer,
                    &mut tree,
                    Some("Z\x1b[200~\x03\u{85}".into()),
                    &[key_event(
                        keyboard::Key::Named(keyboard::key::Named::Insert),
                        Modifiers::SHIFT,
                        None,
                    )],
                )?;
                phase = 2;
            }
            Action::ChangeTitle(title) if title == "pike:paste:off" => {
                check(phase == 2, "mode-off phase mismatch")?;
                accepted(terminal.paste(Some("E\nF".into())))?;
                // Near the original-byte limit, including a split UTF-8 scalar
                // at the existing 8KiB input chunk boundary.
                accepted(terminal.paste(Some(large_text())))?;
                send(terminal, b"!")?;
                phase = 3;
            }
            Action::BackendCompleted(completion) => {
                check(
                    phase == 3 && completion.exit_code == Some(0),
                    "byte fixture failed",
                )?;
                fs::write(
                    output.join("bytes-passed.txt"),
                    "normal/bracketed/mode-change/Unicode/controls/order/limit/no-Enter passed\n",
                )?;
                return Ok(());
            }
            Action::InputRejected(_) | Action::PasteRejected(_) | Action::EventBridgeError(_) => {
                return Err(io::Error::other("unexpected input/bridge rejection"));
            }
            _ => {}
        }
    }
    Err(io::Error::other("subscription ended before completion"))
}

fn admitted(terminal: &Terminal) -> usize {
    let s = terminal.input_status();
    s.outstanding_bytes + s.written_bytes + s.cancelled_bytes
}

struct Clipboard(Option<String>);
impl clipboard::Clipboard for Clipboard {
    fn read(&self, _: clipboard::Kind) -> Option<String> {
        self.0.clone()
    }
    fn write(&mut self, _: clipboard::Kind, _: String) {
        panic!("unexpected clipboard write")
    }
}

fn key(name: &str, modifiers: Modifiers, text: Option<&str>) -> iced::Event {
    key_event(keyboard::Key::Character(name.into()), modifiers, text)
}

fn key_event(key: keyboard::Key, modifiers: Modifiers, text: Option<&str>) -> iced::Event {
    iced::Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    })
}

fn keyboard_commands(
    terminal: &Terminal,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    text: Option<String>,
    keys: &[iced::Event],
) -> io::Result<Vec<Event>> {
    let mut clipboard = Clipboard(text);
    let size = Size::new(640.0, 480.0);
    let mut events = if keys.len() > 1 {
        vec![
            Event::BackendCall(
                terminal.id,
                BackendCommand::MouseReport(
                    iced_term::MouseButton::LeftButton,
                    Modifiers::empty(),
                    iced_term::AlacrittyPoint::default(),
                    true,
                ),
            ),
            Event::BackendCall(terminal.id, BackendCommand::FocusChanged(true)),
        ]
    } else {
        Vec::new()
    };
    for key in keys {
        let mut view = iced_term::TerminalView::show(terminal);
        tree.diff(view.as_widget());
        let node = view
            .as_widget_mut()
            .layout(tree, renderer, &layout::Limits::new(size, size));
        view.as_widget_mut().operate(
            tree,
            Layout::new(&node),
            renderer,
            &mut operation::focusable::focus::<()>(terminal.widget_id().clone()),
        );
        let mut shell = Shell::new(&mut events);
        view.as_widget_mut().update(
            tree,
            key,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut clipboard,
            &mut shell,
            &iced::Rectangle::with_size(size),
        );
        check(
            shell.is_event_captured(),
            "paste/key shortcut escaped the widget",
        )?;
    }
    Ok(events)
}

fn keyboard_batch(
    terminal: &mut Terminal,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    text: Option<String>,
    keys: &[iced::Event],
) -> io::Result<()> {
    let events = keyboard_commands(terminal, renderer, tree, text, keys)?;
    handle_commands(terminal, events)
}

fn handle_commands(terminal: &mut Terminal, events: Vec<Event>) -> io::Result<()> {
    // Deliberately delay application handling until all key/paste events have
    // run, reproducing Iced event batching. Receipts must never resend bytes.
    for Event::BackendCall(id, command) in events {
        check(id == terminal.id, "wrong widget paste origin")?;
        accepted(terminal.handle(Command::ProxyToBackend(command.clone())))?;
        if matches!(command, BackendCommand::Paste(_)) {
            accepted(terminal.handle(Command::ProxyToBackend(command)))?;
        }
    }
    Ok(())
}

async fn close_pending(
    terminal: &mut Terminal,
    renderer: &iced::Renderer,
    tree: &mut Tree,
) -> io::Result<()> {
    let keys = [key("V", Modifiers::CTRL | Modifiers::SHIFT, None)];
    let events = keyboard_commands(
        terminal,
        renderer,
        tree,
        Some("MUST_NOT_SEND".into()),
        &keys,
    )?;
    check(
        admitted(terminal) == 0,
        "queued paste was treated as accepted",
    )?;
    terminal.handle(Command::ProxyToBackend(BackendCommand::RequestShutdown));
    for Event::BackendCall(_, command) in events {
        terminal.handle(Command::ProxyToBackend(command));
    }
    check(
        admitted(terminal) == 0,
        "earlier close was overtaken by paste",
    )?;
    tokio::time::timeout(Duration::from_secs(5), terminal.shutdown_handle().wait())
        .await
        .map_err(io::Error::other)?;
    Ok(())
}

fn accepted(action: Action) -> io::Result<()> {
    check(action == Action::Ignore, "synthetic input was rejected")
}

fn send(terminal: &mut Terminal, bytes: &[u8]) -> io::Result<()> {
    accepted(
        terminal.handle(Command::ProxyToBackend(BackendCommand::Write(
            bytes.to_vec(),
        ))),
    )
}

async fn stalled(
    terminal: &mut Terminal,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    output: &Path,
) -> io::Result<()> {
    // Preflight succeeds while credit is available. Earlier deferred writes
    // then consume that credit; handling the pending paste must reject it whole.
    let pending = keyboard_commands(
        terminal,
        renderer,
        tree,
        Some("x".repeat(PASTE_BYTE_LIMIT)),
        &[key("V", Modifiers::CTRL | Modifiers::SHIFT, None)],
    )?;
    for _ in 0..16 {
        let action = terminal.handle(Command::ProxyToBackend(BackendCommand::Write(
            vec![b'w'; iced_term::INPUT_BYTE_LIMIT],
        )));
        if matches!(action, Action::InputRejected(InputError::Full { .. })) {
            break;
        }
        accepted(action)?;
        if terminal.input_status().outstanding_bytes > PASTE_BYTE_LIMIT {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let before = admitted(terminal);
    let mut full = false;
    for Event::BackendCall(_, command) in pending {
        let paste = matches!(command, BackendCommand::Paste(_));
        let action = terminal.handle(Command::ProxyToBackend(command));
        if paste {
            check(
                matches!(
                    action,
                    Action::PasteRejected(PasteError::Input(InputError::Full { .. }))
                ),
                "stale preflight did not recheck actual credit",
            )?;
            full = true;
        } else {
            accepted(action)?;
        }
    }
    check(
        full && admitted(terminal) == before,
        "rejected framed paste changed accepted bytes",
    )?;
    let mut record = fs::File::create(output.join("stall.csv"))?;
    writeln!(
        record,
        "attempt,outstanding,queued,written,cancelled,rejected,admission_us"
    )?;
    let mut rejected = 0;
    for attempt in 0..32 {
        let before = admitted(terminal);
        let start = Instant::now();
        let action = terminal.paste(Some("x".repeat(PASTE_BYTE_LIMIT)));
        let admission_us = start.elapsed().as_micros();
        let failed = matches!(
            action,
            Action::PasteRejected(PasteError::Input(InputError::Full { .. }))
        );
        if failed {
            rejected += 1;
            check(admitted(terminal) == before, "full paste admitted a prefix")?;
        } else {
            accepted(action)?;
        }
        let status = terminal.input_status();
        check(
            status.outstanding_bytes <= iced_term::INPUT_BYTE_LIMIT,
            "input budget exceeded",
        )?;
        writeln!(
            record,
            "{attempt},{},{},{},{},{failed},{admission_us}",
            status.outstanding_bytes,
            status.queued_bytes,
            status.written_bytes,
            status.cancelled_bytes
        )?;
        // Finite probe sampling only; no production polling/task is added.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    check(rejected != 0, "stalled fixture did not reach backpressure")?;
    let start = Instant::now();
    terminal.request_close();
    let request_us = start.elapsed().as_micros();
    check(
        terminal.input_status().queued_bytes == 0,
        "close retained queued paste",
    )?;
    check(
        terminal.paste(Some("late".into()))
            == Action::PasteRejected(PasteError::Input(InputError::Closed)),
        "closed session admitted paste",
    )?;
    let completion =
        tokio::time::timeout(Duration::from_secs(5), terminal.shutdown_handle().wait())
            .await
            .map_err(io::Error::other)?;
    fs::write(
        output.join("stall-summary.txt"),
        format!(
            "rejections={rejected}\nclose_request_us={request_us}\ncompletion_us={}\n{completion:?}\n",
            start.elapsed().as_micros()
        ),
    )?;
    Ok(())
}

fn large_text() -> String {
    let mut text = "x".repeat(8191);
    text.push('界');
    text.extend(std::iter::repeat_n('z', PASTE_BYTE_LIMIT - text.len()));
    text
}

fn fixture(mode: &str, output: &Path) -> io::Result<()> {
    raw_console()?;
    fs::write(
        output.join(format!("{mode}.pid")),
        std::process::id().to_string(),
    )?;
    let mut writer = io::stdout().lock();
    write!(
        writer,
        "\x1b[?2004{}\x1b]2;pike:paste:ready\x07",
        if mode == "stall" { 'h' } else { 'l' }
    )?;
    if mode == "bytes" {
        write!(writer, "\x1b[?1004h\x1b[?1000h\x1b[?1006h")?;
    }
    writer.flush()?;
    if mode != "bytes" {
        loop {
            std::thread::park();
        }
    }
    let mut input = io::stdin().lock();
    expect(
        &mut input,
        "\x1b[<0;1;1M\x1b[Ip\x03A\r界\t[201~\rBS".as_bytes(),
    )?;
    write!(writer, "\x1b[?2004h\x1b]2;pike:paste:on\x07")?;
    writer.flush()?;
    expect(
        &mut input,
        b"<\x1b[200~C\nD\x1b[201~>\x1b[200~Z[200~\x1b[201~",
    )?;
    write!(writer, "\x1b[?2004l\x1b]2;pike:paste:off\x07")?;
    writer.flush()?;
    expect(&mut input, b"E\rF")?;
    expect(&mut input, large_text().as_bytes())?;
    expect(&mut input, b"!")?;
    Ok(())
}

fn expect(reader: &mut impl Read, mut expected: &[u8]) -> io::Result<()> {
    let mut buffer = [0; 4096];
    while !expected.is_empty() {
        let len = expected.len().min(buffer.len());
        reader.read_exact(&mut buffer[..len])?;
        check(
            buffer[..len] == expected[..len],
            "synthetic paste bytes differed",
        )?;
        expected = &expected[len..];
    }
    Ok(())
}

#[cfg(windows)]
fn raw_console() -> io::Result<()> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_INPUT_HANDLE, SetConsoleCP, SetConsoleMode,
    };
    // SAFETY: Borrow this isolated fixture's console handle; never close it.
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        if handle.is_null()
            || handle == INVALID_HANDLE_VALUE
            || SetConsoleMode(handle, 0) == 0
            || SetConsoleCP(65001) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn raw_console() -> io::Result<()> {
    let status = std::process::Command::new("stty")
        .args(["raw", "-echo"])
        .status()?;
    check(status.success(), "fixture could not select raw input")
}
