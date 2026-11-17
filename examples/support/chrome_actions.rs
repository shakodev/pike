//! Actual current chrome: measured visible IDs and the public effective menu.
//! Shared only by finite fixtures; no production state or manufactured actions.
use crate::{host::Host, widget};
use iced::{
    Event,
    keyboard::{Key, Modifiers, key::Named},
    mouse,
};
use pike::app::commands::Command;
use std::io;

pub fn click_id(host: &mut Host, id: impl Into<iced::widget::Id>) -> io::Result<()> {
    let id = id.into();
    for _ in 0..8 {
        let mut targets = widget::Targets::default();
        host.operate(&mut targets);
        if let Some((_, bounds)) = targets
            .containers
            .iter()
            .find(|(candidate, _)| candidate.as_ref() == Some(&id))
        {
            host.click(bounds.center());
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
    Err(io::Error::other(format!(
        "current chrome target missing: {id:?}"
    )))
}

fn named(host: &mut Host, key: Named) -> io::Result<()> {
    host.event(
        Event::Keyboard(iced::keyboard::Event::ModifiersChanged(Modifiers::empty())),
        mouse::Cursor::Unavailable,
    );
    host.event(
        widget::key(Key::Named(key), Modifiers::empty(), None),
        mouse::Cursor::Unavailable,
    );
    host.drain()
}

pub fn menu(host: &mut Host, command: Command) -> io::Result<()> {
    click_id(host, "command-menu-control")?;
    let reference = host.app.command_reference();
    let index = reference
        .iter()
        .position(|row| row.command == command)
        .ok_or_else(|| io::Error::other("effective menu command missing"))?;
    if !host.app.interaction_blocked() || !reference[index].enabled {
        return Err(io::Error::other("current menu/action unavailable"));
    }
    named(host, Named::Home)?;
    for _ in 0..index {
        named(host, Named::ArrowDown)?;
    }
    named(host, Named::Enter)
}
