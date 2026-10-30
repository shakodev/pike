//! Finite actual-App/runtime/native-PTY fixture; all input and titles are synthetic.
#[path = "support/tab_management_child.rs"]
mod child;
#[allow(dead_code)]
#[path = "command_routing/host.rs"]
mod host;
#[path = "support/host_viewport.rs"]
mod host_viewport;
#[path = "tab_management/scenarios.rs"]
mod scenarios;
#[path = "support/tab_management_snapshot.rs"]
mod snapshot;
#[allow(dead_code)]
#[path = "support/tab_management_widget.rs"]
mod widget;

use iced::Executor;
use iced::advanced::renderer::Headless;
use pike::app::{App, CloseAction, Control, Message};
use pike::executor::SharedExecutor;
use std::{fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "--child") {
        return child::run(Path::new(args.get(2).ok_or("missing child output")?))
            .map_err(Into::into);
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
        .ok_or("fixture requires native graphics")?;
    let profile = iced_term::settings::BackendSettings {
        program: std::env::current_exe()?.to_string_lossy().into_owned(),
        args: vec!["--child".into(), output.to_string_lossy().into_owned()],
        working_directory: Some(output.clone()),
        ..Default::default()
    };
    let (app, task) = executor.enter(|| App::with_settings(profile));
    let mut host = host::Host::new(app, task, renderer, executor);
    let result = scenarios::verify(&mut host, &output);
    if let Err(error) = &result {
        fs::write(output.join("failure.txt"), error.to_string())?;
    }
    // Failure also goes through the App close owner, including pending launches.
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
            "App model empty after native cleanup acknowledgement\n"
        } else {
            "App cleanup deadline failed\n"
        },
    )?;
    fs::write(output.join("events.txt"), host.evidence.join("\n"))?;
    cleanup?;
    let receipts = result?;
    fs::write(output.join("cleanup.txt"), receipts.join("\n"))?;
    fs::write(output.join("completed.txt"), "passed\n")?;
    Ok(())
}
