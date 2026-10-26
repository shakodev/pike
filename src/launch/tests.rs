use super::*;
use crate::executor::SharedExecutor;
use iced::Executor;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Duration;

const DEADLINE: Duration = Duration::from_secs(10);

#[path = "reserved_tests.rs"]
mod reserved;

fn limits(jobs: u16, sessions: u16) -> LaunchLimits {
    LaunchLimits {
        jobs: NonZeroU16::new(jobs).unwrap(),
        sessions: NonZeroU16::new(sessions).unwrap(),
    }
}

#[test]
fn slow_failure_keeps_executor_responsive_and_rejects_30_of_32_requests() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::new(limits(2, 16));
    let mut releases = Vec::new();
    let mut requests = Vec::new();
    let (entered, entries) = mpsc::channel();
    for _ in 0..32 {
        let (release, gate) = mpsc::channel();
        let entered = entered.clone();
        let launch = executor.enter(|| {
            pool.start_with(move |_| {
                entered.send(std::thread::current().id()).unwrap();
                gate.recv_timeout(DEADLINE).unwrap();
                Err(io::Error::other("private synthetic credentials"))
            })
        });
        if requests.len() < 2 {
            releases.push(release);
            requests.push(launch.unwrap());
        } else {
            assert!(matches!(launch, Err(LaunchError::Busy)));
        }
    }
    let ui_thread = std::thread::current().id();
    for _ in 0..2 {
        assert_ne!(entries.recv_timeout(DEADLINE).unwrap(), ui_thread);
    }
    // The shared async worker and the caller remain available with both launch
    // workers blocked; no sleep-based ordering or timing threshold is required.
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, tokio::spawn(async { 42 }))
            .await
            .unwrap()
            .unwrap();
    });
    for release in releases {
        release.send(()).unwrap();
    }
    for request in requests {
        let error = executor.block_on(request.receive()).unwrap_err();
        assert_eq!(error, LaunchError::Spawn(io::ErrorKind::Other));
        assert!(!format!("{error:?}").contains("private"));
    }
    executor.block_on(pool.shutdown());
}

#[test]
fn canceled_running_job_keeps_capacity_until_worker_returns() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::new(limits(1, 16));
    let (entered, entries) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let request = executor
        .enter(|| {
            pool.start_with(move |_| {
                entered.send(()).unwrap();
                gate.recv_timeout(DEADLINE).unwrap();
                Err(io::Error::other("synthetic"))
            })
        })
        .unwrap();
    entries.recv_timeout(DEADLINE).unwrap();
    drop(request);
    assert!(matches!(
        executor.enter(|| pool.start(Default::default())),
        Err(LaunchError::Busy)
    ));
    release.send(()).unwrap();
    executor.block_on(pool.shutdown());
    assert!(matches!(
        executor.enter(|| pool.start(Default::default())),
        Err(LaunchError::Closed)
    ));
}

#[test]
fn canceled_before_blocking_work_starts_never_calls_prepare() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::default();
    let mut releases = Vec::new();
    let (entered, entries) = mpsc::channel();
    for _ in 0..2 {
        let (release, gate) = mpsc::channel();
        releases.push(release);
        let entered = entered.clone();
        executor.enter(|| {
            drop(tokio::task::spawn_blocking(move || {
                entered.send(()).unwrap();
                gate.recv_timeout(DEADLINE).unwrap();
            }));
        });
    }
    for _ in 0..2 {
        entries.recv_timeout(DEADLINE).unwrap();
    }
    let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = called.clone();
    let request = executor
        .enter(|| {
            pool.start_with(move |_| {
                observed.store(true, Ordering::SeqCst);
                Err(io::Error::other("synthetic"))
            })
        })
        .unwrap();
    drop(request);
    for release in releases {
        release.send(()).unwrap();
    }
    executor.block_on(pool.shutdown());
    assert!(!called.load(Ordering::SeqCst));
}

#[test]
fn retry_uses_fresh_request_and_session_ids_after_failure() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::default();
    let first = executor
        .enter(|| pool.start_with(|_| Err(io::Error::other("failure"))))
        .unwrap();
    let ids = (first.request, first.session);
    assert!(executor.block_on(first.receive()).is_err());
    // Wait for full disposal before reusing a one-slot budget in other callers.
    let second = executor
        .enter(|| pool.start_with(|_| Err(io::Error::other("failure"))))
        .unwrap();
    assert_ne!(second.request, ids.0);
    assert_ne!(second.session, ids.1);
    assert!(executor.block_on(second.receive()).is_err());
    executor.block_on(pool.shutdown());
}

#[test]
fn launch_and_pure_state_share_the_process_identity_allocator() {
    let mut model = crate::workspace::Model::default();
    let (_, crate::session::Effect::Spawn(before)) = model.create_tab().unwrap() else {
        panic!("expected reservation");
    };
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::default();
    let launch = executor
        .enter(|| pool.start_with(|_| Err(io::Error::other("synthetic failure"))))
        .unwrap();
    let session: crate::identity::SessionId = launch.session;
    let request: crate::identity::RequestId = launch.request;
    assert!(session.get() > before.session.get());
    assert!(request.get() > before.request.get());
    assert!(executor.block_on(launch.receive()).is_err());
    executor.block_on(pool.shutdown());
    let (_, crate::session::Effect::Spawn(after)) = model.create_tab().unwrap() else {
        panic!("expected reservation");
    };
    assert!(after.session.get() > session.get());
    assert!(after.request.get() > request.get());
}

#[test]
fn panicked_launch_reports_failure_and_drains_its_permits() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::default();
    let request = executor
        .enter(|| pool.start_with(|_| panic!("synthetic worker failure")))
        .unwrap();
    assert_eq!(
        executor.block_on(request.receive()).unwrap_err(),
        LaunchError::WorkerFailed
    );
    executor.block_on(pool.shutdown());
}

#[test]
fn session_limit_is_independent_of_job_budget() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::new(limits(2, 1));
    let (release, gate) = mpsc::channel();
    let request = executor
        .enter(|| {
            pool.start_with(move |_| {
                gate.recv_timeout(DEADLINE).unwrap();
                Err(io::Error::other("synthetic"))
            })
        })
        .unwrap();
    assert!(matches!(
        executor.enter(|| pool.start(Default::default())),
        Err(LaunchError::SessionLimit)
    ));
    drop(request);
    release.send(()).unwrap();
    executor.block_on(pool.shutdown());
}

fn fixture_settings() -> BackendSettings {
    #[cfg(windows)]
    let (program, args) = ("cmd.exe", vec!["/D", "/Q", "/C", "set /p PIKE_WAIT="]);
    #[cfg(not(windows))]
    let (program, args) = ("/bin/sh", vec!["-c", "read pike_wait"]);
    BackendSettings {
        program: program.into(),
        args: args.into_iter().map(str::to_owned).collect(),
        #[cfg(windows)]
        escape_args: false,
        ..Default::default()
    }
}

fn wait<T>(executor: &SharedExecutor, future: impl Future<Output = T>) -> T {
    executor.block_on(async { tokio::time::timeout(DEADLINE, future).await.unwrap() })
}

#[test]
fn slow_success_after_cancel_releases_pty_before_reusing_capacity() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::new(limits(1, 1));
    let (created, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let request = executor
        .enter(|| {
            pool.start_with(move |id| {
                let prepared = PreparedTerminal::new(id.get(), fixture_settings())?;
                created.send(prepared.shutdown_handle()).unwrap();
                gate.recv_timeout(DEADLINE).unwrap();
                Ok(prepared)
            })
        })
        .unwrap();
    let handle = observed.recv_timeout(DEADLINE).unwrap();
    request.cancellation().cancel();
    assert!(matches!(
        executor.enter(|| pool.start(fixture_settings())),
        Err(LaunchError::Busy)
    ));
    release.send(()).unwrap();
    assert_eq!(
        executor.block_on(request.receive()).unwrap_err(),
        LaunchError::Canceled
    );
    wait(&executor, pool.shutdown());
    assert_eq!(
        handle.completion().unwrap().reason,
        iced_term::ExitReason::Requested
    );
}

#[test]
fn unconsumed_success_stays_bounded_and_dropping_it_releases_pty() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::new(limits(1, 1));
    let request = executor.enter(|| pool.start(fixture_settings())).unwrap();
    let ready = executor.block_on(request.receive()).unwrap();
    let handle = ready.prepared.shutdown_handle();
    assert!(matches!(
        executor.enter(|| pool.start(fixture_settings())),
        Err(LaunchError::Busy)
    ));
    drop(ready);
    wait(&executor, pool.shutdown());
    assert_eq!(
        handle.completion().unwrap().reason,
        iced_term::ExitReason::Requested
    );
}

#[test]
fn attached_session_has_no_job_slot_but_retains_session_slot_until_cleanup() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::new(limits(1, 1));
    let request = executor.enter(|| pool.start(fixture_settings())).unwrap();
    let ready = executor.block_on(request.receive()).unwrap();
    let terminal = ready
        .attach(Default::default(), Default::default())
        .unwrap();
    // Await acquisition to synchronize the worker's handoff; never poll/sleep.
    let job = executor.block_on(pool.jobs.acquire()).unwrap();
    drop(job);
    assert!(matches!(
        executor.enter(|| pool.start(fixture_settings())),
        Err(LaunchError::SessionLimit)
    ));
    let handle = terminal.shutdown_handle();
    wait(&executor, pool.shutdown());
    assert!(handle.completion().is_some());
    // Final terminal model may outlive native resources without using admission.
    drop(terminal);
}

#[test]
fn canceled_prepared_handoff_cannot_attach_and_window_shutdown_waits_for_cleanup() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::default();
    let request = executor.enter(|| pool.start(fixture_settings())).unwrap();
    let cancel = request.cancellation();
    let ready = executor.block_on(request.receive()).unwrap();
    let handle = ready.prepared.shutdown_handle();
    cancel.cancel();
    assert!(matches!(
        ready.attach(Default::default(), Default::default()),
        Err(LaunchError::Canceled)
    ));
    wait(&executor, pool.shutdown());
    assert!(handle.completion().is_some());
}

#[test]
fn pool_drop_closes_prepared_and_attached_backends() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::default();
    let first = executor.enter(|| pool.start(fixture_settings())).unwrap();
    let ready = executor.block_on(first.receive()).unwrap();
    let handle = ready.prepared.shutdown_handle();
    let terminal = ready
        .attach(Default::default(), Default::default())
        .unwrap();
    drop(pool);
    wait(&executor, async {
        handle.wait().await;
    });
    assert!(handle.completion().is_some());
    drop(terminal);
}

#[test]
fn window_close_during_slow_success_waits_for_late_backend_disposal() {
    use iced::futures::FutureExt;
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::default();
    let (created, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let request = executor
        .enter(|| {
            pool.start_with(move |id| {
                let prepared = PreparedTerminal::new(id.get(), fixture_settings())?;
                created.send(prepared.shutdown_handle()).unwrap();
                gate.recv_timeout(DEADLINE).unwrap();
                Ok(prepared)
            })
        })
        .unwrap();
    let handle = observed.recv_timeout(DEADLINE).unwrap();
    let shutdown = pool.shutdown();
    assert!(Box::pin(shutdown).as_mut().now_or_never().is_none());
    release.send(()).unwrap();
    assert_eq!(
        executor.block_on(request.receive()).unwrap_err(),
        LaunchError::Canceled
    );
    wait(&executor, pool.shutdown());
    assert!(handle.completion().is_some());
}

/// Opt-in release measurement, kept out of normal timing-sensitive test runs.
/// This drives an event-loop surrogate; it does not measure Iced rendering.
#[test]
#[ignore = "release startup/cancellation measurement; see docs/verification/issue-10.md"]
fn measure_32_requested_launches() {
    use std::time::Instant;
    println!(
        "scenario,requested,admitted,rejected,max_dispatch_us,heartbeats,max_gap_ms,recovery_ms"
    );
    for scenario in ["target_close", "window_close", "failure"] {
        let executor = SharedExecutor::new().unwrap();
        let mut pool = LaunchPool::new(limits(2, 2));
        let (entered, entries) = mpsc::channel();
        let (handles, observed) = mpsc::channel();
        let mut releases = Vec::new();
        let mut requests = Vec::new();
        let mut max_dispatch = Duration::ZERO;
        for _ in 0..32 {
            let (release, gate) = mpsc::channel();
            let entered = entered.clone();
            let handles = handles.clone();
            let start = Instant::now();
            let launch = executor.enter(|| {
                pool.start_with(move |id| {
                    entered.send(()).unwrap();
                    gate.recv_timeout(DEADLINE).unwrap();
                    if scenario == "failure" {
                        return Err(io::Error::other("synthetic failure"));
                    }
                    // Deliberately complete successfully after cancellation, modeling
                    // an uninterruptible OS launch that was already in progress.
                    let prepared = PreparedTerminal::new(id.get(), fixture_settings())?;
                    handles.send(prepared.shutdown_handle()).unwrap();
                    Ok(prepared)
                })
            });
            max_dispatch = max_dispatch.max(start.elapsed());
            if requests.len() < 2 {
                releases.push(release);
                requests.push(launch.unwrap());
            } else {
                assert!(matches!(launch, Err(LaunchError::Busy)));
            }
        }
        for _ in 0..2 {
            entries.recv_timeout(DEADLINE).unwrap();
        }
        if scenario == "window_close" {
            drop(pool.shutdown()); // Signal now; await the same drain below.
        } else if scenario == "target_close" {
            for request in &requests {
                request.cancellation().cancel();
            }
        }
        let releasing = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            for release in releases {
                release.send(()).unwrap();
            }
            Instant::now()
        });
        let (heartbeats, max_gap) = executor.block_on(async {
            let start = Instant::now();
            let mut previous = start;
            let mut max_gap = Duration::ZERO;
            let mut heartbeats = 0;
            while start.elapsed() < Duration::from_millis(90) {
                tokio::time::sleep(Duration::from_millis(5)).await;
                let now = Instant::now();
                max_gap = max_gap.max(now.duration_since(previous));
                previous = now;
                heartbeats += 1;
            }
            (heartbeats, max_gap)
        });
        for request in requests {
            assert!(executor.block_on(request.receive()).is_err());
        }
        let released = releasing.join().unwrap();
        wait(&executor, pool.shutdown());
        let recovery = released.elapsed();
        if scenario != "failure" {
            for _ in 0..2 {
                let handle = observed.recv_timeout(DEADLINE).unwrap();
                assert_eq!(
                    handle.completion().unwrap().reason,
                    iced_term::ExitReason::Requested
                );
            }
        }
        println!(
            "{scenario},32,2,30,{:.3},{heartbeats},{:.3},{:.3}",
            max_dispatch.as_secs_f64() * 1e6,
            max_gap.as_secs_f64() * 1e3,
            recovery.as_secs_f64() * 1e3
        );
    }
}

#[test]
fn cancellation_during_ui_attachment_cannot_return_a_closing_terminal() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::default();
    let request = executor.enter(|| pool.start(fixture_settings())).unwrap();
    let cancel = request.cancellation();
    let ready = executor.block_on(request.receive()).unwrap();
    let handle = ready.prepared.shutdown_handle();
    let result = ready.attach_with(|prepared| {
        let terminal = prepared.attach(Default::default(), Default::default());
        // Deterministically place cancellation between the initial eligibility
        // check and committing the UI handoff, without timing assumptions.
        cancel.cancel();
        terminal
    });
    assert!(matches!(result, Err(LaunchError::Canceled)));
    wait(&executor, pool.shutdown());
    assert_eq!(
        handle.completion().unwrap().reason,
        iced_term::ExitReason::Requested
    );
}

#[test]
fn committed_ui_attachment_wins_over_later_startup_cancellation() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::default();
    let request = executor.enter(|| pool.start(fixture_settings())).unwrap();
    let cancel = request.cancellation();
    let ready = executor.block_on(request.receive()).unwrap();
    let terminal = ready
        .attach(Default::default(), Default::default())
        .unwrap();
    cancel.cancel();
    assert!(*cancel.0.borrow() == Handoff::Attached);
    let job = executor.block_on(pool.jobs.acquire()).unwrap();
    drop(job);
    assert!(terminal.shutdown_handle().completion().is_none());
    drop(terminal);
    wait(&executor, pool.shutdown());
}
