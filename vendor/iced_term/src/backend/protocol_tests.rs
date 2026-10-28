use super::*;
use crate::event_bridge::{self, Delivery, Receiver};
use crate::settings::ThemeSettings;
use iced::futures::FutureExt;
use std::sync::Mutex;
use std::time::Duration;

struct Session {
    backend: Backend,
    receiver: Arc<Receiver>,
}

impl Session {
    fn new() -> Self {
        let (sender, receiver) = event_bridge::channel();
        #[cfg(windows)]
        let (program, args) =
            ("cmd.exe", vec!["/D", "/Q", "/C", "set /p PIKE_WAIT="]);
        #[cfg(not(windows))]
        let (program, args) = ("/bin/sh", vec!["-c", "read pike_wait"]);
        let backend = Backend::new(
            20001,
            sender,
            BackendSettings {
                program: program.into(),
                args: args.into_iter().map(str::to_owned).collect(),
                #[cfg(windows)]
                escape_args: false,
                ..Default::default()
            },
        )
        .unwrap();
        Self { backend, receiver }
    }

    fn parse(&mut self, bytes: &[u8]) {
        let mut processor: alacritty_terminal::vte::ansi::Processor =
            Default::default();
        processor.advance(&mut *self.backend.term.lock(), bytes);
    }

    fn accepted(&self) -> usize {
        let status = self.backend.input_status();
        status.outstanding_bytes + status.written_bytes + status.cancelled_bytes
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.backend.request_shutdown();
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                tokio::time::timeout(
                    Duration::from_secs(5),
                    self.backend.shutdown_handle().wait(),
                )
                .await
                .unwrap();
            });
    }
}

#[test]
fn vt_queries_use_ordered_model_overrides_and_current_palette_and_dimensions() {
    let mut session = Session::new();
    session.backend.theme =
        Theme::new(ThemeSettings::new(Box::new(crate::ColorPalette {
            foreground: "#123456".into(),
            background: "#654321".into(),
            red: "#112233".into(),
            ..Default::default()
        })));
    session.backend.handle(Command::Resize(
        Some(Size::new(90.0, 70.0)),
        Some(Size::new(2.0, 5.0)),
    ));
    let before = session.backend.snapshot_stats().snapshots;
    session.parse(b"\x1b[18t\x1b[14t\x1b]10;?\x07\x1b]11;?\x07\x1b]12;?\x07\x1b]4;1;?\x07\x1b]4;196;?\x07\x1b]10;#abcdef\x07\x1b]10;?\x07\x1b]110\x07\x1b]10;?\x07");
    // The parser has already reset the override before any UI processing.
    assert!(session.backend.term.lock().colors()[256].is_none());
    let replies = Arc::new(Mutex::new(Vec::new()));
    while let Some(Some(Delivery::Terminal(event))) =
        session.receiver.recv().now_or_never()
    {
        let recorded = replies.clone();
        let event = match event {
            Event::PtyWrite(reply) => {
                recorded.lock().unwrap().push(reply);
                Event::PtyWrite(String::new())
            },
            Event::ColorRequest(index, format) => Event::ColorRequest(
                index,
                Arc::new(move |color| {
                    recorded.lock().unwrap().push(format(color));
                    String::new()
                }),
            ),
            Event::TextAreaSizeRequest(format) => {
                Event::TextAreaSizeRequest(Arc::new(move |size| {
                    recorded.lock().unwrap().push(format(size));
                    String::new()
                }))
            },
            other => other,
        };
        session
            .backend
            .handle(Command::ProcessAlacrittyEvent(event));
    }
    assert_eq!(
        *replies.lock().unwrap(),
        [
            "\x1b[8;14;45t",
            "\x1b[4;70;90t",
            "\x1b]10;rgb:1212/3434/5656\x07",
            "\x1b]11;rgb:6565/4343/2121\x07",
            "\x1b]12;rgb:1212/3434/5656\x07",
            "\x1b]4;1;rgb:1111/2222/3333\x07",
            "\x1b]4;196;rgb:ffff/0000/0000\x07",
            "\x1b]10;rgb:abab/cdcd/efef\x07",
            "\x1b]10;rgb:1212/3434/5656\x07"
        ]
    );
    assert_eq!(session.backend.snapshot_stats().snapshots, before);
    assert_eq!(session.accepted(), 0);
}

#[test]
fn pixel_queries_round_fractional_grid_area_once_without_native_resize_work() {
    let mut session = Session::new();
    session.backend.handle(Command::Resize(
        Some(Size::new(680.0, 320.0)),
        Some(Size::new(8.5, 16.0)),
    ));
    let before = session.backend.resize_counts();
    assert_eq!(
        session.backend.text_area_size(),
        alacritty_terminal::event::TextAreaSize {
            width: 680,
            height: 320
        }
    );
    assert_eq!(
        session.backend.last_pty_size.cell_width, 9,
        "native cell metadata remains independent"
    );
    session.parse(b"\x1b[14t");
    let mut reply = None;
    while let Some(Some(Delivery::Terminal(event))) =
        session.receiver.recv().now_or_never()
    {
        if let Event::TextAreaSizeRequest(format) = event {
            reply = Some(format(session.backend.text_area_size()));
        }
    }
    assert_eq!(reply.as_deref(), Some("\x1b[4;320;680t"));
    assert_eq!(session.backend.resize_counts(), before);
    session.backend.handle(Command::Resize(
        Some(Size::new(690.0, 330.0)),
        Some(Size::new(8.625, 16.5)),
    ));
    assert_eq!(
        session.backend.text_area_size(),
        alacritty_terminal::event::TextAreaSize {
            width: 690,
            height: 330
        }
    );
}

#[test]
fn focus_reports_require_mode_and_real_effective_transitions() {
    let mut session = Session::new();
    session.backend.window_focus.set_focused(true);
    session.backend.set_visible(true);
    session.backend.handle(Command::FocusChanged(true));
    assert_eq!(session.accepted(), 0);
    session.parse(b"\x1b[?1004h");
    session.backend.handle(Command::FocusChanged(true));
    assert_eq!(
        session.accepted(),
        0,
        "enabling mode is not a focus transition"
    );
    session.backend.handle(Command::FocusChanged(false));
    session.backend.handle(Command::FocusChanged(false));
    assert_eq!(session.accepted(), 3);
    session.backend.handle(Command::FocusChanged(true));
    assert_eq!(session.accepted(), 6);
    session.parse(b"\x1b[?1004l");
    session.backend.handle(Command::FocusChanged(false));
    assert_eq!(session.accepted(), 6);
    session.parse(b"\x1b[?1004h");
    session.backend.window_focus.set_focused(false);
    session.backend.handle(Command::FocusChanged(true));
    assert_eq!(
        session.accepted(),
        6,
        "a queued true focus command cannot override actual window blur"
    );
    session.backend.window_focus.set_focused(true);
    session.backend.set_visible(false);
    session.backend.handle(Command::FocusChanged(true));
    assert!(!session.backend.is_focused());
    assert_eq!(session.accepted(), 6, "hidden session cannot gain focus");
}

#[test]
fn hidden_metadata_and_exit_status_are_explicit_without_viewport_work() {
    let mut session = Session::new();
    assert_eq!(
        session
            .backend
            .handle(Command::ProcessAlacrittyEvent(Event::Exit)),
        Action::Shutdown { exit_code: None }
    );
    let before = session.backend.snapshot_stats().snapshots;
    for (event, expected) in [
        (
            Event::Title("safe\n\u{202e}title".into()),
            Action::ChangeTitle("safetitle".into()),
        ),
        (Event::ResetTitle, Action::ResetTitle),
        (Event::Bell, Action::Bell),
        (Event::MouseCursorDirty, Action::MouseCursorDirty),
        (
            Event::CursorBlinkingChange,
            Action::CursorBlinkingChanged(false),
        ),
        (Event::ChildExit(37), Action::ChildExit(37)),
        (
            Event::Exit,
            Action::Shutdown {
                exit_code: Some(37),
            },
        ),
    ] {
        assert_eq!(
            session
                .backend
                .handle(Command::ProcessAlacrittyEvent(event)),
            expected
        );
    }
    assert_eq!(session.backend.exit_code(), Some(37));
    assert_eq!(session.backend.snapshot_stats().snapshots, before);
    assert_eq!(session.backend.snapshot_stats().viewport_cells, 0);
}

#[test]
fn vt_reset_emits_default_title_even_after_a_queued_title_change() {
    let mut session = Session::new();
    session.parse(b"\x1b]2;synthetic\x07\x1bc");
    let mut reset = false;
    while let Some(Some(Delivery::Terminal(event))) =
        session.receiver.recv().now_or_never()
    {
        if let Event::ResetTitle = event {
            reset = true;
        }
        assert!(!matches!(event, Event::Title(_)));
    }
    assert!(
        reset,
        "RIS must reset the displayed title as well as model metadata"
    );
}

#[test]
fn osc52_stays_disabled_after_history_changes_and_invalid_queries_do_not_call_formatters(
) {
    let mut session = Session::new();
    for change in [false, true] {
        if change {
            session
                .backend
                .handle(Command::SetHistoryLimit(0.try_into().unwrap()));
        }
        session.parse(b"\x1b]52;c;c3ludGhldGlj\x07\x1b]52;c;?\x07");
        while let Some(Some(delivery)) = session.receiver.recv().now_or_never()
        {
            assert!(!matches!(
                delivery,
                Delivery::Terminal(
                    Event::ClipboardStore(..) | Event::ClipboardLoad(..)
                )
            ));
        }
    }
    session.backend.handle(Command::ProcessAlacrittyEvent(
        Event::ClipboardLoad(
            term::ClipboardType::Clipboard,
            Arc::new(|_| panic!("disabled clipboard formatter")),
        ),
    ));
    session.backend.handle(Command::ProcessAlacrittyEvent(
        Event::ColorRequest(
            usize::MAX,
            Arc::new(|_| panic!("invalid color formatter")),
        ),
    ));
    assert_eq!(session.accepted(), 0);
}

#[test]
fn rejected_protocol_reply_closes_only_its_origin_and_retains_completion() {
    let mut session = Session::new();
    let other = Session::new();
    let action = session.backend.handle(Command::ProcessAlacrittyEvent(
        Event::PtyWrite("x".repeat(crate::INPUT_BYTE_LIMIT + 1)),
    ));
    assert!(matches!(
        action,
        Action::InputRejected(crate::InputError::Full { .. })
    ));
    assert!(session.backend.input_status().closed);
    assert!(!other.backend.input_status().closed);
}
