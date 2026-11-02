//! R1 review regression: hold real Enter, then change native/App window focus.
use super::{
    host::Host,
    scenarios::{bytes, check, named},
    widget,
};
use iced::{
    Event,
    keyboard::{Key, Modifiers, key::Named},
    mouse,
};
use pike::{
    app::{
        CloseAction, Control, Message,
        commands::{Command, HelpAction},
    },
    identity::SessionId,
};
use std::{io, path::Path};

pub fn verify(host: &mut Host, owner: SessionId, file: &Path, expected: &[u8]) -> io::Result<()> {
    for native_only in [true, false] {
        for roundtrip in [false, true] {
            crate::chrome_actions::click_id(host, "command-menu-control")?;
            named(host, Named::Home, Modifiers::empty())?;
            named(host, Named::ArrowDown, Modifiers::empty())?; // Close pane.
            let (_, held) = host.held_event(
                widget::key(Key::Named(Named::Enter), Modifiers::empty(), None),
                mouse::Cursor::Unavailable,
            );
            check(
                held.iter()
                    .filter(|m| {
                        matches!(
                            m,
                            Message::Control(Control::HelpAction(
                                _,
                                HelpAction::Execute(Command::ClosePane)
                            ))
                        )
                    })
                    .count()
                    == 1,
                "real help Enter did not emit exactly one held ClosePane",
            )?;
            let count = host.app.model().sessions().len();
            host.event(
                Event::Window(iced::window::Event::Unfocused),
                mouse::Cursor::Unavailable,
            );
            if !native_only {
                host.update(Message::WindowFocused(false));
            }
            if roundtrip {
                host.event(
                    Event::Window(iced::window::Event::Focused),
                    mouse::Cursor::Unavailable,
                );
                if !native_only {
                    host.update(Message::WindowFocused(true));
                }
            }
            // Intentionally do not drain subscribed focus messages here. This
            // covers the native-only gap before App blur/refocus delivery.
            for message in held {
                host.update(message);
            }
            check(
                host.app.close_confirmation().is_none()
                    && host.app.interaction_blocked()
                    && host.app.model().sessions().len() == count,
                "held help Enter survived native/App blur or blur/refocus",
            )?;
            host.event(
                Event::Window(iced::window::Event::Focused),
                mouse::Cursor::Unavailable,
            );
            host.drain()?;
            host.update(Message::WindowFocused(true));
            // A fresh menu command must work after input returns; cancel its
            // conservative confirmation so the controlled child stays alive.
            named(host, Named::Home, Modifiers::empty())?;
            named(host, Named::ArrowDown, Modifiers::empty())?;
            named(host, Named::Enter, Modifiers::empty())?;
            let (token, _) = host
                .app
                .close_confirmation()
                .ok_or_else(|| io::Error::other("fresh help action was trapped after refocus"))?;
            host.update(Message::Control(Control::CloseAction(CloseAction::Cancel(
                token,
            ))));
            host.until(|host| host.focused(owner))?;
            crate::chrome_actions::click_id(host, "command-menu-control")?;
            named(host, Named::End, Modifiers::empty())?;
            named(host, Named::Escape, Modifiers::empty())?;
            host.until(|host| host.focused(owner))?;
            check(
                host.app.active_session() == Some(owner)
                    && host.app.model().sessions().len() == count,
                "help refocus/dismiss changed the original session",
            )?;
            bytes(host, file, expected)?;
            host.evidence.push(format!("held real Help Enter revoked: native_only={native_only} roundtrip={roundtrip}; fresh Execute/navigation/Escape/focus and child bytes intact"));
        }
    }
    Ok(())
}
