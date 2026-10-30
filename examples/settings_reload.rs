//! Finite actual-App/settings/Iced/real-PTY acceptance; only synthetic data.
#[path = "settings_reload/child.rs"]
mod child;
#[allow(dead_code)]
#[path = "command_routing/host.rs"]
mod host;
#[path = "support/host_viewport.rs"]
mod host_viewport;
#[path = "settings_reload/scenarios.rs"]
mod scenarios;
#[allow(dead_code)]
#[path = "support/tab_management_widget.rs"]
mod widget;

use iced::{Executor, advanced::renderer::Headless};
use pike::{
    app::{App, CloseAction, Control, Message},
    executor::SharedExecutor,
};
use std::{fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "--child") {
        return child::run(
            Path::new(args.get(2).ok_or("missing synthetic output")?),
            args.get(3)
                .ok_or("missing synthetic argument")?
                .to_str()
                .ok_or("argument UTF-8")?,
        )
        .map_err(Into::into);
    }
    let output = std::path::absolute(args.get(1).ok_or("expected fresh output directory")?)?;
    fs::create_dir(&output)?;
    for name in ["a", "b", "a-next", "b-next"] {
        fs::create_dir(output.join(name))?;
    }
    let path = output.join("settings.toml");
    fs::write(&path, scenarios::configuration(&output, false, false)?)?;
    let executor = SharedExecutor::new()?;
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or("native graphics required")?;
    let (app, task) = executor.enter(|| App::with_config_path(path));
    let mut host = host::Host::new(app, task, renderer, executor);
    let result = scenarios::verify(&mut host, &output);
    if let Err(error) = &result {
        fs::write(output.join("failure.txt"), error.to_string())?;
    }
    host.update(Message::CloseWindow);
    if let Some((token, _)) = host.app.close_confirmation() {
        host.update(Message::Control(Control::CloseAction(
            CloseAction::Confirm(token),
        )));
    }
    let cleanup = host.until(|host| host.app.model().sessions().is_empty());
    fs::write(
        output.join("fallback-cleanup.txt"),
        if cleanup.is_ok() {
            "App empty after native cleanup acknowledgement\n"
        } else {
            "App cleanup deadline failed\n"
        },
    )?;
    fs::write(output.join("events.txt"), host.evidence.join("\n"))?;
    cleanup?;
    fs::write(output.join("cleanup.txt"), result?.join("\n"))?;
    fs::write(output.join("completed.txt"), "passed\n")?;
    Ok(())
}
