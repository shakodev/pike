//! Finite actual-App move/zoom fixture; only controlled child data is retained.
#[cfg(windows)]
#[allow(dead_code)]
#[path = "support/cursor_input.rs"]
mod fixture_input;
#[allow(dead_code)]
#[path = "command_routing/host.rs"]
mod host;
#[path = "support/host_viewport.rs"]
mod host_viewport;
#[path = "pane_moving/scenarios.rs"]
mod scenarios;

use host::Host;
use iced::advanced::renderer::Headless;
use iced::{Executor, Point, mouse};
use pike::app::{App, Control, Direction, Message};
use pike::executor::SharedExecutor;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|a| a == "--child") {
        return child(Path::new(args.get(2).ok_or("missing output")?)).map_err(Into::into);
    }
    let output = std::path::absolute(args.get(1).ok_or("expected fresh output directory")?)?;
    fs::create_dir(&output)?;
    let executor = SharedExecutor::new()?;
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or("fixture needs native graphics")?;
    let profile = iced_term::settings::BackendSettings {
        program: std::env::current_exe()?.to_string_lossy().into_owned(),
        args: vec!["--child".into(), output.to_string_lossy().into_owned()],
        working_directory: Some(output.clone()),
        ..Default::default()
    };
    let (app, task) = executor.enter(|| App::with_settings(profile));
    let mut host = Host::new(app, task, renderer, executor);
    let result = verify(
        &mut host,
        &output,
        args.get(2).and_then(|arg| arg.to_str()).unwrap_or(""),
    );
    let cleanup = host.cleanup();
    fs::write(output.join("events.txt"), host.evidence.join("\n"))?;
    if let Ok(receipts) = &cleanup {
        fs::write(output.join("cleanup.txt"), receipts.join("\n"))?;
    }
    if let Err(error) = &result {
        fs::write(output.join("failure.txt"), error.to_string())?;
    }
    cleanup?;
    result?;
    fs::write(output.join("completed.txt"), "passed\n")?;
    Ok(())
}

fn child(output: &Path) -> io::Result<()> {
    #[cfg(unix)]
    if !std::process::Command::new("/bin/stty")
        .args(["raw", "-echo"])
        .status()?
        .success()
    {
        return Err(io::Error::other("raw mode failed"));
    }
    #[cfg(windows)]
    fixture_input::configure(None)?;
    let pid = std::process::id();
    let mut file = fs::File::create(output.join(format!("{pid}.bin")))?;
    for line in 0..100 {
        println!("PIKE27_HISTORY_{line}\r");
    }
    println!("\x1b[?2004hPIKE27_READY={pid}\r");
    io::stdout().flush()?;
    let mut byte = [0];
    loop {
        match io::stdin().read(&mut byte) {
            Ok(0) => return Ok(()),
            Ok(_) => {
                file.write_all(&byte)?;
                file.flush()?;
                match byte[0] {
                    b't' => {
                        let output = output.to_owned();
                        std::thread::spawn(move || {
                            for tick in 1..=100 {
                                println!("PIKE27_TICK={tick}\r");
                                let _ = io::stdout().flush();
                                let _ = fs::write(
                                    output.join(format!("{pid}.ticks")),
                                    tick.to_string(),
                                );
                                std::thread::sleep(std::time::Duration::from_millis(25));
                            }
                        });
                    }
                    b'm' => {
                        print!("\x1b[?1000h\x1b[?1006h");
                        io::stdout().flush()?;
                    }
                    b'g' => {
                        print!("\x1b[48;2;0;255;0m\x1b[2J\x1b[HPIKE27_GREEN\x1b[0m");
                        io::stdout().flush()?;
                    }
                    b'n' => {
                        print!("\x1b[?1000l\x1b[?1006l");
                        io::stdout().flush()?;
                    }
                    b'f' => {
                        let deadline =
                            std::time::Instant::now() + std::time::Duration::from_secs(10);
                        while !output.join(format!("{pid}.go")).exists() {
                            if std::time::Instant::now() >= deadline {
                                return Err(io::Error::other("flood gate timeout"));
                            }
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        for line in 0..3000 {
                            println!("PIKE27_FLOOD_{line}\r");
                        }
                        print!("\x1b[6n");
                        io::stdout().flush()?;
                        let mut reply = Vec::new();
                        while reply.len() < 64 {
                            io::stdin().read_exact(&mut byte)?;
                            reply.push(byte[0]);
                            if byte[0] == b'R' {
                                break;
                            }
                        }
                        fs::write(output.join(format!("{pid}.query")), &reply)?;
                        println!("PIKE27_FINAL_SENTINEL={pid}\r");
                        io::stdout().flush()?;
                        std::process::exit(37);
                    }
                    _ => {}
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

fn check(ok: bool, message: &str) -> io::Result<()> {
    if ok {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}

fn pointer(host: &mut Host, event: mouse::Event, point: Point) {
    host.event(iced::Event::Mouse(event), mouse::Cursor::Available(point));
}

fn drag(host: &mut Host, from: Point, to: Point) -> io::Result<()> {
    pointer(host, mouse::Event::CursorMoved { position: from }, from);
    pointer(host, mouse::Event::ButtonPressed(mouse::Button::Left), from);
    host.drain()?;
    pointer(host, mouse::Event::CursorMoved { position: to }, to);
    pointer(host, mouse::Event::ButtonReleased(mouse::Button::Left), to);
    host.drain()
}

fn verify(host: &mut Host, output: &Path, mode: &str) -> io::Result<()> {
    host.update(Message::WindowFocused(true));
    host.until(|h| {
        h.app
            .active_session()
            .is_some_and(|id| h.app.terminal(id).is_some() && h.focused(id))
    })?;
    host.event(
        iced::Event::Window(iced::window::Event::Focused),
        mouse::Cursor::Unavailable,
    );
    host.redraw();
    let first = host.app.active_session().unwrap();
    let source = host.app.active_target().unwrap();
    host.update(Message::Control(Control::Split(source, Direction::Right)));
    host.until(|h| {
        h.app
            .active_session()
            .is_some_and(|id| id != first && h.app.terminal(id).is_some() && h.focused(id))
    })?;
    host.redraw();
    let second = host.app.active_session().unwrap();
    let before = host.bounds(first).unwrap();
    let destination = host.bounds(second).unwrap();
    let from = Point::new(before.x + before.width / 2.0, before.y - 14.0);
    drag(host, from, destination.center())?;
    let after = host.bounds(first).unwrap();
    host.evidence.push(format!(
        "center-drop source={} before={before:?} after={after:?}",
        first.get()
    ));
    check(
        after.x > before.x,
        "actual header center drop did not swap pane positions",
    )?;
    check(
        host.app.active_session() == Some(first) && host.focused(first),
        "drop lost source focus",
    )?;
    if mode == "--ime-pointer" {
        scenarios::ime_pointer(host, output, first, second)
    } else if mode == "--outside-release" {
        scenarios::outside_release(host, output, first, second)
    } else if mode == "--joint-tabs" {
        scenarios::joint_dialogs(host, output, first, second)
    } else {
        scenarios::verify(host, output, first, second)
    }
}
