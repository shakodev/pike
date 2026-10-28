use super::*;
use crate::{PasteError, PreparedTerminal, PASTE_BYTE_LIMIT};
use iced::keyboard::{key, Event as KeyEvent};
use std::time::Duration;

#[derive(Default)]
struct Clipboard(Option<String>);
impl iced_core::Clipboard for Clipboard {
    fn read(&self, _: ClipboardKind) -> Option<String> {
        self.0.clone()
    }
    fn write(&mut self, _: ClipboardKind, _: String) {
        panic!("paste must not write clipboard")
    }
}

#[test]
fn paste_shortcuts_share_commands_and_report_unavailable_or_oversize_text() {
    #[cfg(windows)]
    let (program, args) = ("cmd.exe", vec!["/D", "/Q", "/K"]);
    #[cfg(not(windows))]
    let (program, args) = ("/bin/sh", vec!["-c", "exec sleep 30"]);
    let mut terminal = PreparedTerminal::new(
        16001,
        crate::settings::BackendSettings {
            program: program.into(),
            args: args.into_iter().map(str::to_owned).collect(),
            #[cfg(windows)]
            escape_args: false,
            ..Default::default()
        },
    )
    .unwrap()
    .attach(Default::default(), Default::default());
    let mut state = TerminalViewState::new(terminal.id);
    for (key, modifiers) in [
        (
            Key::Character("V".into()),
            Modifiers::CTRL | Modifiers::SHIFT,
        ),
        (Key::Named(key::Named::Insert), Modifiers::SHIFT),
    ] {
        let event = KeyEvent::KeyPressed {
            key,
            modified_key: Key::Unidentified,
            physical_key: key::Physical::Unidentified(
                key::NativeCode::Unidentified,
            ),
            location: iced::keyboard::Location::Standard,
            modifiers,
            text: Some("v".into()), // Must never fall through to this text.
            repeat: false,
        };
        for data in [
            Some("synthetic\n界".into()),
            None,
            Some("x".repeat(PASTE_BYTE_LIMIT + 1)),
        ] {
            let mut clipboard = Clipboard(data.clone());
            let mut events = Vec::new();
            let before = terminal.input_status();
            let accepted_before =
                before.written_bytes + before.outstanding_bytes;
            let command = TerminalView { term: &terminal }
                .handle_keyboard_event(
                    &mut state,
                    &mut clipboard,
                    &event,
                    &mut iced_core::Shell::new(&mut events),
                )
                .unwrap();
            match (&data, &command) {
                (Some(text), Command::Paste(request)) => {
                    assert_eq!(text, "synthetic\n界");
                    assert_eq!(request.session_id(), terminal.id);
                    // No payload enters the writer before ordered handling.
                    assert!(!format!("{command:?}").contains("synthetic"));
                    assert_eq!(
                        terminal.paste(Some("must not bypass pending".into())),
                        crate::actions::Action::PasteRejected(
                            PasteError::Pending
                        )
                    );
                },
                (
                    None,
                    Command::PasteRejected(PasteError::ClipboardUnavailable),
                ) => {},
                (
                    Some(text),
                    Command::PasteRejected(PasteError::TooLarge {
                        bytes,
                        limit,
                    }),
                ) => {
                    assert_eq!(*bytes, text.len());
                    assert_eq!(*limit, PASTE_BYTE_LIMIT);
                },
                _ => panic!("wrong paste shortcut command"),
            }
            let queued = terminal.input_status();
            assert_eq!(
                accepted_before,
                queued.written_bytes + queued.outstanding_bytes
            );
            let expected = if matches!(command, Command::Paste(_)) {
                "synthetic\r界".len()
            } else {
                0
            };
            // Replaying a consumed request never writes the data again.
            terminal.handle(crate::Command::ProxyToBackend(command.clone()));
            terminal.handle(crate::Command::ProxyToBackend(command));
            let after = terminal.input_status();
            assert_eq!(
                accepted_before + expected,
                after.written_bytes + after.outstanding_bytes
            );
        }
    }
    assert_eq!(
        terminal.input_status().written_bytes
            + terminal.input_status().outstanding_bytes,
        2 * "synthetic\r界".len()
    );
    assert_eq!(
        terminal.paste(None),
        crate::actions::Action::PasteRejected(PasteError::ClipboardUnavailable)
    );
    let pending = terminal.backend.queue_paste(Some("cancel on close".into()));
    let Command::Paste(request) = &pending else {
        panic!("paste was not staged")
    };
    terminal.request_close();
    assert!(request.take(terminal.id).unwrap().is_none());
    assert_eq!(
        terminal.paste(Some("late".into())),
        crate::actions::Action::PasteRejected(crate::InputError::Closed.into())
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(5),
            terminal.shutdown_handle().wait(),
        )
        .await
        .unwrap();
    });
}
