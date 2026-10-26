use super::*;
type Parser = alacritty_terminal::vte::ansi::Processor;
use std::time::Duration;

fn backend() -> Backend {
    #[cfg(windows)]
    let (program, args) =
        ("cmd.exe", vec!["/D", "/Q", "/C", "set /p PIKE_WAIT="]);
    #[cfg(not(windows))]
    let (program, args) = ("/bin/sh", vec!["-c", "read pike_wait"]);
    let (sender, _receiver) = crate::event_bridge::channel();
    Backend::new(
        11000,
        sender,
        BackendSettings {
            program: program.into(),
            args: args.into_iter().map(str::to_owned).collect(),
            #[cfg(windows)]
            escape_args: false,
            ..Default::default()
        },
    )
    .unwrap()
}

fn finish(backend: Backend) {
    let handle = backend.shutdown_handle();
    drop(backend);
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(5), handle.wait())
                .await
                .unwrap();
        });
}

fn flood(backend: &Backend) {
    let mut term = backend.term.lock();
    let mut parser = Parser::new();
    for _ in 0..4500 {
        parser.advance(&mut *term, b"synthetic history\r\n");
    }
}

#[test]
fn hidden_flood_and_explicit_refresh_do_not_create_snapshots() {
    let mut backend = backend();
    assert_eq!(backend.snapshot_stats().snapshots, 0);
    flood(&backend);
    for _ in 0..100 {
        backend.sync();
    }
    assert_eq!(backend.snapshot_stats().snapshots, 0);
    assert_eq!(backend.snapshot_stats().history_lines, 2000);
    backend.set_visible(true);
    let stats = backend.snapshot_stats();
    assert_eq!(stats.snapshots, 1);
    assert_eq!(stats.viewport_cells, stats.columns * stats.screen_lines);
    backend.set_visible(true);
    assert_eq!(backend.snapshot_stats().snapshots, 1);
    backend.set_visible(false);
    assert_eq!(backend.snapshot_stats().viewport_cells, 0);
    flood(&backend);
    backend.sync();
    backend.set_visible(true);
    assert_eq!(backend.snapshot_stats().snapshots, 2);
    finish(backend);
}

#[test]
fn history_reduction_clamps_scrollback_and_copy_uses_authoritative_selection() {
    let mut backend = backend();
    flood(&backend);
    {
        let mut term = backend.term.lock();
        term.scroll_display(Scroll::Top);
        let top = term.topmost_line();
        let mut selection = Selection::new(
            SelectionType::Lines,
            Point::new(top, Column(0)),
            Side::Left,
        );
        selection.update(Point::new(top + 100, Column(0)), Side::Right);
        term.selection = Some(selection);
    }
    // No visible snapshot exists, but multi-page copy remains correct.
    let copied = backend.selectable_content();
    assert!(copied.matches("synthetic history").count() >= 100);
    backend.handle(Command::SetHistoryLimit(
        HistoryLimit::try_from(100).unwrap(),
    ));
    assert_eq!(backend.snapshot_stats().history_lines, 100);
    assert!(backend.term.lock().grid().display_offset() <= 100);
    backend
        .handle(Command::SetHistoryLimit(HistoryLimit::try_from(0).unwrap()));
    assert_eq!(backend.snapshot_stats().history_lines, 0);
    assert!(backend.selectable_content().is_empty());
    flood(&backend);
    assert_eq!(backend.snapshot_stats().history_lines, 0);
    finish(backend);
}

#[test]
fn changing_history_in_alternate_screen_updates_only_the_normal_buffer() {
    let mut backend = backend();
    flood(&backend);
    backend.term.lock().swap_alt();
    flood(&backend);
    assert_eq!(backend.snapshot_stats().history_lines, 0);
    backend.handle(Command::SetHistoryLimit(
        HistoryLimit::try_from(10).unwrap(),
    ));
    backend.term.lock().swap_alt();
    assert_eq!(backend.snapshot_stats().history_lines, 10);
    backend.term.lock().swap_alt();
    backend
        .handle(Command::SetHistoryLimit(HistoryLimit::try_from(0).unwrap()));
    backend.term.lock().swap_alt();
    assert_eq!(backend.snapshot_stats().history_lines, 0);
    finish(backend);
}

#[test]
fn activation_applies_layout_deferred_during_shutdown_even_after_bridge_failure(
) {
    for failed in [false, true] {
        let mut backend = backend();
        let actual_shutdown = backend.shutdown_handle();
        // Hold the UI's completion observation pending deterministically, while
        // still closing and joining the real synthetic ConPTY backend below.
        let (completed, completion) = tokio::sync::watch::channel(None);
        let (events, _receiver) = crate::event_bridge::channel();
        backend.shutdown = crate::ShutdownHandle::new(
            backend.notifier.0.clone(),
            events,
            completion,
        );
        if failed {
            backend.handle(Command::EventBridge(EventBridgeEvent::Error(
                crate::EventBridgeError::ControlOverflow,
            )));
        } else {
            backend.handle(Command::RequestShutdown);
        }
        assert!(backend.input_status().closed);
        assert_eq!(
            backend.handle(Command::Write(b"rejected".to_vec())),
            Action::InputRejected(crate::InputError::Closed)
        );
        backend.handle(Command::Resize(
            Some(Size::new(600.0, 200.0)),
            Some(Size::new(10.0, 20.0)),
        ));
        assert_eq!(backend.snapshot_stats().columns, 80);
        assert_eq!(backend.snapshot_stats().screen_lines, 50);
        assert_eq!(backend.snapshot_stats().snapshots, 0);
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                tokio::time::timeout(
                    Duration::from_secs(5),
                    actual_shutdown.wait(),
                )
                .await
                .unwrap()
            });
        completed.send_replace(Some(result));
        backend.sync();
        assert_eq!(backend.snapshot_stats().snapshots, 0);
        backend.set_visible(true);
        let stats = backend.snapshot_stats();
        assert_eq!(
            (stats.columns, stats.screen_lines),
            (60, 10),
            "bridge failure: {failed}"
        );
        assert_eq!(stats.viewport_cells, 600);
        assert_eq!(stats.snapshots, 1);
        finish(backend);
    }
}
