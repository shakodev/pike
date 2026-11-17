//! Full chrome acceptance scenes, run only after agreed actual-main integration.
use crate::{frame, host::Host, host_viewport};
use iced::{Executor, Size};
use pike::{
    app::{App, CloseAction, Control, Direction, Message},
    executor::SharedExecutor,
    session::Lifecycle,
};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn configuration(program: &str, child: &str) -> String {
    let shell = toml::Value::String(program.into());
    let child = toml::Value::String(child.into());
    format!(
        "version=1\ndefault_profile='fixture'\ntheme='system'\n[profiles.fixture]\nshell={shell}\nargs=[{child}]\n"
    )
}

pub fn ready(host: &mut Host) -> io::Result<()> {
    host.until(|host| {
        host.redraw();
        host.app
            .model()
            .workspace()
            .active()
            .and_then(|tab| host.app.tab_title(tab))
            .is_some_and(|title| title.as_str() == "PIKE33 controlled fixture")
            && prompt_ready(host)
            && host.app.model().sessions().iter().all(|(id, _)| {
                host.app
                    .terminal(id)
                    .is_some_and(|term| term.snapshot_stats().cells_copied > 0)
            })
    })?;
    host.drain()
}

/// Observe the controlled marker in each currently visible pane's snapshot.
/// A blank initial snapshot or a child title alone is insufficient evidence.
/// Only the fixed synthetic fixture marker is examined; nothing is logged.
pub fn prompt_ready(host: &Host) -> bool {
    let Some(tab) = host
        .app
        .model()
        .workspace()
        .active()
        .and_then(|id| host.app.model().workspace().tab(id))
    else {
        return true;
    };
    tab.grid()
        .iter()
        .filter(|(pane, _)| {
            tab.grid()
                .maximized()
                .is_none_or(|visible| **pane == visible)
        })
        .all(|(_, pane)| {
            host.app.terminal(pane.session).is_none_or(|terminal| {
                terminal.renderable_content().is_some_and(|content| {
                    content
                        .viewport
                        .display_iter()
                        .map(|indexed| indexed.cell.c)
                        .collect::<String>()
                        .contains("PIKE33")
                })
            })
        })
}

pub fn verify_readiness(host: &mut Host, output: &Path) -> io::Result<()> {
    host.update(Message::WindowFocused(true));
    host.until(|host| {
        host.redraw();
        host.app
            .active_session()
            .and_then(|id| host.app.terminal(id))
            .is_some_and(|terminal| terminal.snapshot_stats().cells_copied > 0)
    })?;
    let id = host.app.active_session().unwrap();
    let reservation = host.app.model().sessions().get(id).unwrap().reservation();
    let initial = host.app.terminal(id).unwrap().snapshot_stats().cells_copied;
    assert!(
        !prompt_ready(host),
        "initial blank snapshot was accepted as a ready prompt"
    );
    fs::write(
        output.join("readiness-initial.txt"),
        format!("initial_cells_copied={initial}\ninitial_blank_marker_accepted=false\n"),
    )?;
    let terminal_id = host.app.terminal(id).unwrap().id;
    host.update(Message::Terminal((
        id,
        iced_term::Event::BackendCall(terminal_id, iced_term::BackendCommand::Write(vec![b'\n'])),
    )));
    ready(host)?;
    assert!(prompt_ready(host));
    assert_eq!(
        host.app.model().sessions().get(id).unwrap().reservation(),
        reservation
    );
    fs::write(
        output.join("readiness.txt"),
        format!(
            "initial_cells_copied={initial}\ninitial_blank_marker_accepted=false\nobserved_current_prompt_marker=true\nstable_reservation={reservation:?}\n"
        ),
    )
}

pub fn verify(host: &mut Host, output: &Path) -> io::Result<()> {
    host.update(Message::WindowFocused(true));
    ready(host)?;
    frame::capture(host, output, "one", host_viewport::SIZE)?;
    frame::capture(host, output, "one-small", Size::new(320.0, 240.0))?;
    // Restore through the actual sensor before requesting cell-safe splits.
    frame::capture(host, output, "one-restored", host_viewport::SIZE)?;
    for (count, direction) in [
        (2, Direction::Right),
        (3, Direction::Down),
        (4, Direction::Right),
    ] {
        host.update(Message::Control(Control::Split(
            host.app.active_target().unwrap(),
            direction,
        )));
        host.until(|host| host.app.model().sessions().len() == count)?;
        ready(host)?;
    }
    frame::capture(host, output, "nested-four", host_viewport::SIZE)?;
    for direction in [
        Direction::Down,
        Direction::Right,
        Direction::Down,
        Direction::Right,
    ] {
        let targets: Vec<_> = host
            .app
            .model()
            .workspace()
            .active()
            .into_iter()
            .flat_map(|tab| {
                host.app
                    .model()
                    .workspace()
                    .tab(tab)
                    .unwrap()
                    .grid()
                    .iter()
                    .map(move |(pane, state)| {
                        (pike::layout::PaneTarget { tab, pane: *pane }, state.session)
                    })
            })
            .collect();
        let target = targets
            .into_iter()
            .max_by_key(|(_, id)| host.bounds(*id).map_or(0, |r| (r.width * r.height) as u64))
            .unwrap()
            .0;
        let count = host.app.model().sessions().len() + 1;
        host.update(Message::Control(Control::Split(target, direction)));
        host.until(|host| host.app.model().sessions().len() == count)?;
        ready(host)?;
    }
    frame::capture(host, output, "eight", host_viewport::SIZE)?;
    frame::capture(host, output, "eight-small", Size::new(320.0, 240.0))?;
    frame::capture(host, output, "eight-restored", host_viewport::SIZE)?;
    for _ in 0..8 {
        let count = host.app.model().sessions().len() + 1;
        host.update(Message::Control(Control::NewTab));
        host.until(|host| host.app.model().sessions().len() == count)?;
        ready(host)?;
    }
    frame::capture(host, output, "overflow-strip", host_viewport::SIZE)?;
    host.update(Message::Control(Control::TabAction(
        pike::tab_strip::TabAction::OpenOverflow,
    )));
    frame::capture(host, output, "overflow", host_viewport::SIZE)?;
    frame::capture(host, output, "overflow-small", Size::new(320.0, 240.0))?;
    let token = host
        .app
        .tab_dialog_token()
        .ok_or_else(|| io::Error::other("overflow did not open"))?;
    host.update(Message::Control(Control::TabDialog(
        token,
        pike::app::DialogAction::Cancel,
    )));
    host.drain()?;
    // A real rejected reload exercises the passive host error row with a live
    // controlled prompt still below it. No user configuration is accessed.
    fs::write(
        output.join("settings.toml"),
        "version=1\ninvalid-fixture-key=true\n",
    )?;
    host.update(Message::Control(Control::Command(
        pike::app::commands::Command::ReloadSettings,
    )));
    host.until(|host| !host.app.settings_pending() && host.app.settings_error().is_some())?;
    frame::capture(host, output, "settings-error", host_viewport::SIZE)?;
    Ok(())
}

fn make_host(config: PathBuf) -> io::Result<Host> {
    use iced::advanced::renderer::Headless;
    let executor = SharedExecutor::new()?;
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or_else(|| io::Error::other("native graphics required"))?;
    let (app, task) = executor.enter(|| App::with_config_path(config));
    Ok(Host::new(app, task, renderer, executor))
}

pub fn error_empty(output: &Path) -> io::Result<()> {
    let config = output.join("error-settings.toml");
    // Existing regular file passes settings path validation, while its missing
    // executable permission (Unix) / invalid image (Windows) fails real launch.
    // A missing configured path would instead reject settings and start defaults.
    let missing = std::path::absolute(output)?.join("pike33-controlled-invalid-executable");
    fs::write(&missing, b"PIKE33 controlled non-executable fixture\n")?;
    fs::write(
        &config,
        configuration(&missing.to_string_lossy(), "--child"),
    )?;
    let mut host = make_host(config)?;
    host.until(|host| {
        host.app
            .model()
            .sessions()
            .iter()
            .any(|(_, e)| matches!(e.lifecycle(), Lifecycle::Failed(_)))
    })?;
    frame::capture(&mut host, output, "error", host_viewport::SIZE)?;
    frame::capture(&mut host, output, "error-small", Size::new(320.0, 240.0))?;
    let id = host.app.active_session().unwrap();
    host.update(Message::Control(Control::CloseSession(id)));
    host.until(|host| host.app.model().sessions().is_empty())?;
    frame::capture(&mut host, output, "empty", host_viewport::SIZE)?;
    frame::capture(&mut host, output, "empty-small", Size::new(320.0, 240.0))?;
    let config = output.join("exited-settings.toml");
    fs::write(
        &config,
        configuration(&std::env::current_exe()?.to_string_lossy(), "--child-exit"),
    )?;
    let mut host = make_host(config)?;
    host.until(|host| {
        host.app
            .model()
            .sessions()
            .iter()
            .any(|(_, e)| matches!(e.lifecycle(), Lifecycle::Exited { .. }))
    })?;
    host.redraw();
    frame::capture(&mut host, output, "exited", host_viewport::SIZE)?;
    let id = host.app.active_session().unwrap();
    host.update(Message::Control(Control::CloseSession(id)));
    host.until(|host| host.app.model().sessions().is_empty())?;
    Ok(())
}

pub fn close(host: &mut Host) -> io::Result<()> {
    host.update(Message::CloseWindow);
    if let Some((token, _)) = host.app.close_confirmation() {
        host.update(Message::Control(Control::CloseAction(
            CloseAction::Confirm(token),
        )));
    }
    host.until(|host| host.app.model().sessions().is_empty())
}
