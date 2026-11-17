//! Native Unix regression for the platform-independent launch result contract.
use super::*;
use crate::launch::LaunchLimits;
use crate::terminal::Entry;
use std::num::NonZeroU16;
use std::sync::mpsc;
use std::time::Instant;

pub(super) struct CleanupGate {
    pub(super) directory: std::path::PathBuf,
    // Closing the sole writer releases the child's FIFO read, including when
    // this test unwinds. The child never depends on terminal input for cleanup.
    pub(super) writer: Option<std::fs::File>,
}

impl CleanupGate {
    pub(super) fn new(id: SessionId) -> Self {
        let directory =
            std::env::temp_dir().join(format!("pike23-cancel-{}-{}", std::process::id(), id.get()));
        std::fs::create_dir(&directory).unwrap();
        let fifo = directory.join("release");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let writer = Some(
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(fifo)
                .unwrap(),
        );
        Self { directory, writer }
    }
}

impl Drop for CleanupGate {
    fn drop(&mut self) {
        self.writer.take();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn cancellation_during_native_preparation_retains_closing_until_disposal() {
    cancel_during_preparation(false);
}

#[test]
fn window_close_during_native_preparation_accounts_for_late_cleanup() {
    cancel_during_preparation(true);
}

fn cancel_during_preparation(window: bool) {
    let executor = SharedExecutor::new().unwrap();
    let one = NonZeroU16::new(1).unwrap();
    let mut app = App {
        model: Model::new(one),
        launches: LaunchPool::new(LaunchLimits {
            jobs: one,
            sessions: one,
        }),
        profile: waiting(),
        ..App::default()
    };
    let (_, Effect::Spawn(ids)) = app.model.create_tab().unwrap() else {
        unreachable!()
    };
    let token = app.model.claim_launch(ids).unwrap();
    let mut cleanup = CleanupGate::new(ids.session);
    let marker = cleanup.directory.join("ready");
    let fifo = cleanup.directory.join("release");
    let child_marker = marker.clone();
    let (created, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let request = executor
        .enter(|| {
            app.launches.start_reserved_with(token, move |id| {
                let prepared = iced_term::PreparedTerminal::new(
                    id.get(),
                    BackendSettings {
                        program: "/bin/sh".into(),
                        args: vec![
                            "-c".into(),
                            // Hold real disposal until the test closes its FIFO
                            // writer. Readiness proves the reader is already open
                            // and HUP ignored, so timing cannot release the gate.
                            "exec 3< \"$2\"; trap '' HUP; echo ready > \"$1\"; read pike_release <&3"
                                .into(),
                            "pike23-cancel".into(),
                            child_marker.to_string_lossy().into_owned(),
                            fifo.to_string_lossy().into_owned(),
                        ],
                        ..Default::default()
                    },
                )?;
                created.send(prepared.shutdown_handle()).unwrap();
                gate.recv_timeout(DEADLINE).unwrap();
                Ok(prepared)
            })
        })
        .unwrap();
    app.sessions.entries.insert(
        ids.session,
        Entry::new(ids, Owner::Starting(request.cancellation())),
    );
    let handle = observed.recv_timeout(DEADLINE).unwrap();
    let started = Instant::now();
    while !marker.exists() {
        assert!(started.elapsed() < DEADLINE);
        std::thread::sleep(Duration::from_millis(5)); // Fixture readiness only.
    }
    drop(app.update(if window {
        Message::CloseWindow
    } else {
        Message::Control(Control::CloseSession(ids.session))
    }));
    assert_eq!(app.close_confirmation().unwrap().1, 1);
    assert_eq!(
        app.model.sessions().get(ids.session).unwrap().lifecycle(),
        Lifecycle::Starting
    );
    confirm_close(&mut app);
    assert_eq!(handle.completion(), None);
    assert_eq!(app.model.sessions().len(), 1);
    release.send(()).unwrap();

    let mut receive = Box::pin(request.receive());
    let early = executor
        .block_on(async { tokio::time::timeout(Duration::from_millis(100), &mut receive).await });
    let received_early = early.is_ok();
    let mut completion_at_receipt = None;
    if let Ok(result) = early {
        completion_at_receipt = handle.completion();
        drop(app.launched(ids, result));
    }
    // Exercise the actual host while native cleanup is held by the child.
    // A mismatched failure or premature disposal receipt cannot release it.
    let other = Reservation::allocate().unwrap();
    drop(app.launched(
        Reservation {
            request: other.request,
            ..ids
        },
        Err(LaunchError::Canceled),
    ));
    drop(app.update(Message::Disposed(ids)));
    let retained = app.model.sessions().get(ids.session).map(|e| e.lifecycle());
    let owners = app.sessions.entries.len();
    let unavailable = executor.enter(|| app.begin_open()).is_none();
    let capacity_held = if window {
        !app.launches.shutdown_complete()
            && matches!(
                executor.enter(|| app.launches.start(waiting())),
                Err(LaunchError::Closed)
            )
    } else {
        matches!(
            executor.enter(|| app.launches.start(waiting())),
            Err(LaunchError::Busy)
        )
    };
    let pending_before_release = handle.completion().is_none();
    cleanup.writer.take();

    if !received_early {
        let result = executor
            .block_on(async { tokio::time::timeout(DEADLINE, &mut receive).await.unwrap() });
        completion_at_receipt = handle.completion();
        assert!(matches!(result, Err(LaunchError::Canceled)));
        drop(app.launched(ids, result));
    }
    // Drain before reporting an invariant failure, preserving native ownership
    // even on the unfixed implementation used for this regression's red run.
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap();
    });
    assert!(
        !received_early,
        "cancellation was delivered before disposal"
    );
    assert!(pending_before_release);
    assert!(completion_at_receipt.is_some());
    assert_eq!(
        retained,
        Some(Lifecycle::Closing {
            awaiting_spawn: true
        })
    );
    assert_eq!(owners, 1);
    assert!(unavailable, "Closing must consume the sole model slot");
    assert!(capacity_held);
    assert!(app.model.sessions().is_empty());
    assert!(app.sessions.entries.is_empty());

    if window {
        executor.block_on(async {
            tokio::time::timeout(DEADLINE, app.launches.shutdown())
                .await
                .unwrap();
        });
        assert!(app.update(Message::ShutdownComplete).units() > 0);
        return;
    }

    // Replayed late receipts acknowledge nothing twice; retry owns fresh IDs.
    drop(app.launched(ids, Err(LaunchError::Canceled)));
    drop(app.update(Message::Disposed(ids)));
    let (next, ready) = launch(&mut app, &executor);
    assert_ne!(next.session, ids.session);
    assert_ne!(next.request, ids.request);
    drop(app.launched(next, Ok(ready)));
    drop(app.launched(ids, Err(LaunchError::Canceled)));
    drop(app.update(Message::Disposed(ids)));
    assert_eq!(app.active_session(), Some(next.session));
    assert_eq!(app.model.sessions().len(), 1);
    assert_eq!(app.sessions.entries.len(), 1);
    assert!(app.terminal(next.session).is_some());
    shutdown(&mut app, &executor);
    drop(app.update(Message::ShutdownComplete));
    assert!(app.model.sessions().is_empty());
    assert!(app.sessions.entries.is_empty());
}
