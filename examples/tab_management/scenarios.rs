use super::{host::Host, snapshot::Snapshot, widget};
use iced::advanced::input_method::Event as Ime;
use iced::advanced::widget::{Id, Operation, operation};
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::{Event, mouse};
use pike::app::{Control, DialogAction, Message};
use pike::identity::TabId;
use pike::session::Lifecycle;
use pike::tab_strip::TabAction;
use pike::tabs::Title;
use std::{collections::BTreeSet, fs, io, path::Path};

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
fn chord(host: &mut Host, letter: &str) -> io::Result<()> {
    key(
        host,
        Key::Character(letter.into()),
        Modifiers::CTRL | Modifiers::SHIFT,
    )
}
fn named(host: &mut Host, key_name: Named) -> io::Result<()> {
    key(host, Key::Named(key_name), Modifiers::empty())
}
fn ime(host: &mut Host, event: Ime) -> io::Result<()> {
    host.event(Event::InputMethod(event), mouse::Cursor::Unavailable);
    host.drain()
}
fn targets(host: &mut Host) -> widget::Targets {
    let mut targets = widget::Targets::default();
    host.operate(&mut targets);
    targets
}
fn click(host: &mut Host, label: impl Fn(&str) -> bool) -> io::Result<()> {
    for _ in 0..8 {
        let targets = targets(host);
        if let Some((_, point)) = targets.labels.iter().find(|(text, _)| label(text)) {
            host.click(*point);
            return host.drain();
        }
        if host.app.tab_dialog_selected().is_none() {
            break;
        }
        for point in targets.scrolls {
            host.event(
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: 0.0, y: -6.0 },
                }),
                mouse::Cursor::Available(point),
            );
        }
        host.drain()?;
    }
    Err(io::Error::other("actual visible pointer target missing"))
}
fn focused(host: &mut Host, id: Id) -> bool {
    let mut query = operation::focusable::is_focused(id);
    host.operate(&mut operation::black_box(&mut query));
    matches!(query.finish(), operation::Outcome::Some(true))
}
fn ready(host: &mut Host) -> io::Result<Snapshot> {
    let id = host
        .app
        .active_session()
        .ok_or_else(|| io::Error::other("no active child"))?;
    host.redraw();
    host.until(|host| Snapshot::capture(&host.app, id).is_some() && host.focused(id))?;
    // Attachment focus can build the tree before its first native paint. Let
    // sensor/resize receipts settle before comparing history across tab visits.
    host.redraw();
    host.redraw();
    host.drain()?;
    host.until(|host| {
        host.app
            .terminal(id)
            .is_some_and(|terminal| terminal.work_stats().grid_resizes > 0)
    })?;
    let snapshot = Snapshot::capture(&host.app, id).unwrap();
    host.evidence.push(format!(
        "ready tab={} session={} pid={}",
        snapshot.tab().get(),
        id.get(),
        snapshot.pid
    ));
    Ok(snapshot)
}
fn select(host: &mut Host, tab: TabId) -> io::Result<()> {
    host.update(Message::Control(Control::ActivateTab(tab)));
    host.until(|host| {
        host.app.model().workspace().active() == Some(tab)
            && host.app.active_session().is_some_and(|id| host.focused(id))
    })
}
fn open_rename(host: &mut Host) -> io::Result<(u64, Id)> {
    let controls = host.controls;
    chord(host, "r")?;
    check(
        host.controls == controls + 1,
        "rename shortcut must emit one action",
    )?;
    host.until(|host| {
        host.app
            .tab_name_widget_id()
            .is_some_and(|id| focused(host, id))
    })?;
    host.redraw();
    host.redraw();
    Ok((
        host.app.tab_dialog_token().unwrap(),
        host.app.tab_name_widget_id().unwrap(),
    ))
}
fn set_name(host: &mut Host, name: &str) -> io::Result<()> {
    let (_, input) = open_rename(host)?;
    host.operate(&mut operation::text_input::select_all(input));
    ime(host, Ime::Opened)?;
    ime(host, Ime::Commit(name.into()))?;
    ime(host, Ime::Closed)?;
    named(host, Named::Enter)?;
    host.until(|host| host.app.tab_dialog_token().is_none())
}
fn close_overflow_row(host: &mut Host, tab: TabId) -> io::Result<()> {
    chord(host, "l")?;
    named(host, Named::Home)?;
    let id = Id::from(format!("tab-overflow-row-{}", tab.get()));
    let targets = targets(host);
    let row = targets
        .containers
        .iter()
        .find(|(candidate, _)| candidate.as_ref() == Some(&id))
        .map(|(_, bounds)| *bounds)
        .ok_or_else(|| io::Error::other("close row is not visible"))?;
    let point = targets
        .labels
        .iter()
        .find(|(label, point)| label == "Close tab" && row.contains(*point))
        .map(|(_, point)| *point)
        .ok_or_else(|| io::Error::other("close row button missing"))?;
    host.click(point);
    host.drain()
}

pub fn verify(host: &mut Host, output: &Path) -> io::Result<Vec<String>> {
    host.update(Message::WindowFocused(true));
    host.event(
        Event::Window(iced::window::Event::Focused),
        mouse::Cursor::Unavailable,
    );
    let mut saved = vec![ready(host)?];
    for index in 1..12 {
        if index % 3 == 0 {
            chord(host, "t")?;
        } else {
            click(host, |label| label == "New tab")?;
        }
        saved.push(ready(host)?);
    }
    check(
        host.app.model().workspace().order().len() == 12,
        "expected twelve actual tabs",
    )?;
    check(
        saved.iter().map(|s| s.pid).collect::<BTreeSet<_>>().len() == 12,
        "children must have distinct PIDs",
    )?;
    let tabs: Vec<_> = saved.iter().map(Snapshot::tab).collect();
    select(host, tabs[0])?;
    named(host, Named::F2)?;
    host.until(|_| {
        fs::read(output.join(saved[0].pid.to_string()).join("input.bin"))
            .ok()
            .as_deref()
            == Some(b"\x1bOQ")
    })?;
    host.evidence
        .push("bare-F2 remains terminal-owned: exact 1b4f51".into());

    let (old_token, input) = open_rename(host)?;
    host.operate(&mut operation::text_input::select_all(input));
    ime(host, Ime::Commit("UNOPENED-MUST-NOT-APPEAR".into()))?;
    named(host, Named::Enter)?;
    let initial = format!("Fixture {}", saved[0].pid);
    check(
        host.app.tab_title(tabs[0]).unwrap().as_str() == initial,
        "unopened commit changed draft",
    )?;
    let (token, input) = open_rename(host)?;
    host.operate(&mut operation::text_input::select_all(input));
    ime(host, Ime::Opened)?;
    ime(host, Ime::Preedit("ni".into(), Some(0..2)))?;
    named(host, Named::Enter)?;
    named(host, Named::Escape)?;
    chord(host, "t")?;
    check(
        host.app.tab_dialog_token() == Some(token)
            && host.app.model().workspace().order().len() == 12,
        "composition escaped rename ownership",
    )?;
    ime(host, Ime::Commit("Manual 日本語 🦀 <b>&plain</b> ".into()))?;
    ime(host, Ime::Closed)?;
    named(host, Named::Enter)?;
    check(
        host.app.tab_title(tabs[0]).unwrap().as_str() == "Manual 日本語 🦀 <b>&plain</b>",
        "IME name was not committed exactly once",
    )?;

    let (retry_token, input) = open_rename(host)?;
    host.operate(&mut operation::text_input::select_all(input));
    ime(host, Ime::Opened)?;
    ime(host, Ime::Preedit("old".into(), Some(0..3)))?;
    host.event(
        Event::Window(iced::window::Event::Unfocused),
        mouse::Cursor::Unavailable,
    );
    host.update(Message::WindowFocused(false));
    ime(host, Ime::Commit("BLURRED-MUST-NOT-APPEAR".into()))?;
    host.event(
        Event::Window(iced::window::Event::Focused),
        mouse::Cursor::Unavailable,
    );
    host.update(Message::WindowFocused(true));
    ime(host, Ime::Commit("STALE-MUST-NOT-APPEAR".into()))?;
    host.update(Message::TabName(
        old_token,
        "OLD-TOKEN-MUST-NOT-APPEAR".into(),
    ));
    host.update(Message::Control(Control::TabDialog(
        token,
        DialogAction::Cancel,
    )));
    check(
        host.app
            .tab_dialog_token()
            .is_some_and(|current| current != retry_token),
        "blur did not retire editor token",
    )?;
    host.update(Message::Control(Control::TabDialog(
        retry_token,
        DialogAction::Save,
    )));
    let input = host.app.tab_name_widget_id().unwrap();
    host.operate(&mut operation::text_input::select_all(input));
    ime(host, Ime::Opened)?;
    ime(host, Ime::Commit("Retry name ".into()))?;
    ime(host, Ime::Closed)?;
    named(host, Named::Enter)?;
    check(
        host.app.tab_title(tabs[0]).unwrap().as_str() == "Retry name",
        "blur retirement or valid editor retry failed",
    )?;
    host.evidence.push("same-view rename preedit Enter/Escape/app-chord suppressed; unopened/blurred/stale commits rejected; fresh retry exact".into());

    let held = host.app.update(Message::Control(Control::TabAction(
        TabAction::BeginRename(tabs[0]),
    )));
    let held_token = host.app.tab_dialog_token().unwrap();
    host.update(Message::Control(Control::TabDialog(
        held_token,
        DialogAction::Cancel,
    )));
    host.drain()?;
    let (new_token, new_input) = open_rename(host)?;
    host.finish(held)?;
    check(
        focused(host, new_input),
        "obsolete rename focus operation disturbed replacement",
    )?;
    host.update(Message::Control(Control::TabDialog(
        new_token,
        DialogAction::Cancel,
    )));
    host.drain()?;

    let first_dir = output.join(saved[0].pid.to_string());
    fs::write(first_dir.join("title.go"), "go")?;
    host.until(|_| first_dir.join("title.done").exists())?;
    check(
        host.app.tab_title(tabs[0]).unwrap().as_str() == "Retry name",
        "OSC replaced manual name",
    )?;
    click(host, |label| label == "Use automatic name")?;
    let osc = Title::new(&format!("OSC 日本語 🦀\u{202e}{}", "界".repeat(300))).unwrap();
    host.until(|host| host.app.tab_title(tabs[0]).as_ref() == Some(&osc))?;
    set_name(host, "Pinned manual")?;
    fs::write(first_dir.join("reset.go"), "go")?;
    host.until(|_| first_dir.join("reset.done").exists())?;
    check(
        host.app.tab_title(tabs[0]).unwrap().as_str() == "Pinned manual",
        "OSC reset replaced manual name",
    )?;
    click(host, |label| label == "Use automatic name")?;
    let fallback = std::env::current_exe()?
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    host.until(|host| {
        host.app
            .tab_title(tabs[0])
            .is_some_and(|title| title.as_str() == fallback)
    })?;
    host.evidence.push("manual > bounded sanitized OSC; OSC ResetTitle preserves manual; explicit reset restores launch-profile fallback".into());

    let before_split: BTreeSet<_> = host
        .app
        .model()
        .sessions()
        .iter()
        .map(|(id, _)| id)
        .collect();
    chord(host, "o")?;
    let split_owner = host
        .app
        .model()
        .sessions()
        .iter()
        .map(|(id, _)| id)
        .find(|id| !before_split.contains(id))
        .ok_or_else(|| io::Error::other("split command was refused"))?;
    host.until(|host| {
        host.app.active_session() == Some(split_owner) && host.app.terminal(split_owner).is_some()
    })?;
    saved.push(ready(host)?);
    check(
        saved
            .iter()
            .map(|snapshot| snapshot.pid)
            .collect::<BTreeSet<_>>()
            .len()
            == 13,
        "split reused a live process identity",
    )?;
    let split = *host
        .app
        .model()
        .workspace()
        .tab(tabs[0])
        .unwrap()
        .grid()
        .layout()
        .splits()
        .next()
        .unwrap();
    host.update(Message::Control(Control::Resize(
        tabs[0],
        iced::widget::pane_grid::ResizeEvent { split, ratio: 0.37 },
    )));
    host.drain()?;
    key(
        host,
        Key::Named(Named::Enter),
        Modifiers::CTRL | Modifiers::SHIFT,
    )?;
    let tree = format!(
        "{:?}",
        host.app.model().workspace().tab(tabs[0]).unwrap().grid()
    );
    select(host, tabs[11])?;
    for _ in 0..11 {
        click(host, |label| label == "Move left")?;
    }
    check(
        host.app.model().workspace().order()[0] == tabs[11],
        "stable last-tab reorder failed",
    )?;
    for snapshot in &saved {
        check(
            snapshot.intact(&host.app, snapshot.session != saved[0].session),
            "reorder changed child, widget, registry, history, mode or selection",
        )?;
    }
    check(
        format!(
            "{:?}",
            host.app.model().workspace().tab(tabs[0]).unwrap().grid()
        ) == tree,
        "reorder changed hidden ratios/zoom/focus",
    )?;
    // Do not request hidden snapshots: the adapter deliberately exposes None.
    // Compare each mode on a real visit, restoring the original zoom afterward.
    for tab in &tabs {
        select(host, *tab)?;
        let zoomed = host
            .app
            .model()
            .workspace()
            .tab(*tab)
            .unwrap()
            .grid()
            .maximized()
            .is_some();
        if zoomed {
            key(
                host,
                Key::Named(Named::Enter),
                Modifiers::CTRL | Modifiers::SHIFT,
            )?;
        }
        for snapshot in saved.iter().filter(|snapshot| snapshot.tab() == *tab) {
            check(
                snapshot.visible_mode_matches(&host.app),
                "mode was not retained on real tab visit",
            )?;
        }
        if zoomed {
            key(
                host,
                Key::Named(Named::Enter),
                Modifiers::CTRL | Modifiers::SHIFT,
            )?;
        }
    }
    check(
        format!(
            "{:?}",
            host.app.model().workspace().tab(tabs[0]).unwrap().grid()
        ) == tree,
        "mode visits did not restore split zoom/ratios/focus",
    )?;
    host.evidence.push("12 tabs / 13 distinctive children: reordered last to first; exact SessionId/TabId/PID/widget/registry and hidden nested ratios/zoom/focus preserved".into());

    for index in 0..8 {
        chord(host, &(index + 1).to_string())?;
        check(
            host.app.model().workspace().active()
                == Some(host.app.model().workspace().order()[index]),
            "direct selection used obsolete position",
        )?;
    }
    chord(host, "9")?;
    for _ in 0..12 {
        key(host, Key::Named(Named::Tab), Modifiers::CTRL)?;
    }
    key(
        host,
        Key::Named(Named::Tab),
        Modifiers::CTRL | Modifiers::SHIFT,
    )?;
    chord(host, "1")?;
    click(host, |label| label == "All tabs (12)")?;
    click(host, |label| label.starts_with("12 · "))?;
    let mouse_last = *host.app.model().workspace().order().last().unwrap();
    check(
        host.app.model().workspace().active() == Some(mouse_last),
        "pointer overflow did not reach tab twelve",
    )?;
    chord(host, "l")?;
    named(host, Named::Home)?;
    for _ in 0..11 {
        named(host, Named::ArrowDown)?;
    }
    check(
        host.app.tab_dialog_selected() == Some(mouse_last),
        "keyboard overflow did not reach last stable tab",
    )?;
    let title = host.app.tab_title(mouse_last).unwrap();
    check(
        targets(host)
            .labels
            .iter()
            .any(|(label, _)| label.starts_with(&format!("12 · {}", title.as_str()))),
        "keyboard-selected overflow row is offscreen",
    )?;
    named(host, Named::Enter)?;
    host.until(|host| host.app.active_session().is_some_and(|id| host.focused(id)))?;
    host.evidence.push("direct/current-order next/previous and actual pointer/keyboard overflow reach all twelve tabs; selected final row visible".into());

    let active = host.app.active_session().unwrap();
    close_overflow_row(host, tabs[0])?;
    check(
        host.app
            .close_confirmation()
            .is_some_and(|(_, count)| count == 2),
        "multi-session tab close did not capture exact owners",
    )?;
    named(host, Named::Escape)?;
    host.until(|host| host.app.close_confirmation().is_none() && host.focused(active))?;
    for snapshot in &saved {
        check(
            snapshot.intact(&host.app, snapshot.session != saved[0].session),
            "Cancel changed an owned process or terminal state",
        )?;
    }
    close_overflow_row(host, tabs[0])?;
    click(host, |label| {
        label == "Close selected" || label == "› Close selected"
    })?;
    host.until(|host| host.app.active_session() == Some(active) && host.focused(active))?;
    host.event(
        widget::key(Key::Character("x".into()), Modifiers::empty(), Some("x")),
        mouse::Cursor::Unavailable,
    );
    host.drain()?;
    let active_pid = saved
        .iter()
        .find(|snapshot| snapshot.session == active)
        .unwrap()
        .pid;
    host.until(|_| {
        fs::read(output.join(active_pid.to_string()).join("input.bin"))
            .ok()
            .as_deref()
            == Some(b"x")
    })?;
    host.until(|host| saved[0].cleaned_up(&host.app) && saved[12].cleaned_up(&host.app))?;
    check(
        host.app.model().workspace().active() == Some(mouse_last),
        "background cleanup changed active tab",
    )?;
    host.evidence.push("multi-session background Cancel harmless; confirm cleans only exact two owners; active child remains focused and accepts exact x".into());

    chord(host, "3")?;
    let order = host.app.model().workspace().order().to_vec();
    let closing = order[2];
    let neighbor = order[3];
    chord(host, "q")?;
    named(host, Named::Tab)?;
    named(host, Named::Enter)?;
    host.until(|host| {
        host.app.model().workspace().active() == Some(neighbor)
            && host.app.active_session().is_some_and(|id| host.focused(id))
    })?;
    check(
        host.app.model().workspace().tab(closing).is_none(),
        "active tab not removed",
    )?;
    for snapshot in &saved {
        if host.app.model().target(snapshot.session).is_some() {
            check(
                snapshot.intact(
                    &host.app,
                    snapshot.session != active && snapshot.session != saved[0].session,
                ),
                "close affected surviving process/state",
            )?;
            fs::write(output.join(snapshot.pid.to_string()).join("exit.go"), "go")?;
        }
    }
    host.until(|host| {
        host.app
            .model()
            .sessions()
            .iter()
            .all(|(_, entry)| matches!(entry.lifecycle(), Lifecycle::Exited { code: Some(0) }))
    })?;
    host.update(Message::CloseWindow);
    host.until(|host| {
        host.app.model().sessions().is_empty()
            && saved.iter().all(|snapshot| snapshot.cleaned_up(&host.app))
    })?;
    let mut receipts = Vec::new();
    for snapshot in &saved {
        let actual =
            fs::read(output.join(snapshot.pid.to_string()).join("input.bin")).unwrap_or_default();
        let expected: &[u8] = if snapshot.session == saved[0].session {
            b"\x1bOQ"
        } else if snapshot.session == active {
            b"x"
        } else {
            b""
        };
        check(
            actual == expected,
            "application shortcut or modal text leaked to a PTY",
        )?;
        receipts.push(format!(
            "tab={} session={} pid={} {:?}",
            snapshot.tab().get(),
            snapshot.session.get(),
            snapshot.pid,
            snapshot.cleanup.completion().unwrap()
        ));
    }
    host.evidence.push("active removal selects current right neighbor; all 13 cleanup receipts acknowledged; exact fixture byte streams only; no stale registry owners".into());
    Ok(receipts)
}
