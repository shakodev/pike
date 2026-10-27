//! Opt-in protocol contract probe. All child output and captured bytes are synthetic.
use iced::advanced::subscription::into_recipes;
use iced::futures::{StreamExt, stream};
use iced::{Executor, Size};
use iced_term::actions::Action;
use iced_term::{AlacrittyEvent, BackendCommand, Command, Event, PreparedTerminal, Terminal};
use pike::executor::SharedExecutor;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--fixture") {
        let id: u8 = args.get(2).ok_or("missing fixture id")?.parse()?;
        let result = fixture(
            id,
            Path::new(args.get(3).ok_or("missing fixture directory")?),
        );
        match result {
            Ok(()) => std::process::exit(i32::from(40 + id)),
            Err(error) => {
                eprintln!("Synthetic protocol fixture: {error}");
                std::process::exit(80);
            }
        }
    }
    let output = Path::new(args.get(1).ok_or("expected fresh output directory")?);
    if output.exists() {
        return Err("choose a fresh output directory".into());
    }
    std::fs::create_dir_all(output)?;
    let executor = SharedExecutor::new()?;
    let program = std::env::current_exe()?
        .to_str()
        .ok_or("non-Unicode fixture path")?
        .to_owned();
    let mut terminals = Vec::new();
    let window_focus = iced_term::WindowFocus::default();
    window_focus.set_focused(true); // Controlled headless window owner.
    let result = (|| -> io::Result<()> {
        for id in 1..=2 {
            let prepared = PreparedTerminal::new(
                20000 + u64::from(id),
                iced_term::settings::BackendSettings {
                    program: program.clone(),
                    args: vec![
                        "--fixture".into(),
                        id.to_string(),
                        output.to_str().unwrap().into(),
                    ],
                    ..Default::default()
                },
            )?;
            let mut terminal = prepared.attach_hidden(Default::default(), Default::default());
            terminal.set_window_focus(window_focus.clone());
            terminal.handle(Command::ChangeTheme(Box::new(iced_term::ColorPalette {
                foreground: foreground(id).into(),
                background: "#654321".into(),
                red: "#112233".into(),
                ..Default::default()
            })));
            terminal.handle(Command::ProxyToBackend(BackendCommand::Resize(
                Some(if id == 1 {
                    Size::new(720.0, 480.0)
                } else {
                    Size::new(680.0, 320.0)
                }),
                Some(Size::new(if id == 1 { 8.0 } else { 8.5 }, 16.0)),
            )));
            terminal.set_visible(id == 1);
            terminals.push(terminal);
        }
        executor.block_on(async {
            tokio::time::timeout(Duration::from_secs(30), run(&mut terminals, output))
                .await
                .map_err(io::Error::other)?
        })
    })();
    let handles: Vec<_> = terminals.iter().map(Terminal::shutdown_handle).collect();
    drop(terminals);
    let mut cleanup = std::fs::File::create(output.join("cleanup.csv"))?;
    writeln!(cleanup, "session,reason,exit_code,output_drained")?;
    for (index, handle) in handles.into_iter().enumerate() {
        let completion = executor.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), handle.wait()).await
        })?;
        writeln!(
            cleanup,
            "{},{:?},{:?},{}",
            20001 + index,
            completion.reason,
            completion.exit_code,
            completion.output_drained
        )?;
    }
    result?;
    std::fs::write(output.join("completed.txt"), "passed\n")?;
    Ok(())
}

fn foreground(id: u8) -> &'static str {
    if id == 1 { "#123456" } else { "#fedcba" }
}

fn query_bytes() -> &'static [u8] {
    b"\x1b[18t\x1b[14t\x1b]10;?\x07\x1b]11;?\x07\x1b]12;?\x07\x1b]4;1;?\x07\x1b]4;196;?\x07\x1b]10;#abcdef\x07\x1b]10;?\x07\x1b]110\x07\x1b]10;?\x07"
}

fn expected_replies(id: u8) -> Vec<u8> {
    let fg = if id == 1 {
        "1212/3434/5656"
    } else {
        "fefe/dcdc/baba"
    };
    let geometry = if id == 1 {
        "\x1b[8;30;90t\x1b[4;480;720t"
    } else {
        "\x1b[8;20;80t\x1b[4;320;680t"
    };
    format!("{geometry}\x1b]10;rgb:{fg}\x07\x1b]11;rgb:6565/4343/2121\x07\x1b]12;rgb:{fg}\x07\x1b]4;1;rgb:1111/2222/3333\x07\x1b]4;196;rgb:ffff/0000/0000\x07\x1b]10;rgb:abab/cdcd/efef\x07\x1b]10;rgb:{fg}\x07").into_bytes()
}

fn expect(input: &mut impl Read, expected: &[u8]) -> io::Result<()> {
    let mut bytes = vec![0; expected.len()];
    input.read_exact(&mut bytes)?;
    check(bytes == expected, "synthetic ordered reply bytes differ")
}

fn title(output: &mut impl Write, value: &str) -> io::Result<()> {
    write!(output, "\x1b]2;{value}\x07")?;
    output.flush()
}

fn fixture(id: u8, directory: &Path) -> io::Result<()> {
    raw_console()?;
    let mut output = io::stdout().lock();
    let mut input = io::stdin().lock();
    std::fs::write(
        directory.join(format!("fixture-{id}.pid")),
        std::process::id().to_string(),
    )?;
    title(&mut output, &format!("pike:protocol:ready:{id}"))?;
    expect(&mut input, b"S")?;
    title(&mut output, &format!("synthetic-\u{202e}title-{id}"))?;
    expect(&mut input, b"T")?;
    output.write_all(b"\x1bc")?;
    output.flush()?;
    expect(&mut input, b"R")?;
    output.write_all(b"\x07")?;
    output.flush()?;
    expect(&mut input, b"B")?;
    output.write_all(query_bytes())?;
    output.flush()?;
    let expected = expected_replies(id);
    expect(&mut input, &expected)?;
    std::fs::write(directory.join(format!("replies-{id}.bin")), &expected)?;
    for index in 0..10_000 {
        write!(output, "\x1b]2;synthetic-flood-{id}-{index}\x07\x07")?;
        if index % 1000 == 0 {
            output.write_all(b"\x1b]10;?\x07")?;
        }
    }
    output.flush()?;
    let fg = if id == 1 {
        "1212/3434/5656"
    } else {
        "fefe/dcdc/baba"
    };
    expect(
        &mut input,
        format!("\x1b]10;rgb:{fg}\x07").repeat(10).as_bytes(),
    )?;
    output.write_all(b"\x1b[?1004h")?;
    title(&mut output, &format!("pike:protocol:focus:{id}"))?;
    if id == 2 {
        expect(&mut input, b"H")?;
    }
    expect(&mut input, b"\x1b[I\x1b[O")?;
    output.write_all(b"\x1b[?1004l")?;
    title(&mut output, &format!("pike:protocol:disabled:{id}"))?;
    expect(&mut input, b"D")?;
    output.write_all(b"\x1b[?1004h")?;
    title(&mut output, &format!("pike:protocol:hidden:{id}"))?;
    expect(&mut input, b"\x1b[O")?;
    title(&mut output, &format!("pike:protocol:done:{id}"))?;
    Ok(())
}

fn send(terminal: &mut Terminal, bytes: &[u8]) -> io::Result<()> {
    let action = terminal.handle(Command::ProxyToBackend(BackendCommand::Write(
        bytes.to_vec(),
    )));
    check(
        !matches!(action, Action::InputRejected(_)),
        "fixture input rejected",
    )
}

async fn run(terminals: &mut [Terminal], output: &Path) -> io::Result<()> {
    let streams: Vec<_> = terminals
        .iter()
        .map(|terminal| {
            into_recipes(terminal.subscription())
                .pop()
                .unwrap()
                .stream(stream::pending().boxed())
        })
        .collect();
    let mut events = stream::select_all(streams);
    let mut phases = [0_u8; 2];
    // Count only the fixture's query phase, excluding console negotiation.
    // Console startup negotiation is not one of the fixture's requests.
    let mut queries = [[0_u32; 3]; 2];
    let mut completed = [false; 2];
    let mut record = std::fs::File::create(output.join("events.csv"))?;
    writeln!(record, "event,session,value")?;
    while let Some(Event::BackendCall(id, command)) = events.next().await {
        let index = usize::try_from(
            id.checked_sub(20001)
                .ok_or_else(|| io::Error::other("wrong origin"))?,
        )
        .map_err(io::Error::other)?;
        check(index < 2, "unknown originating session")?;
        let fixture_id = index + 1;
        if phases[index] == 4 {
            match &command {
                BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::ColorRequest(..)) => {
                    queries[index][0] += 1
                }
                BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::TextAreaSizeRequest(..)) => {
                    queries[index][1] += 1
                }
                BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::PtyWrite(reply)) => {
                    check(
                        reply.starts_with("\x1b[8;"),
                        "unexpected forwarded protocol reply",
                    )?;
                    queries[index][2] += 1;
                }
                _ => {}
            }
        }
        let terminal = &mut terminals[index];
        let action = terminal.handle(Command::ProxyToBackend(command));
        match action {
            Action::ChangeTitle(value) if value == format!("pike:protocol:ready:{fixture_id}") => {
                check(phases[index] == 0, "duplicate readiness")?;
                phases[index] = 1;
                send(terminal, b"S")?;
            }
            Action::ChangeTitle(value) if value == format!("synthetic-title-{fixture_id}") => {
                check(phases[index] == 1, "wrong title origin or phase")?;
                phases[index] = 2;
                send(terminal, b"T")?;
                writeln!(record, "sanitized_title,{id},passed")?;
            }
            Action::ResetTitle if phases[index] == 2 => {
                phases[index] = 3;
                send(terminal, b"R")?;
                writeln!(record, "reset_title,{id},passed")?;
            }
            Action::Bell if phases[index] == 3 => {
                phases[index] = 4;
                send(terminal, b"B")?;
                writeln!(record, "bell,{id},passed")?;
            }
            Action::ChangeTitle(value) if value == format!("pike:protocol:focus:{fixture_id}") => {
                check(phases[index] == 4, "query fixture phase mismatch")?;
                let [colors, pixel_size, character_size] = queries[index];
                writeln!(record, "adapter_color_queries,{id},{colors}")?;
                writeln!(record, "adapter_pixel_size_queries,{id},{pixel_size}")?;
                writeln!(
                    record,
                    "adapter_character_size_queries,{id},{character_size}"
                )?;
                record.flush()?;
                check(colors == 17, "palette queries bypassed the adapter")?;
                check(
                    pixel_size == 1 && character_size == 1,
                    "fixture size queries bypassed the adapter",
                )?;
                phases[index] = 5;
                if index == 1 {
                    terminal.set_focused(true);
                    send(terminal, b"H")?;
                    terminal.set_visible(true);
                }
                for focused in [true, true, false, false] {
                    check(
                        !matches!(terminal.set_focused(focused), Action::InputRejected(_)),
                        "focus reply rejected",
                    )?;
                }
                writeln!(record, "ordered_queries,{id},19")?;
            }
            Action::ChangeTitle(value)
                if value == format!("pike:protocol:disabled:{fixture_id}") =>
            {
                check(phases[index] == 5, "focus fixture phase mismatch")?;
                phases[index] = 6;
                terminal.set_focused(true);
                terminal.set_focused(true);
                send(terminal, b"D")?;
            }
            Action::ChangeTitle(value) if value == format!("pike:protocol:hidden:{fixture_id}") => {
                check(phases[index] == 6, "disabled focus fixture phase mismatch")?;
                phases[index] = 7;
                terminal.set_focused(true);
                terminal.set_visible(false);
                terminal.set_focused(true);
            }
            Action::ChangeTitle(value) if value == format!("pike:protocol:done:{fixture_id}") => {
                check(phases[index] == 7, "hidden focus fixture phase mismatch")?;
                phases[index] = 8;
                writeln!(record, "focus_reports,{id},passed")?;
            }
            Action::ChildExit(code)
            | Action::Shutdown {
                exit_code: Some(code),
            } => {
                check(
                    code == 40 + fixture_id as i32,
                    "wrong child exit status or origin",
                )?;
                writeln!(record, "exit_status,{id},{code}")?;
            }
            Action::BackendCompleted(completion) => {
                check(
                    phases[index] == 8
                        && completion.exit_code == Some(40 + fixture_id as i32)
                        && completion.reason == iced_term::ExitReason::ChildExit
                        && completion.output_drained,
                    "missing natural completion or final metadata",
                )?;
                completed[index] = true;
                writeln!(record, "completed,{id},passed")?;
                if completed.iter().all(|done| *done) {
                    return Ok(());
                }
            }
            Action::InputRejected(_) | Action::EventBridgeError(_) => {
                return Err(io::Error::other("fixture protocol delivery failed"));
            }
            _ => {}
        }
        record.flush()?;
    }
    Err(io::Error::other("fixture streams ended before completion"))
}

fn check(condition: bool, message: &'static str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}

#[cfg(windows)]
fn raw_console() -> io::Result<()> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_INPUT, GetStdHandle, STD_INPUT_HANDLE, SetConsoleCP, SetConsoleMode,
    };
    // SAFETY: Borrow the fixture's standard input handle. Disabling line/echo
    // processing and enabling VT input affects only this synthetic console.
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        if handle.is_null()
            || handle == INVALID_HANDLE_VALUE
            || SetConsoleMode(handle, ENABLE_VIRTUAL_TERMINAL_INPUT) == 0
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
