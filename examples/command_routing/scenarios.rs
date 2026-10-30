//! Assertions observe real host effects, widget focus and native child bytes.
use super::{check, host::Host, input_files, press};
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::{Point, mouse};
use pike::app::{Control, Message};
use pike::identity::SessionId;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const APP: Modifiers = Modifiers::CTRL.union(Modifiers::SHIFT);

fn admitted(host: &Host) -> BTreeMap<SessionId, usize> {
    host.app
        .model()
        .sessions()
        .iter()
        .filter_map(|(id, _)| {
            let input = host.app.terminal(id)?.input_status();
            Some((
                id,
                input.written_bytes + input.outstanding_bytes + input.cancelled_bytes,
            ))
        })
        .collect()
}

fn zero_admission(host: &Host, before: &BTreeMap<SessionId, usize>, label: &str) -> io::Result<()> {
    for (id, after) in admitted(host) {
        if let Some(before) = before.get(&id) {
            check(
                *before == after,
                &format!("{label}: unintended input for session {}", id.get()),
            )?;
        }
    }
    Ok(())
}

fn command(host: &mut Host, event: iced::Event, label: &str, repeat: bool) -> io::Result<()> {
    let before = admitted(host);
    let actions = host.controls;
    let status = host.event(event, mouse::Cursor::Unavailable);
    check(
        status == iced::event::Status::Captured,
        &format!("{label}: event was not captured"),
    )?;
    check(
        host.controls == actions + usize::from(!repeat),
        &format!(
            "{label}: wrong application action count {}",
            host.controls - actions
        ),
    )?;
    zero_admission(host, &before, label)
}

fn character(host: &mut Host, key: &str, label: &str, repeat: bool) -> io::Result<()> {
    command(
        host,
        press(Key::Character(key.into()), APP, Some(key), repeat),
        label,
        repeat,
    )
}

fn selected(host: &mut Host, id: SessionId) -> io::Result<()> {
    host.until(|host| host.app.active_session() == Some(id) && host.focused(id))?;
    let visible: Vec<_> = host
        .app
        .model()
        .sessions()
        .iter()
        .filter_map(|(candidate, _)| {
            host.app
                .terminal(candidate)
                .filter(|terminal| terminal.is_visible())
                .map(|_| candidate)
        })
        .collect();
    for candidate in visible {
        check(
            host.focused(candidate) == (candidate == id),
            "actual widget focus is not exclusive",
        )?;
    }
    Ok(())
}

struct Bytes {
    owners: BTreeMap<SessionId, PathBuf>,
    expected: BTreeMap<PathBuf, Vec<u8>>,
}

impl Bytes {
    fn bind(&mut self, host: &mut Host, output: &Path, id: SessionId) -> io::Result<()> {
        host.until(|_| {
            input_files(output)
                .iter()
                .filter(|path| !self.expected.contains_key(*path))
                .count()
                == 1
        })?;
        let file = input_files(output)
            .into_iter()
            .find(|path| !self.expected.contains_key(path))
            .unwrap();
        self.owners.insert(id, file.clone());
        self.expected.insert(file, Vec::new());
        Ok(())
    }

    fn type_text(&mut self, host: &mut Host, id: SessionId, text: &str) -> io::Result<()> {
        selected(host, id)?;
        let before = admitted(host);
        host.event(
            press(
                Key::Character(text.into()),
                Modifiers::empty(),
                Some(text),
                false,
            ),
            mouse::Cursor::Unavailable,
        );
        self.expected
            .get_mut(&self.owners[&id])
            .unwrap()
            .extend_from_slice(text.as_bytes());
        let after = admitted(host);
        for (candidate, before) in before {
            check(
                after[&candidate] == before + if candidate == id { text.len() } else { 0 },
                "typing admission went to an inactive terminal or wrong owner",
            )?;
        }
        Ok(())
    }

    fn input(
        &mut self,
        host: &mut Host,
        id: SessionId,
        event: iced::Event,
        expected: &str,
    ) -> io::Result<()> {
        selected(host, id)?;
        let before = admitted(host);
        host.event(event, mouse::Cursor::Unavailable);
        self.expected
            .get_mut(&self.owners[&id])
            .unwrap()
            .extend_from_slice(expected.as_bytes());
        for (candidate, before) in before {
            let after = admitted(host)[&candidate];
            check(
                after == before + if candidate == id { expected.len() } else { 0 },
                "IME/text input had the wrong destination or byte count",
            )?;
        }
        Ok(())
    }

    fn priorities(&mut self, host: &mut Host, id: SessionId) -> io::Result<()> {
        use iced::advanced::input_method::Event as Ime;
        for (key, modifiers, text) in [
            (
                Key::Character("e".into()),
                Modifiers::CTRL | Modifiers::ALT,
                "€",
            ),
            (
                Key::Named(Named::Space),
                Modifiers::CTRL | Modifiers::ALT,
                "\u{a0}",
            ),
            (Key::Named(Named::Space), Modifiers::empty(), "´"),
            (Key::Unidentified, Modifiers::empty(), "é"),
        ] {
            self.input(host, id, press(key, modifiers, Some(text), false), text)?;
        }
        self.input(
            host,
            id,
            press(Key::Unidentified, Modifiers::empty(), None, false),
            "",
        )?;
        self.input(
            host,
            id,
            press(Key::Unidentified, Modifiers::empty(), Some(""), false),
            "",
        )?;
        host.redraw();
        host.event(
            iced::Event::InputMethod(Ime::Opened),
            mouse::Cursor::Unavailable,
        );
        host.event(
            iced::Event::InputMethod(Ime::Preedit("ni".into(), Some(0..2))),
            mouse::Cursor::Unavailable,
        );
        check(
            host.app.terminal(id).unwrap().is_composing(),
            "IME preedit did not own input",
        )?;
        let actions = host.controls;
        let before = admitted(host);
        for event in [
            press(Key::Character("o".into()), APP, Some("o"), false),
            press(Key::Character("w".into()), APP, Some("w"), false),
            press(Key::Character("t".into()), APP, Some("t"), false),
            press(Key::Named(Named::ArrowLeft), Modifiers::ALT, None, false),
            press(Key::Named(Named::Enter), APP, Some("\r"), false),
            press(
                Key::Character("c".into()),
                Modifiers::CTRL,
                Some("\x03"),
                false,
            ),
        ] {
            check(
                host.event(event, mouse::Cursor::Unavailable) == iced::event::Status::Captured,
                "composition key escaped capture",
            )?;
        }
        check(
            host.controls == actions,
            "application command ran during composition",
        )?;
        zero_admission(host, &before, "composition preedit and shortcut fallback")?;
        self.input(
            host,
            id,
            iced::Event::InputMethod(Ime::Commit("你好".into())),
            "你好",
        )?;
        host.event(
            iced::Event::InputMethod(Ime::Closed),
            mouse::Cursor::Unavailable,
        );
        check(
            !host.app.terminal(id).unwrap().is_composing(),
            "IME remained active after close",
        )?;

        host.event(
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(APP)),
            mouse::Cursor::Unavailable,
        );
        host.event(
            iced::Event::Window(iced::window::Event::Unfocused),
            mouse::Cursor::Unavailable,
        );
        let actions = host.controls;
        let before = admitted(host);
        host.event(
            press(Key::Character("t".into()), APP, Some("t"), false),
            mouse::Cursor::Unavailable,
        );
        host.event(
            press(
                Key::Character("x".into()),
                Modifiers::empty(),
                Some("x"),
                false,
            ),
            mouse::Cursor::Unavailable,
        );
        host.drain()?;
        check(
            host.controls == actions,
            "blurred window dispatched an application command",
        )?;
        zero_admission(host, &before, "window blur")?;
        host.event(
            iced::Event::Window(iced::window::Event::Focused),
            mouse::Cursor::Unavailable,
        );
        host.until(|host| {
            host.app
                .terminal(id)
                .unwrap()
                .renderable_content()
                .is_some_and(|content| content.focused)
        })?;
        host.event(
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(Modifiers::empty())),
            mouse::Cursor::Unavailable,
        );
        self.type_text(host, id, "m")
    }

    fn verify(&self, host: &mut Host, output: &Path) -> io::Result<()> {
        for (path, expected) in &self.expected {
            host.until(|_| fs::read(path).is_ok_and(|bytes| bytes.len() >= expected.len()))?;
            check(
                fs::read(path)? == *expected,
                "raw child input differs from the independent expected byte stream",
            )?;
        }
        let mut report = fs::File::create(output.join("raw-owners.txt"))?;
        for (id, file) in &self.owners {
            let bytes = &self.expected[file];
            writeln!(
                report,
                "session={} child={} exact_bytes={}",
                id.get(),
                file.file_stem().unwrap().to_string_lossy(),
                bytes.len()
            )?;
        }
        Ok(())
    }
}

pub fn verify(
    host: &mut Host,
    output: &Path,
    left: SessionId,
    first_file: PathBuf,
) -> io::Result<()> {
    let mut bytes = Bytes {
        owners: [(left, first_file.clone())].into(),
        expected: [(first_file, vec![3])].into(),
    };
    let top = host.app.active_session().unwrap();
    bytes.bind(host, output, top)?;
    for _ in 0..3 {
        character(host, "o", "repeated split right", true)?;
    }
    check(
        host.app.model().sessions().len() == 2,
        "repeated split created another session",
    )?;
    host.redraw();
    character(host, "e", "split down", false)?;
    host.until(|host| {
        host.app
            .active_session()
            .is_some_and(|id| id != top && host.app.terminal(id).is_some() && host.focused(id))
    })?;
    let bottom = host.app.active_session().unwrap();
    bytes.bind(host, output, bottom)?;
    for (key, id) in [
        (Named::ArrowUp, top),
        (Named::ArrowLeft, left),
        (Named::ArrowRight, top),
        (Named::ArrowDown, bottom),
    ] {
        command(
            host,
            press(Key::Named(key), Modifiers::ALT, None, false),
            "directional focus",
            false,
        )?;
        selected(host, id)?;
        bytes.type_text(host, id, "d")?;
    }
    for id in [left, top, bottom] {
        let bounds = host.bounds(id).unwrap();
        let before = host.controls;
        host.click(Point::new(bounds.x + 40.0, bounds.y - 14.0));
        selected(host, id)?;
        check(
            host.controls == before + 1,
            "pane header produced duplicate focus actions",
        )?;
        bytes.type_text(host, id, "h")?;
        let point = Point::new(bounds.x + 25.0, bounds.y + 25.0);
        host.click(point);
        selected(host, id)?;
        bytes.type_text(host, id, "s")?;
    }
    command(
        host,
        press(Key::Named(Named::Enter), APP, None, false),
        "maximize",
        false,
    )?;
    let target = host.app.active_target().unwrap();
    check(
        host.app
            .model()
            .workspace()
            .tab(target.tab)
            .unwrap()
            .grid()
            .maximized()
            == Some(target.pane),
        "maximize did not preserve and select its pane",
    )?;
    command(
        host,
        press(Key::Named(Named::Enter), APP, None, true),
        "repeat maximize",
        true,
    )?;
    command(
        host,
        press(Key::Named(Named::Enter), APP, None, false),
        "restore",
        false,
    )?;
    check(
        host.app
            .model()
            .workspace()
            .tab(target.tab)
            .unwrap()
            .grid()
            .maximized()
            .is_none(),
        "restore lost the full layout",
    )?;
    bytes.priorities(host, bottom)?;

    let mut tab_owners = vec![bottom];
    for _ in 1..9 {
        character(host, "t", "new tab", false)?;
        let previous = *tab_owners.last().unwrap();
        host.until(|host| {
            host.app.active_session().is_some_and(|id| {
                id != previous && host.app.terminal(id).is_some() && host.focused(id)
            })
        })?;
        let id = host.app.active_session().unwrap();
        bytes.bind(host, output, id)?;
        selected(host, id)?;
        bytes.type_text(host, id, "t")?;
        tab_owners.push(id);
        character(host, "t", "repeat new tab", true)?;
    }
    check(
        host.app.model().workspace().order().len() == 9,
        "new-tab repeat grew the workspace",
    )?;
    for (index, id) in tab_owners.iter().copied().enumerate() {
        character(
            host,
            &(index + 1).to_string(),
            "direct tab selection",
            false,
        )?;
        selected(host, id)?;
        bytes.type_text(host, id, "n")?;
    }
    command(
        host,
        press(Key::Named(Named::Tab), Modifiers::CTRL, None, false),
        "next tab wraps",
        false,
    )?;
    selected(host, bottom)?;
    command(
        host,
        press(Key::Named(Named::Tab), APP, None, false),
        "previous tab wraps",
        false,
    )?;
    selected(host, *tab_owners.last().unwrap())?;

    for id in bytes.owners.keys().copied().collect::<Vec<_>>() {
        let target = host.app.model().target(id).unwrap();
        host.update(Message::Control(Control::Focus(target)));
        selected(host, id)?;
        bytes.type_text(host, id, ".")?;
        bytes.input(
            host,
            id,
            press(
                Key::Character("d".into()),
                Modifiers::CTRL,
                Some("\x04"),
                false,
            ),
            "\x04",
        )?;
        host.until(|host| {
            matches!(
                host.app
                    .model()
                    .sessions()
                    .get(id)
                    .map(|entry| entry.lifecycle()),
                Some(pike::session::Lifecycle::Exited { code: Some(0) })
            )
        })?;
    }
    bytes.verify(host, output)
}
