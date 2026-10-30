//! Finite actual-App move/zoom fixture; only controlled child data is retained.
#[cfg(windows)]
#[allow(dead_code)]
#[path = "support/cursor_input.rs"]
mod fixture_input;
#[allow(dead_code)]
#[path = "command_routing/host.rs"]
mod host;

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
    let result = verify(&mut host, &output);
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

fn verify(host: &mut Host, _output: &Path) -> io::Result<()> {
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
    )
}
