use crate::*;
use iced::advanced::subscription::into_recipes;
use iced::futures::{stream, FutureExt, StreamExt};
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(5);

#[test]
fn hidden_font_change_updates_geometry_without_building_a_viewport() {
    let runtime = runtime();
    let mut terminal = terminal(11002, false);
    terminal.handle(Command::ProxyToBackend(BackendCommand::Resize(
        Some(iced_core::Size::new(960.0, 540.0)),
        None,
    )));
    terminal.set_visible(false);
    let before = terminal.snapshot_stats();
    terminal.handle(Command::ChangeFont(settings::FontSettings {
        size: 28.0,
        ..Default::default()
    }));
    let after = terminal.snapshot_stats();
    assert_eq!(after.snapshots, before.snapshots);
    assert_eq!(after.viewport_cells, 0);
    assert!(after.columns < before.columns);
    assert!(after.screen_lines < before.screen_lines);
    terminal.set_visible(true);
    assert_eq!(terminal.snapshot_stats().snapshots, before.snapshots + 1);
    let handle = terminal.shutdown_handle();
    drop(terminal);
    runtime.block_on(async {
        tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap();
    });
}

#[test]
fn hidden_native_output_keeps_history_and_reactivates_the_exited_buffer_once() {
    let runtime = runtime();
    let mut terminal = prepare_terminal(11001, true)
        .attach_hidden(Default::default(), Default::default());
    let before = terminal.snapshot_stats().snapshots;
    assert_eq!(before, 0);
    let recipe = into_recipes(terminal.subscription()).pop().unwrap();
    let mut events = recipe.stream(stream::empty().boxed());
    runtime.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(Event::BackendCall(_, command)) = events.next().await
            {
                terminal.handle(Command::ProxyToBackend(command));
            }
        })
        .await
        .unwrap();
    });
    terminal.refresh();
    assert_eq!(terminal.snapshot_stats().snapshots, before);
    assert_eq!(terminal.snapshot_stats().history_lines, 2000);
    assert!(terminal.final_content().is_none());
    terminal.set_visible(true);
    assert_eq!(terminal.snapshot_stats().snapshots, before + 1);
    let old_rows = terminal.snapshot_stats().screen_lines;
    // First real pane layout arrives after this hidden session has exited.
    // Reflow the retained model, never a disposed PTY, so its newest text can
    // actually fit the visible pane instead of staying below its bottom edge.
    terminal.handle(Command::ProxyToBackend(BackendCommand::Resize(
        Some(iced_core::Size::new(960.0, 540.0)),
        None,
    )));
    assert!(terminal.snapshot_stats().screen_lines < old_rows);
    assert!(terminal.input_status().closed);
    let text: String = terminal
        .renderable_content()
        .unwrap()
        .viewport
        .display_iter()
        .map(|cell| cell.c)
        .collect();
    assert!(text.contains("PIKE_FINAL_7"));
    assert!(
        terminal
            .shutdown_handle()
            .completion()
            .unwrap()
            .output_drained
    );
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
}

fn terminal(id: u64, natural: bool) -> Terminal {
    prepare_terminal(id, natural).attach(Default::default(), Default::default())
}

fn prepare_terminal(id: u64, natural: bool) -> PreparedTerminal {
    #[cfg(windows)]
    let (program, args) = if natural {
        ("cmd.exe", vec!["/D", "/Q", "/C", "(for /L %i in (1,1,3000) do @echo 012345678901234567890123456789012345678901234567890123456789) & echo PIKE_FINAL_7 & exit /b 37"])
    } else {
        (
            "cmd.exe",
            vec!["/D", "/Q", "/C", "for /L %i in (1,0,2) do @echo synthetic"],
        )
    };
    #[cfg(not(windows))]
    let (program, args) = if natural {
        ("/bin/sh", vec!["-c", "i=0; while [ $i -lt 3000 ]; do printf '012345678901234567890123456789012345678901234567890123456789\n'; i=$((i+1)); done; printf '\r\nPIKE_FINAL_7\r\n'; exit 37"])
    } else {
        (
            "/bin/sh",
            vec!["-c", "while :; do printf 'synthetic\n'; done"],
        )
    };
    PreparedTerminal::new(
        id,
        settings::BackendSettings {
            #[cfg(windows)]
            escape_args: false,
            program: program.into(),
            args: args.into_iter().map(str::to_owned).collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn natural_exit_drains_multiple_chunks_and_retains_the_final_read_only_model() {
    let runtime = runtime();
    let mut terminal = terminal(7001, true);
    let handle = terminal.shutdown_handle();
    let start = Instant::now();
    let result = runtime.block_on(async {
        tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap()
    });
    eprintln!("natural completion: {:?}", start.elapsed());
    assert_eq!(result.reason, ExitReason::ChildExit);
    assert_eq!(result.exit_code, Some(37));
    assert!(result.output_drained);
    let final_text: String = terminal
        .final_content()
        .unwrap()
        .viewport
        .display_iter()
        .map(|cell| cell.c)
        .collect();
    assert!(final_text.contains("PIKE_FINAL_7"));
    // Undersized resize/input after completion cannot erase the retained grid.
    terminal.backend.handle(BackendCommand::Resize(
        Some(iced_core::Size::new(1.0, 1.0)),
        None,
    ));
    assert_eq!(
        terminal
            .backend
            .handle(BackendCommand::Write(b"late".to_vec())),
        actions::Action::InputRejected(InputError::Closed)
    );
    let after: String = terminal
        .final_content()
        .unwrap()
        .viewport
        .display_iter()
        .map(|cell| cell.c)
        .collect();
    assert_eq!(after, final_text);
    assert_eq!(runtime.block_on(handle.wait()), result);
}

#[test]
fn repeated_close_during_output_has_one_backend_result_and_fused_stream() {
    let runtime = runtime();
    let terminal = terminal(7002, false);
    let handle = terminal.shutdown_handle();
    let other = handle.clone();
    assert!(handle.request_close());
    for _ in 0..100 {
        assert!(!other.request_close());
    }
    let result = runtime.block_on(async {
        tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap()
    });
    assert_eq!(result.reason, ExitReason::Requested);
    let recipe = into_recipes(terminal.subscription()).pop().unwrap();
    let mut events = recipe.stream(stream::empty().boxed());
    runtime.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            let (mut source, mut backend) = (0, 0);
            while let Some(Event::BackendCall(id, command)) =
                events.next().await
            {
                assert_eq!(id, 7002);
                match command {
                    BackendCommand::EventBridge(
                        EventBridgeEvent::Completed,
                    ) => source += 1,
                    BackendCommand::EventBridge(
                        EventBridgeEvent::BackendCompleted(value),
                    ) => {
                        assert_eq!(source, 1);
                        assert_eq!(value, result);
                        backend += 1;
                    },
                    BackendCommand::EventBridge(EventBridgeEvent::Error(
                        error,
                    )) => panic!("{error:?}"),
                    _ => (),
                }
            }
            assert_eq!((source, backend), (1, 1));
            assert!(events.next().await.is_none());
        })
        .await
        .unwrap();
    });
}

#[test]
fn canceled_subscription_and_waiter_do_not_cancel_cleanup_of_dropped_owner() {
    let runtime = runtime();
    let terminal = terminal(7003, false);
    let handle = terminal.shutdown_handle();
    let recipe = into_recipes(terminal.subscription()).pop().unwrap();
    let mut events = recipe.stream(stream::empty().boxed());
    // Dropping a subscription permits later resubscription; it is not session
    // ownership. Dropping the Terminal requests cleanup even if a recipe survives.
    drop(events.next().now_or_never());
    drop(events);
    let mut wait = Box::pin(handle.wait());
    assert!(wait.as_mut().now_or_never().is_none());
    drop(wait);
    drop(terminal);
    let result = runtime.block_on(async {
        tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap()
    });
    assert_eq!(result.reason, ExitReason::Requested);
    assert_eq!(handle.completion(), Some(result));
}

#[test]
fn canceled_startup_result_disposes_an_unobserved_session() {
    // The synchronous constructor is unchanged (#10). Model a completed
    // constructor whose caller canceled before consuming its result.
    let runtime = runtime();
    for id in 7010..7020 {
        let terminal = terminal(id, false);
        let handle = terminal.shutdown_handle();
        drop(terminal);
        let result = runtime.block_on(async {
            tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap()
        });
        assert_eq!(result.reason, ExitReason::Requested);
    }
}

fn prepared(id: u64) -> PreparedTerminal {
    #[cfg(windows)]
    let (program, args) =
        ("cmd.exe", vec!["/D", "/Q", "/C", "set /p PIKE_WAIT="]);
    #[cfg(not(windows))]
    let (program, args) = ("/bin/sh", vec!["-c", "read pike_wait"]);
    PreparedTerminal::new(
        id,
        settings::BackendSettings {
            program: program.into(),
            args: args.into_iter().map(str::to_owned).collect(),
            #[cfg(windows)]
            escape_args: false,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn prepared_backend_moves_from_worker_and_attaches_on_ui_thread() {
    fn assert_send<T: Send>() {}
    assert_send::<PreparedTerminal>();
    let prepared = std::thread::spawn(|| prepared(7010)).join().unwrap();
    let handle = prepared.shutdown_handle();
    let terminal = prepared.attach(Default::default(), Default::default());
    assert_eq!(terminal.id, 7010);
    drop(terminal);
    let result = runtime().block_on(async {
        tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap()
    });
    assert_eq!(result.reason, ExitReason::Requested);
}

#[test]
fn dropping_prepared_backend_without_font_attachment_acknowledges_cleanup() {
    let handle = std::thread::spawn(|| {
        let prepared = prepared(7011);
        let handle = prepared.shutdown_handle();
        drop(prepared);
        handle
    })
    .join()
    .unwrap();
    let result = runtime().block_on(async {
        tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap()
    });
    assert_eq!(result.reason, ExitReason::Requested);
}
