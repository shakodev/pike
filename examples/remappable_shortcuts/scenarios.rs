use super::{configuration, host::Host, widget};
use iced::{
    Event,
    keyboard::{self, Key, Modifiers, key::Named},
    mouse,
};
use pike::{
    app::{Control, Message, commands::Command},
    identity::SessionId,
};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(super) fn check(condition: bool, message: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}
fn key(
    host: &mut Host,
    base: Key,
    modified: Key,
    modifiers: Modifiers,
    text: Option<&str>,
    repeat: bool,
) -> io::Result<()> {
    let Event::Keyboard(keyboard::Event::KeyPressed {
        key,
        modified_key,
        physical_key,
        location,
        ..
    }) = widget::key(base, modifiers, text)
    else {
        unreachable!()
    };
    host.event(
        Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)),
        mouse::Cursor::Unavailable,
    );
    host.event(
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modified_key: modified,
            physical_key,
            location,
            modifiers,
            text: text.map(Into::into),
            repeat,
        }),
        mouse::Cursor::Unavailable,
    );
    let _ = modified_key;
    host.event(
        Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::empty())),
        mouse::Cursor::Unavailable,
    );
    host.drain()
}
pub(super) fn named(host: &mut Host, keycode: Named, modifiers: Modifiers) -> io::Result<()> {
    key(
        host,
        Key::Named(keycode),
        Key::Named(keycode),
        modifiers,
        None,
        false,
    )
}
fn character(
    host: &mut Host,
    character: &str,
    modifiers: Modifiers,
    text: Option<&str>,
) -> io::Result<()> {
    key(
        host,
        Key::Character(character.into()),
        Key::Character(character.into()),
        modifiers,
        text,
        false,
    )
}
fn menu(host: &mut Host, command: Command) -> io::Result<()> {
    crate::chrome_actions::click_id(host, "command-menu-control")?;
    check(
        host.app.interaction_blocked(),
        "actual root menu did not open help",
    )?;
    let references = host.app.command_reference();
    let index = references
        .iter()
        .position(|r| r.command == command)
        .ok_or_else(|| io::Error::other("command absent from effective menu"))?;
    check(references[index].enabled, "requested menu command disabled")?;
    named(host, Named::Home, Modifiers::empty())?;
    for _ in 0..index {
        named(host, Named::ArrowDown, Modifiers::empty())?;
    }
    named(host, Named::Enter, Modifiers::empty())
}
fn file(host: &mut Host, output: &Path) -> io::Result<(SessionId, PathBuf)> {
    host.until(|host| {
        host.app.active_session().is_some_and(|id| {
            host.app
                .terminal(id)
                .and_then(|t| t.renderable_content())
                .is_some_and(|c| {
                    c.viewport
                        .cells()
                        .iter()
                        .map(|c| c.c)
                        .collect::<String>()
                        .contains("PIKE32_READY=")
                })
                && host.focused(id)
        })
    })?;
    let id = host.app.active_session().unwrap();
    let content = host.app.terminal(id).unwrap().renderable_content().unwrap();
    let text: String = content.viewport.cells().iter().map(|cell| cell.c).collect();
    let pid = text
        .split("PIKE32_READY=")
        .nth(1)
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .ok_or_else(|| io::Error::other("child PID marker missing"))?;
    Ok((id, output.join(format!("{pid}.bin"))))
}
pub(super) fn bytes(host: &mut Host, file: &Path, expected: &[u8]) -> io::Result<()> {
    host.until(|_| fs::read(file).is_ok_and(|actual| actual == expected))
        .map_err(|_| {
            io::Error::other(format!(
                "synthetic byte mismatch: expected {expected:?}, actual {:?}",
                fs::read(file)
            ))
        })
}
fn reload(host: &mut Host, output: &Path, suffix: &str) -> io::Result<()> {
    fs::write(
        output.join("settings.toml"),
        format!("{}\n{suffix}", configuration(output)?),
    )?;
    menu(host, Command::ReloadSettings)?;
    host.until(|host| !host.app.settings_pending())
}

pub fn verify(host: &mut Host, output: &Path) -> io::Result<()> {
    let cs = Modifiers::CTRL | Modifiers::SHIFT;
    host.update(Message::WindowFocused(true));
    let (original, original_file) = file(host, output)?;
    host.event(
        Event::Window(iced::window::Event::Focused),
        mouse::Cursor::Unavailable,
    );
    host.redraw();
    // Defaults dispatch once before the child encoder; repeats are captured.
    let count = host.controls;
    character(host, "t", cs, Some("\u{14}"))?;
    key(
        host,
        Key::Character("t".into()),
        Key::Character("T".into()),
        cs,
        Some("\u{14}"),
        true,
    )?;
    let (second, second_file) = file(host, output)?;
    check(
        second != original && host.app.model().sessions().len() == 2 && host.controls == count + 1,
        "default shortcut duplicated or leaked repeat",
    )?;
    bytes(host, &original_file, b"")?;
    bytes(host, &second_file, b"")?;
    host.evidence
        .push("default create: one Control, repeat captured, both raw child streams empty".into());
    // A real native menu keyboard selection reads clipboard exactly once.
    host.clipboard.0 = Some("PIKE32_MENU_PASTE".into());
    menu(host, Command::Paste)?;
    host.until(|host| host.focused(second))?;
    bytes(host, &second_file, b"PIKE32_MENU_PASTE")?;
    menu(host, Command::Copy)?;
    host.until(|host| host.focused(second))?;
    bytes(host, &second_file, b"PIKE32_MENU_PASTE")?;
    host.evidence.push(
        "actual root menu/arrow/Enter paste and copy return terminal focus; no duplicate bytes"
            .into(),
    );
    let mappings = "[[shortcuts]]\ncommand='new-tab'\nkey='k'\nmodifiers=['ctrl','shift']\n[[shortcuts]]\ncommand='font-larger'\nkey='t'\nmodifiers=['ctrl','shift']\n[[shortcuts]]\ncommand='close-pane'\nunbind=true\n[[shortcuts]]\ncommand='resize-left'\nunbind=true\n[[shortcuts]]\ncommand='copy'\nunbind=true\n[[shortcuts]]\ncommand='paste'\nkey='f2'\nmodifiers=['ctrl','shift']\n[[shortcuts]]\ncommand='history-page-up'\nunbind=true";
    reload(host, output, mappings)?;
    check(
        host.app.settings_error().is_none(),
        "valid custom mappings were rejected",
    )?;
    let reference = host.app.command_reference();
    check(
        reference
            .iter()
            .find(|r| r.command == Command::NewTab)
            .unwrap()
            .bindings
            == ["Ctrl+Shift+K"],
        "reference displays stale default new-tab chord",
    )?;
    check(
        reference
            .iter()
            .find(|r| r.command == Command::Copy)
            .unwrap()
            .bindings
            .is_empty(),
        "unbound copy still listed",
    )?;
    let before = host.app.model().sessions().len();
    character(host, "t", cs, Some("\u{14}"))?;
    check(
        host.app.model().sessions().len() == before,
        "overridden old T still creates a tab",
    )?;
    check(
        host.app
            .terminal(second)
            .unwrap()
            .renderable_content()
            .unwrap()
            .terminal_size
            .cell_height
            > 0.0,
        "font consumer unavailable",
    )?;
    host.clipboard.0 = Some("PIKE32_CUSTOM_PASTE".into());
    named(host, Named::F2, cs)?;
    bytes(host, &second_file, b"PIKE32_MENU_PASTEPIKE32_CUSTOM_PASTE")?;
    // Released chords use the same native byte encoder, including resize/history.
    for (ch, text) in [("w", "\u{17}"), ("c", "\u{3}"), ("v", "\u{16}")] {
        character(host, ch, cs, Some(text))?;
    }
    named(host, Named::ArrowLeft, Modifiers::ALT | Modifiers::SHIFT)?;
    named(host, Named::PageUp, Modifiers::SHIFT)?;
    character(host, "Z", Modifiers::empty(), Some("Z"))?;
    let mut expected =
        b"PIKE32_MENU_PASTEPIKE32_CUSTOM_PASTE\x17\x03\x16\x1b[1;4D\x1b[5;2~Z".to_vec();
    bytes(host, &second_file, &expected)?;
    check(
        host.app.model().sessions().len() == before && host.app.close_confirmation().is_none(),
        "unbound old chord retained an application action",
    )?;
    host.evidence.push("reload overrides/unbind remove stale copy/paste/close/resize/history chords; raw native fallback bytes exact".into());
    // Failed effective-default conflict rejects the entire candidate, including theme.
    let invalid =
        "theme='light'\n[[shortcuts]]\ncommand='new-tab'\nkey='o'\nmodifiers=['ctrl','shift']";
    // Put the global theme before profile table, preserving an otherwise valid candidate.
    let config = configuration(output)?.replacen("version = 1", "version = 1\ntheme='light'", 1);
    fs::write(
        output.join("settings.toml"),
        format!(
            "{config}\n{}",
            invalid.strip_prefix("theme='light'\n").unwrap()
        ),
    )?;
    menu(host, Command::ReloadSettings)?;
    host.until(|host| !host.app.settings_pending())?;
    check(
        host.app.settings_error().is_some(),
        "ambiguous default conflict accepted",
    )?;
    check(
        host.app
            .command_reference()
            .iter()
            .find(|r| r.command == Command::NewTab)
            .unwrap()
            .bindings
            == ["Ctrl+Shift+K"],
        "failed reload changed last-valid map",
    )?;
    host.clipboard.0 = Some("LAST_VALID".into());
    named(host, Named::F2, cs)?;
    expected.extend_from_slice(b"LAST_VALID");
    bytes(host, &second_file, &expected)?;
    host.evidence.push("ambiguous surviving-default conflict rejected atomically; previous custom paste remains live".into());
    // Custom help alternatives replace F1, with enough rows/bindings to exercise measured scrolling.
    let mut many = mappings.to_owned();
    for symbol in [
        "b", "d", "f", "g", "h", "i", "j", "m", "n", "p", "s", "u", "x", "y", "z", "?", "[", "]",
        ";", ",", ".", "/", "ö",
    ] {
        many.push_str(&format!(
            "\n[[shortcuts]]\ncommand='help'\nkey='{symbol}'\nmodifiers=['ctrl','shift']\n"
        ));
    }
    reload(host, output, &many)?;
    check(
        host.app.settings_error().is_none(),
        "many effective help bindings rejected",
    )?;
    let help = host
        .app
        .command_reference()
        .into_iter()
        .find(|r| r.command == Command::Help)
        .unwrap();
    check(
        help.bindings.len() == 23 && !help.bindings.iter().any(|b| b == "F1"),
        "help alternatives are stale/truncated",
    )?;
    key(
        host,
        Key::Character("/".into()),
        Key::Character("?".into()),
        cs,
        Some("?"),
        false,
    )?;
    check(
        host.app.interaction_blocked(),
        "shifted logical symbol did not open help",
    )?;
    named(host, Named::End, Modifiers::empty())?;
    let mut targets = widget::Targets::default();
    host.operate(&mut targets);
    check(
        targets
            .labels
            .iter()
            .any(|(label, _)| label.contains("profile 16")),
        "keyboard End failed to reveal last help row",
    )?;
    let sessions = host.app.model().sessions().len();
    named(host, Named::Enter, Modifiers::empty())?;
    check(
        host.app.interaction_blocked() && host.app.model().sessions().len() == sessions,
        "disabled help command executed",
    )?;
    named(host, Named::PageUp, Modifiers::empty())?;
    named(host, Named::Home, Modifiers::empty())?;
    named(host, Named::Escape, Modifiers::empty())?;
    host.until(|host| host.focused(second))?;
    // AltGr and native composition outrank custom logical mappings.
    character(host, "ö", cs | Modifiers::ALT, Some("€"))?;
    expected.extend_from_slice("€".as_bytes());
    bytes(host, &second_file, &expected)?;
    use iced::advanced::input_method::Event as Ime;
    host.redraw();
    host.redraw(); // Existing native IME disable/enable handoff needs both paints.
    for ime in [Ime::Opened, Ime::Preedit("合".into(), Some(0..3))] {
        host.event(Event::InputMethod(ime), mouse::Cursor::Unavailable);
    }
    check(
        host.app.terminal(second).unwrap().is_composing(),
        "native IME preedit did not acquire ownership",
    )?;
    character(host, "k", cs, Some("\u{b}"))?;
    check(
        host.app.model().sessions().len() == sessions,
        "custom shortcut bypassed native IME ownership",
    )?;
    host.event(
        Event::InputMethod(Ime::Commit("合".into())),
        mouse::Cursor::Unavailable,
    );
    host.event(Event::InputMethod(Ime::Closed), mouse::Cursor::Unavailable);
    expected.extend_from_slice("合".as_bytes());
    bytes(host, &second_file, &expected)?;
    host.evidence.push("23 effective help bindings, measured End/PageUp/Home navigation, disabled command, Escape focus, shifted symbol/non-US/AltGr/IME priority".into());
    // An asynchronous reload may finish while help owns focus. Held old rows
    // must be rejected and the newly rendered menu must remain navigable.
    let refreshed =
        format!("{many}\n[[shortcuts]]\ncommand='help'\nkey='ß'\nmodifiers=['ctrl','shift']\n");
    fs::write(
        output.join("settings.toml"),
        format!("{}\n{refreshed}", configuration(output)?),
    )?;
    let pending_reload = host
        .app
        .update(Message::Control(Control::Command(Command::ReloadSettings)));
    check(
        host.app.settings_pending(),
        "reload did not retain its asynchronous pending state",
    )?;
    crate::chrome_actions::click_id(host, "command-menu-control")?;
    let held = widget::click(&host.app, &host.renderer, |message| {
        matches!(
            message,
            Message::Control(Control::HelpAction(
                _,
                pike::app::commands::HelpAction::Execute(Command::NewTab)
            ))
        )
    })
    .ok_or_else(|| io::Error::other("held old help row not hit-testable"))?;
    host.finish(pending_reload)?;
    check(
        !host.app.settings_pending() && host.app.interaction_blocked(),
        "reload trapped or dismissed open help",
    )?;
    let count = host.app.model().sessions().len();
    host.update(held);
    check(
        host.app.model().sessions().len() == count && host.app.interaction_blocked(),
        "held old help row survived reload",
    )?;
    check(
        host.app
            .command_reference()
            .into_iter()
            .find(|r| r.command == Command::Help)
            .unwrap()
            .bindings
            .len()
            == 24,
        "open menu reference did not refresh after reload",
    )?;
    named(host, Named::Home, Modifiers::empty())?;
    named(host, Named::Escape, Modifiers::empty())?;
    host.until(|host| host.focused(second))?;
    host.evidence.push("actual asynchronous reload while help is open rejects held old row, refreshes 24 live help bindings and preserves Escape/focus".into());

    super::focus::verify(host, second, &second_file, &expected)?;

    // Deferred clipboard work is invalidated even after returning to the same owner.
    host.clipboard.0 = Some("MUST_NOT_ARRIVE".into());
    let delayed = host
        .app
        .update(Message::Control(Control::Command(Command::Paste)));
    crate::chrome_actions::click_id(host, "command-menu-control")?;
    named(host, Named::Escape, Modifiers::empty())?;
    host.finish(delayed)?;
    host.drain()?;
    let delayed = host
        .app
        .update(Message::Control(Control::Command(Command::Paste)));
    host.update(Message::Control(Control::ActivateTab(
        host.app.model().target(original).unwrap().tab,
    )));
    host.update(Message::Control(Control::ActivateTab(
        host.app.model().target(second).unwrap().tab,
    )));
    host.until(|host| host.focused(second))?;
    host.finish(delayed)?;
    host.drain()?;
    let delayed = host
        .app
        .update(Message::Control(Control::Command(Command::Paste)));
    host.event(
        Event::Window(iced::window::Event::Unfocused),
        mouse::Cursor::Unavailable,
    );
    host.event(
        Event::Window(iced::window::Event::Focused),
        mouse::Cursor::Unavailable,
    );
    host.finish(delayed)?;
    host.drain()?;
    host.until(|host| host.focused(second))?;
    character(host, "!", Modifiers::empty(), Some("!"))?;
    expected.push(b'!');
    bytes(host, &second_file, &expected)?;
    bytes(host, &original_file, b"")?;
    // The replacement structural shortcut remains live after all failed/successful reloads.
    character(host, "k", cs, Some("\u{b}"))?;
    let (third, third_file) = file(host, output)?;
    check(
        third != second && host.app.model().sessions().len() == sessions + 1,
        "new mapping stopped working after reloads",
    )?;
    bytes(host, &third_file, b"")?;
    host.evidence.push("delayed clipboard modal/tab/native-blur roundtrips discarded; custom structural mapping still dispatches once with zero child bytes".into());
    Ok(())
}
