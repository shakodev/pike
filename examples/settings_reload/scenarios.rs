use super::{host::Host, widget};
use iced::{
    Event,
    advanced::{input_method::Event as Ime, widget::Id},
    keyboard::{Key, Modifiers, key::Named},
    mouse,
};
use iced_term::{ShutdownHandle, TermMode};
use pike::{
    app::{Control, Message},
    identity::{Reservation, SessionId},
    layout::PaneTarget,
    session::Lifecycle,
};
use std::{fs, io, path::Path};

pub fn configuration(output: &Path, changed: bool, remove_b: bool) -> io::Result<String> {
    let quote = |value: &str| toml::Value::String(value.to_owned()).to_string();
    let shell = quote(&std::env::current_exe()?.to_string_lossy());
    let output_arg = quote(&output.to_string_lossy());
    let (font, history, theme, policy, revision, argument) = if changed {
        (20, 7, "light", "never", "after", "new-args")
    } else {
        (14, 80, "dark", "confirm-running", "before", "old-args")
    };
    let mut source = format!(
        "version = 1\ndefault_profile = 'a'\nhistory = {history}\ntheme = '{theme}'\nclose_policy = '{policy}'\n[font]\nsize = {font}\n"
    );
    for name in if remove_b { vec!["a"] } else { vec!["a", "b"] } {
        let cwd = output.join(if changed {
            format!("{name}-next")
        } else {
            name.to_owned()
        });
        source.push_str(&format!("[profiles.{name}]\nshell = {shell}\nargs = ['--child', {output_arg}, '{argument}']\ncwd = {}\nenv = {{ PIKE31_PROFILE = '{name}', PIKE31_REVISION = '{revision}' }}\n", quote(&cwd.to_string_lossy())));
    }
    Ok(source)
}

fn check(value: bool, message: &'static str) -> io::Result<()> {
    if value {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}

fn key(host: &mut Host, key: Key, modifiers: Modifiers) -> io::Result<()> {
    host.event(
        Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)),
        mouse::Cursor::Unavailable,
    );
    host.event(
        widget::key(key, modifiers, None),
        mouse::Cursor::Unavailable,
    );
    host.event(
        Event::Keyboard(iced::keyboard::Event::ModifiersChanged(Modifiers::empty())),
        mouse::Cursor::Unavailable,
    );
    host.drain()
}
fn reload(host: &mut Host) -> io::Result<()> {
    key(
        host,
        Key::Named(Named::F5),
        Modifiers::CTRL | Modifiers::SHIFT,
    )?;
    host.until(|host| !host.app.settings_pending())
}
fn click(host: &mut Host, label: &str) -> io::Result<()> {
    let mut targets = widget::Targets::default();
    host.operate(&mut targets);
    let point = targets
        .labels
        .iter()
        .find(|(text, _)| text == label)
        .map(|(_, point)| *point)
        .ok_or_else(|| io::Error::other("visible label missing"))?;
    host.click(point);
    host.drain()
}

struct Child {
    session: SessionId,
    reservation: Reservation,
    target: PaneTarget,
    widget: Id,
    pid: u32,
    cleanup: ShutdownHandle,
    cell: iced::Size,
    mode: TermMode,
}
impl Child {
    fn capture(host: &Host, session: SessionId) -> Option<Self> {
        let terminal = host.app.terminal(session)?;
        let content = terminal.renderable_content()?;
        let text: String = content.viewport.cells().iter().map(|cell| cell.c).collect();
        let pid = text
            .split("PIKE31_PID=")
            .nth(1)?
            .split(|ch: char| !ch.is_ascii_digit())
            .next()?
            .parse()
            .ok()?;
        Some(Self {
            session,
            reservation: host.app.model().sessions().get(session)?.reservation(),
            target: host.app.model().target(session)?,
            widget: terminal.widget_id().clone(),
            pid,
            cleanup: terminal.shutdown_handle(),
            cell: iced::Size::new(
                content.terminal_size.cell_width,
                content.terminal_size.cell_height,
            ),
            mode: content.terminal_mode,
        })
    }
    fn intact(&self, host: &Host) -> bool {
        host.app
            .model()
            .sessions()
            .get(self.session)
            .is_some_and(|entry| {
                entry.reservation() == self.reservation && entry.lifecycle() == Lifecycle::Running
            })
            && host.app.model().target(self.session) == Some(self.target)
            && host.app.terminal(self.session).is_some_and(|terminal| {
                terminal.widget_id() == &self.widget
                    && terminal
                        .renderable_content()
                        .is_none_or(|c| c.terminal_mode == self.mode)
            })
            && self.cleanup.completion().is_none()
    }
    fn directory(&self, output: &Path) -> std::path::PathBuf {
        output.join(self.pid.to_string())
    }
    fn identity(&self, output: &Path, profile: &str, changed: bool) -> io::Result<()> {
        let text = fs::read_to_string(self.directory(output).join("identity.txt"))?;
        let cwd = output.join(if changed {
            format!("{profile}-next")
        } else {
            profile.to_owned()
        });
        check(
            text == format!(
                "pid={}\ncwd={}\nprofile={profile}\nrevision={}\nargument={}\n",
                self.pid,
                cwd.display(),
                if changed { "after" } else { "before" },
                if changed { "new-args" } else { "old-args" }
            ),
            "child cwd/env/args do not match its launch profile",
        )
    }
}

fn ready(host: &mut Host) -> io::Result<Child> {
    host.until(|host| {
        host.app
            .active_session()
            .is_some_and(|id| Child::capture(host, id).is_some())
    })?;
    let id = host.app.active_session().unwrap();
    host.redraw();
    host.redraw();
    host.drain()?;
    host.until(|host| {
        host.focused(id) && host.app.terminal(id).unwrap().work_stats().grid_resizes > 0
    })?;
    let child = Child::capture(host, id)
        .ok_or_else(|| io::Error::other("child marker lost after layout"))?;
    host.evidence.push(format!(
        "ready session={} request={} pid={} cell={:?} history={}",
        id.get(),
        child.reservation.request.get(),
        child.pid,
        child.cell,
        host.app
            .terminal(id)
            .unwrap()
            .snapshot_stats()
            .history_lines
    ));
    Ok(child)
}
fn select(host: &mut Host, child: &Child) -> io::Result<()> {
    host.update(Message::Control(Control::ActivateTab(child.target.tab)));
    host.until(|host| {
        host.app.active_session() == Some(child.session) && host.focused(child.session)
    })
}
fn choose_b(host: &mut Host) -> io::Result<()> {
    let mut targets = widget::Targets::default();
    host.operate(&mut targets);
    let picker = Id::from("settings-profile-picker");
    let point = targets
        .containers
        .iter()
        .find(|(id, _)| id.as_ref() == Some(&picker))
        .map(|(_, bounds)| bounds.center())
        .ok_or_else(|| io::Error::other("visible profile picker missing"))?;
    // Existing Iced PickList's command+wheel selects the last option (b), with
    // no manufactured Control or access to the App's private profile mapping.
    let modifiers = if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    };
    host.event(
        Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)),
        mouse::Cursor::Available(point),
    );
    host.event(
        Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
        }),
        mouse::Cursor::Available(point),
    );
    host.event(
        Event::Keyboard(iced::keyboard::Event::ModifiersChanged(Modifiers::empty())),
        mouse::Cursor::Unavailable,
    );
    host.drain()
}

pub fn verify(host: &mut Host, output: &Path) -> io::Result<Vec<String>> {
    let parent_env = std::env::var_os("PIKE31_PROFILE");
    host.update(Message::WindowFocused(true));
    host.event(
        Event::Window(iced::window::Event::Focused),
        mouse::Cursor::Unavailable,
    );
    let a = ready(host)?;
    a.identity(output, "a", false)?;
    choose_b(host)?;
    let b = ready(host)?;
    b.identity(output, "b", false)?;
    check(
        a.pid != b.pid && a.session != b.session,
        "profiles did not create independent owners",
    )?;
    check(
        host.app
            .terminal(a.session)
            .unwrap()
            .snapshot_stats()
            .history_lines
            == 80
            && host
                .app
                .terminal(b.session)
                .unwrap()
                .snapshot_stats()
                .history_lines
                == 80,
        "initial history cap not saturated",
    )?;
    let config = configuration(output, true, false)?;
    fs::write(output.join("settings.toml"), &config)?;
    // Both modal ownership and held terminal IME preedit precede the one common
    // resolver; an unaccepted reload cannot read the newly written file.
    key(host, Key::Named(Named::F1), Modifiers::empty())?;
    key(
        host,
        Key::Named(Named::F5),
        Modifiers::CTRL | Modifiers::SHIFT,
    )?;
    check(
        host.app.active_settings().font().size == 14.0 && !host.app.settings_pending(),
        "help leaked reload",
    )?;
    key(host, Key::Named(Named::Escape), Modifiers::empty())?;
    host.until(|host| host.focused(b.session))?;
    host.event(Event::InputMethod(Ime::Opened), mouse::Cursor::Unavailable);
    host.event(
        Event::InputMethod(Ime::Preedit("ni".into(), Some(0..2))),
        mouse::Cursor::Unavailable,
    );
    key(
        host,
        Key::Named(Named::F5),
        Modifiers::CTRL | Modifiers::SHIFT,
    )?;
    check(
        host.app.active_settings().font().size == 14.0 && !host.app.settings_pending(),
        "IME preedit leaked reload",
    )?;
    host.event(Event::InputMethod(Ime::Closed), mouse::Cursor::Unavailable);
    host.drain()?;
    let controls = host.controls;
    reload(host)?;
    check(
        host.controls == controls + 1,
        "reload did not use one common captured action",
    )?;
    check(
        host.app.settings_error().is_none() && host.app.active_settings().font().size == 20.0,
        "valid reload not committed",
    )?;
    check(
        a.intact(host) && b.intact(host),
        "reload changed live identity or modes",
    )?;
    for child in [&a, &b] {
        select(host, child)?;
        let terminal = host.app.terminal(child.session).unwrap();
        let content = terminal.renderable_content().unwrap();
        let cell = iced::Size::new(
            content.terminal_size.cell_width,
            content.terminal_size.cell_height,
        );
        check(
            cell != child.cell && terminal.work_stats().font_measurements == 2,
            "live font metrics were not applied exactly once",
        )?;
        check(
            terminal.snapshot_stats().history_limit.lines() == 7
                && terminal.snapshot_stats().history_lines <= 7,
            "live history was not truncated to configured cap",
        )?;
        fs::write(child.directory(output).join("palette.go"), "query")?;
        fs::write(child.directory(output).join("alive.go"), "verify")?;
        fs::write(child.directory(output).join("history.go"), "flood")?;
        let expected = b"\x1b]10;rgb:2020/2020/2020\x07\x1b]11;rgb:fafa/fafa/fafa\x07";
        host.until(|_| {
            fs::read(child.directory(output).join("input.bin"))
                .ok()
                .as_deref()
                == Some(expected.as_slice())
                && child.directory(output).join("alive.txt").exists()
                && child.directory(output).join("history.done").exists()
        })?;
        host.until(|host| {
            host.app
                .terminal(child.session)
                .unwrap()
                .snapshot_stats()
                .history_lines
                == 7
        })?;
        child.identity(
            output,
            if child.session == a.session { "a" } else { "b" },
            false,
        )?;
        check(
            fs::read(child.directory(output).join("alive.txt"))?
                == fs::read(child.directory(output).join("identity.txt"))?,
            "running child environment/cwd changed",
        )?;
        host.evidence.push(format!("live session={} pid={} unchanged-owner cell={cell:?} font-measurements=2 history=7/7 palette-OSC=light child-cwd-env-args=before", child.session.get(), child.pid));
    }
    // Broken-file reload uses the visible toolbar control, preserving bytes and
    // all live settings. No save task runs on either success or failure.
    let broken = "version = 1\nhistory = 10001 # synthetic-invalid";
    fs::write(output.join("settings.toml"), broken)?;
    click(host, "Reload settings (Ctrl+Shift+F5)")?;
    host.until(|host| !host.app.settings_pending())?;
    check(
        host.app.settings_error().is_some()
            && host.app.active_settings().history().lines() == 7
            && fs::read_to_string(output.join("settings.toml"))? == broken,
        "broken reload lost valid settings or overwrote file",
    )?;
    fs::write(output.join("settings.toml"), &config)?;
    reload(host)?;
    select(host, &b)?;
    click(host, "New tab")?;
    let new_b = ready(host)?;
    new_b.identity(output, "b", true)?;
    check(
        a.intact(host) && b.intact(host),
        "new profile version changed earlier running owners",
    )?;
    // Exited sessions restart through the existing retry owner, resolving the
    // retained profile ID against the new args/env/cwd configuration.
    fs::write(a.directory(output).join("exit.go"), "exit")?;
    host.until(|host| {
        host.app
            .model()
            .sessions()
            .get(a.session)
            .is_some_and(|entry| matches!(entry.lifecycle(), Lifecycle::Exited { .. }))
            && a.cleanup.completion().is_some()
    })?;
    select(host, &a)?;
    click(host, "Restart")?;
    let restarted_a = ready(host)?;
    restarted_a.identity(output, "a", true)?;
    check(
        restarted_a.pid != a.pid && restarted_a.session != a.session,
        "restart reused process/session identity",
    )?;
    // A deleted retained profile is recoverable, with no fallback or reservation.
    fs::write(
        output.join("settings.toml"),
        configuration(output, true, true)?,
    )?;
    reload(host)?;
    select(host, &b)?;
    let count = host.app.model().sessions().len();
    click(host, "New tab")?;
    check(
        host.app.settings_error() == Some(pike::settings::Error::MissingProfile)
            && host.app.model().sessions().len() == count
            && b.intact(host)
            && new_b.intact(host),
        "removed profile silently fell back or changed live owners",
    )?;
    check(
        std::env::var_os("PIKE31_PROFILE") == parent_env,
        "launch mutated parent environment",
    )?;
    host.evidence.push(format!("new-only cwd/env/args: inherited-b={} restarted-a={}; invalid-file retained; removed-profile refused; IME/help priority passed", new_b.pid, restarted_a.pid));
    let owners = [&b, &new_b, &restarted_a];
    host.update(Message::CloseWindow);
    check(
        host.app.close_confirmation().is_none(),
        "never close policy still asked",
    )?;
    host.until(|host| host.app.model().sessions().is_empty())?;
    owners
        .iter()
        .map(|child| {
            child
                .cleanup
                .completion()
                .map(|completion| {
                    format!(
                        "session={} pid={} {completion:?}",
                        child.session.get(),
                        child.pid
                    )
                })
                .ok_or_else(|| io::Error::other("missing native cleanup receipt"))
        })
        .collect()
}
