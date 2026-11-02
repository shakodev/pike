//! R32 finite actual-App/Iced/real-PTY shortcut reload acceptance.
//! All clipboard, configuration and captured bytes are synthetic fixture data.
#[allow(dead_code)]
#[path = "support/chrome_actions.rs"]
mod chrome_actions;
#[cfg(windows)]
#[allow(dead_code)]
#[path = "support/cursor_input.rs"]
mod fixture_input;
#[allow(dead_code)]
#[path = "command_routing/host.rs"]
mod host;
#[path = "support/host_viewport.rs"]
mod host_viewport;

use iced::{Executor, advanced::renderer::Headless};
use pike::{app::App, executor::SharedExecutor};
use std::{fs, io, io::Read, io::Write, path::Path};
#[path = "remappable_shortcuts/focus.rs"]
mod focus;
#[path = "remappable_shortcuts/scenarios.rs"]
mod scenarios;
#[allow(dead_code)]
#[path = "support/tab_management_widget.rs"]
mod widget;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "--child") {
        return child(Path::new(args.get(2).ok_or("missing fixture directory")?))
            .map_err(Into::into);
    }
    let output = std::path::absolute(args.get(1).ok_or("expected fresh output directory")?)?;
    fs::create_dir(&output)?;
    let path = output.join("settings.toml");
    fs::write(&path, configuration(&output)?)?;
    let executor = SharedExecutor::new()?;
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or("fixture requires a local graphics device")?;
    let (app, task) = executor.enter(|| App::with_config_path(path));
    let mut host = host::Host::new(app, task, renderer, executor);
    let result = scenarios::verify(&mut host, &output);
    if let Err(error) = &result {
        fs::write(output.join("failure.txt"), error.to_string())?;
    }
    let cleanup = host.cleanup();
    fs::write(output.join("events.txt"), host.evidence.join("\n"))?;
    if let Ok(receipts) = &cleanup {
        fs::write(output.join("cleanup.txt"), receipts.join("\n"))?;
    }
    cleanup?;
    result?;
    fs::write(output.join("completed.txt"), "passed\n")?;
    Ok(())
}

fn configuration(output: &Path) -> io::Result<String> {
    let executable = std::env::current_exe()?;
    // TOML serialization quotes paths, including Windows backslashes, exactly.
    Ok(format!(
        "version = 1\n[profiles.default]\nshell = {}\nargs = [\"--child\", {}]\ncwd = {}\n",
        toml::Value::String(executable.to_string_lossy().into_owned()),
        toml::Value::String(output.to_string_lossy().into_owned()),
        toml::Value::String(output.to_string_lossy().into_owned()),
    ))
}

fn child(output: &Path) -> io::Result<()> {
    #[cfg(unix)]
    if !std::process::Command::new("/bin/stty")
        .args(["raw", "-echo"])
        .status()?
        .success()
    {
        return Err(io::Error::other("controlled raw mode unavailable"));
    }
    #[cfg(windows)]
    fixture_input::configure(None)?;
    let pid = std::process::id();
    let mut file = fs::File::create(output.join(format!("{pid}.bin")))?;
    for line in 0..100 {
        println!("PIKE32_HISTORY_{line}\r");
    }
    println!("PIKE32_READY={pid}\r");
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
