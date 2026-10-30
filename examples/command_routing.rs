//! Synthetic full-host input and real child byte capture; no user terminal data.
#[cfg(windows)]
#[allow(dead_code)] // Reuse only the existing Windows raw-console setup.
#[path = "support/cursor_input.rs"]
mod fixture_input;
#[path = "command_routing/host.rs"]
mod host;
#[path = "support/host_viewport.rs"]
mod host_viewport;
#[path = "command_routing/scenarios.rs"]
mod scenarios;

use host::Host;
use iced::advanced::renderer::Headless;
use iced::keyboard::{self, Key, Location, Modifiers, key};
use iced::{Executor, mouse};
use pike::app::{App, Message};
use pike::executor::SharedExecutor;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "--child") {
        return child(Path::new(args.get(2).ok_or("missing child directory")?)).map_err(Into::into);
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
        .ok_or("fixture requires a local graphics device")?;
    let profile = iced_term::settings::BackendSettings {
        program: std::env::current_exe()?.to_string_lossy().into_owned(),
        args: vec!["--child".into(), output.to_string_lossy().into_owned()],
        working_directory: Some(output.clone()),
        ..Default::default()
    };
    let (app, task) = executor.enter(|| App::with_settings(profile));
    let mut host = Host::new(app, task, renderer, executor);
    let result = if args.get(2).is_some_and(|arg| arg == "--focus-race") {
        verify_focus_race(&mut host, &output)
    } else if args.get(2).is_some_and(|arg| arg == "--header-once") {
        verify_header_once(&mut host, &output)
    } else {
        verify(&mut host, &output)
    };
    let cleanup = host.cleanup();
    fs::write(output.join("commands.txt"), host.evidence.join("\n"))?;
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
        return Err(io::Error::other(
            "could not enter controlled raw input mode",
        ));
    }
    #[cfg(windows)]
    fixture_input::configure(None)?;
    let pid = std::process::id();
    let mut file = fs::File::create(output.join(format!("{pid}.bin")))?;
    for line in 0..100 {
        println!("PIKE26_HISTORY_{line}\r");
    }
    println!("PIKE26_READY={pid}\r");
    io::stdout().flush()?;
    let mut byte = [0];
    loop {
        match io::stdin().read(&mut byte) {
            Ok(0) => return Ok(()),
            Ok(_) => {
                file.write_all(&byte)?;
                file.flush()?;
                if byte[0] == 4 {
                    return Ok(());
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

fn press(key: Key, modifiers: Modifiers, text: Option<&str>, repeat: bool) -> iced::Event {
    iced::Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
        location: Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat,
    })
}

fn input_files(output: &Path) -> Vec<PathBuf> {
    fs::read_dir(output)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "bin"))
        .collect()
}

fn verify(host: &mut Host, output: &Path) -> io::Result<()> {
    host.update(Message::WindowFocused(true));
    host.until(|host| {
        host.app
            .active_session()
            .is_some_and(|id| host.app.terminal(id).is_some() && host.focused(id))
            && input_files(output).len() == 1
    })?;
    // The actual full view supplies the measured grid and terminal bounds.
    host.event(
        iced::Event::Window(iced::window::Event::Focused),
        mouse::Cursor::Unavailable,
    );
    host.redraw();
    let original = host.app.active_session().unwrap();
    let bounds = host
        .bounds(original)
        .ok_or_else(|| io::Error::other("missing actual widget bounds"))?;
    host.click(iced::Point::new(bounds.x + 40.0, bounds.y - 14.0));
    host.until(|host| host.focused(original))?;
    let original_file = input_files(output).pop().unwrap();
    host.event(
        press(
            Key::Character("c".into()),
            Modifiers::CTRL,
            Some("\u{3}"),
            false,
        ),
        mouse::Cursor::Unavailable,
    );
    host.until(|_| fs::read(&original_file).is_ok_and(|bytes| bytes == b"\x03"))?;
    let actions = host.controls;
    host.event(
        press(
            Key::Character("o".into()),
            Modifiers::CTRL | Modifiers::SHIFT,
            Some("\u{f}"),
            false,
        ),
        mouse::Cursor::Unavailable,
    );
    host.drain()?;
    check(
        host.controls == actions + 1,
        &format!(
            "Ctrl+Shift+O expected one application action, observed {}",
            host.controls - actions
        ),
    )?;
    host.until(|host| {
        host.app.model().sessions().len() == 2
            && host.app.active_session().is_some_and(|id| {
                id != original && host.app.terminal(id).is_some() && host.focused(id)
            })
    })?;
    scenarios::verify(host, output, original, original_file)
}

fn check(condition: bool, message: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}

fn verify_focus_race(host: &mut Host, output: &Path) -> io::Result<()> {
    use pike::app::{Control, Direction};
    host.update(Message::WindowFocused(true));
    host.until(|host| {
        host.app
            .active_session()
            .is_some_and(|id| host.app.terminal(id).is_some() && host.focused(id))
            && input_files(output).len() == 1
    })?;
    host.event(
        iced::Event::Window(iced::window::Event::Focused),
        mouse::Cursor::Unavailable,
    );
    host.redraw();
    let source = host.app.active_target().unwrap();
    let first = host.app.active_session().unwrap();
    host.update(Message::Control(Control::Split(source, Direction::Right)));
    host.until(|host| {
        host.app
            .active_session()
            .is_some_and(|id| id != first && host.app.terminal(id).is_some() && host.focused(id))
    })?;
    let second = host.app.active_session().unwrap();
    let second_target = host.app.model().target(second).unwrap();
    let delayed = host.app.update(Message::Control(Control::Focus(source)));
    host.update(Message::Control(Control::Focus(second_target)));
    host.until(|host| host.focused(second))?;
    host.finish(delayed)?;
    check(
        host.app.active_session() == Some(second),
        "delayed focus changed model selection",
    )?;
    check(
        host.focused(second) && !host.focused(first),
        "delayed task focused an older still-visible owner instead of the model-active session",
    )
}

fn verify_header_once(host: &mut Host, output: &Path) -> io::Result<()> {
    use pike::app::{Control, Direction};
    host.update(Message::WindowFocused(true));
    host.until(|host| {
        host.app
            .active_session()
            .is_some_and(|id| host.app.terminal(id).is_some() && host.focused(id))
            && input_files(output).len() == 1
    })?;
    host.event(
        iced::Event::Window(iced::window::Event::Focused),
        mouse::Cursor::Unavailable,
    );
    host.redraw();
    let source = host.app.active_target().unwrap();
    let first = host.app.active_session().unwrap();
    host.update(Message::Control(Control::Split(source, Direction::Right)));
    host.until(|host| {
        host.app
            .active_session()
            .is_some_and(|id| id != first && host.app.terminal(id).is_some() && host.focused(id))
            && input_files(output).len() == 2
    })?;
    let bounds = host.bounds(first).unwrap();
    let actions = host.controls;
    host.click(iced::Point::new(bounds.x + 40.0, bounds.y - 14.0));
    host.until(|host| host.app.active_session() == Some(first) && host.focused(first))?;
    check(
        host.controls == actions + 1,
        &format!(
            "pane header expected one focus action, observed {}",
            host.controls - actions
        ),
    )
}
