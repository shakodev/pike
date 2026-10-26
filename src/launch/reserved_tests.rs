//! Cross-module execution of the model's Spawn/Close contract.
use super::*;
use crate::layout::PaneTarget;
use crate::session::{Effect, Failure, Lifecycle, SpawnDisposition};
use crate::workspace::Model;

fn reserve(model: &mut Model) -> (PaneTarget, Reservation) {
    let (target, Effect::Spawn(ids)) = model.create_tab().unwrap() else {
        panic!("expected spawn effect");
    };
    (target, ids)
}

#[test]
fn model_effect_preserves_identity_through_preparation_attachment_and_backend_events() {
    use iced::advanced::subscription::into_recipes;
    use iced::futures::{StreamExt, stream};

    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::new(limits(1, 1));
    let mut model = Model::default();
    let (target, ids) = reserve(&mut model);
    let token = model.claim_launch(ids).unwrap();
    assert_eq!(token.reservation(), ids);
    assert!(model.claim_launch(ids).is_none()); // Replaying the Copy effect is inert.
    let launch = executor
        .enter(|| pool.start_reserved(token, fixture_settings()))
        .unwrap();
    assert_eq!((launch.session, launch.request), (ids.session, ids.request));
    let ready = executor.block_on(launch.receive()).unwrap();
    assert_eq!((ready.session, ready.request), (ids.session, ids.request));
    assert!(model.accepts_spawn(ids));
    // PreparedTerminal carries the reserved identity through its real attach.
    let terminal = ready
        .attach(Default::default(), Default::default())
        .unwrap();
    assert_eq!(terminal.id, ids.session.get());
    assert_eq!(model.spawn_succeeded(ids), SpawnDisposition::Attach);
    assert_eq!(
        model.sessions().get(ids.session).unwrap().lifecycle(),
        Lifecycle::Running
    );
    assert!(model.claim_launch(ids).is_none());

    let mut events = into_recipes(terminal.subscription())
        .pop()
        .unwrap()
        .stream(Box::pin(stream::pending()));
    assert_eq!(model.close_pane(target), Ok(Some(Effect::Close(ids))));
    terminal.request_close();
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(iced_term::Event::BackendCall(id, command)) = events.next().await {
                assert_eq!(id, ids.session.get());
                if let iced_term::BackendCommand::EventBridge(
                    iced_term::EventBridgeEvent::BackendCompleted(completion),
                ) = command
                {
                    assert_eq!(completion.reason, iced_term::ExitReason::Requested);
                    return;
                }
            }
            panic!("missing backend completion");
        })
        .await
        .unwrap();
    });
    assert!(model.closed(ids.session));
    wait(&executor, pool.shutdown());
    drop(terminal);
}

#[test]
fn stale_mismatched_and_abandoned_claims_do_not_launch_or_reissue_tokens() {
    let mut model = Model::default();
    let (stale, stale_ids) = reserve(&mut model);
    model.close_pane(stale).unwrap();
    assert!(!model.accepts_spawn(stale_ids));
    assert!(model.claim_launch(stale_ids).is_none());
    assert!(model.spawn_failed(stale_ids, Failure::Canceled));
    let (_, ids) = reserve(&mut model);
    let mismatch = Reservation {
        request: stale_ids.request,
        ..ids
    };
    assert!(model.claim_launch(mismatch).is_none());
    {
        let _abandoned = model.claim_launch(ids).unwrap();
        // No pool submission, native resource or work exists.
    }
    assert!(model.spawn_failed(ids, Failure::Canceled));
    assert!(model.claim_launch(ids).is_none());
    assert!(!model.accepts_spawn(ids));
    assert_eq!(model.spawn_succeeded(ids), SpawnDisposition::Dispose);
}

#[test]
fn reserved_admission_rejection_spends_token_and_rolls_back_all_acquired_permits() {
    let executor = SharedExecutor::new().unwrap();
    let mut model = Model::default();
    let mut pool = LaunchPool::new(limits(1, 1));
    for reason in [
        LaunchError::RuntimeUnavailable,
        LaunchError::Busy,
        LaunchError::SessionLimit,
    ] {
        let (target, ids) = reserve(&mut model);
        let token = model.claim_launch(ids).unwrap();
        let held_job =
            (reason == LaunchError::Busy).then(|| pool.jobs.clone().try_acquire_owned().unwrap());
        let held_session = (reason == LaunchError::SessionLimit)
            .then(|| pool.sessions.clone().try_acquire_owned().unwrap());
        let result = if reason == LaunchError::RuntimeUnavailable {
            pool.start_reserved(token, fixture_settings())
        } else {
            executor.enter(|| pool.start_reserved(token, fixture_settings()))
        };
        assert!(matches!(result, Err(error) if error == reason));
        assert_eq!(
            pool.jobs.available_permits(),
            usize::from(held_job.is_none())
        );
        assert_eq!(
            pool.sessions.available_permits(),
            usize::from(held_session.is_none())
        );
        drop(held_job);
        drop(held_session);
        assert_eq!(pool.jobs.available_permits(), 1);
        assert_eq!(pool.sessions.available_permits(), 1);
        assert!(model.claim_launch(ids).is_none());
        assert!(model.spawn_failed(ids, Failure::Admission));
        assert_eq!(model.close_pane(target), Ok(None));
    }
    // A fresh reservation works after each rejection; rejected attempts never
    // queued preparation or retained a permit/native resource.
    let (target, ids) = reserve(&mut model);
    let token = model.claim_launch(ids).unwrap();
    let launch = executor
        .enter(|| pool.start_reserved(token, fixture_settings()))
        .unwrap();
    assert_eq!(launch.session, ids.session);
    let ready = executor.block_on(launch.receive()).unwrap();
    let handle = ready.shutdown_handle();
    model.close_pane(target).unwrap();
    assert_eq!(model.spawn_succeeded(ids), SpawnDisposition::Dispose);
    drop(ready);
    wait(&executor, handle.wait());
    assert!(model.closed(ids.session));
    wait(&executor, pool.shutdown());

    let (target, ids) = reserve(&mut model);
    let token = model.claim_launch(ids).unwrap();
    assert!(matches!(
        executor.enter(|| pool.start_reserved(token, fixture_settings())),
        Err(LaunchError::Closed)
    ));
    assert_eq!(pool.jobs.available_permits(), 1);
    assert_eq!(pool.sessions.available_permits(), 1);
    assert!(model.spawn_failed(ids, Failure::Admission));
    assert_eq!(model.close_pane(target), Ok(None));
    assert!(model.sessions().is_empty());
}

#[test]
fn canceled_consumed_reservation_disposes_late_success_and_fresh_retry_keeps_identity() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::new(limits(1, 1));
    let mut model = Model::default();
    let (target, ids) = reserve(&mut model);
    let token = model.claim_launch(ids).unwrap();
    let (created, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let launch = executor
        .enter(|| {
            pool.start_job(Some(token), move |id| {
                assert_eq!(id, ids.session);
                let prepared = PreparedTerminal::new(id.get(), fixture_settings())?;
                created.send(prepared.shutdown_handle()).unwrap();
                gate.recv_timeout(DEADLINE).unwrap();
                Ok(prepared)
            })
        })
        .unwrap();
    let handle = observed.recv_timeout(DEADLINE).unwrap();
    model.close_pane(target).unwrap();
    launch.cancellation().cancel();
    assert!(!model.accepts_spawn(ids));
    assert!(model.claim_launch(ids).is_none());
    assert_eq!(pool.jobs.available_permits(), 0);
    assert_eq!(pool.sessions.available_permits(), 0);
    release.send(()).unwrap();
    assert_eq!(
        executor.block_on(launch.receive()).unwrap_err(),
        LaunchError::Canceled
    );
    wait(&executor, handle.wait());
    // Synchronize worker permit release without closing the pool before retry.
    drop(executor.block_on(pool.jobs.acquire()).unwrap());
    drop(executor.block_on(pool.sessions.acquire()).unwrap());
    assert!(model.spawn_failed(ids, Failure::Canceled));

    let (retry, retry_ids) = reserve(&mut model);
    assert!(retry_ids.session.get() > ids.session.get());
    assert!(retry_ids.request.get() > ids.request.get());
    let token = model.claim_launch(retry_ids).unwrap();
    let launch = executor
        .enter(|| pool.start_reserved(token, fixture_settings()))
        .unwrap();
    assert_eq!(
        (launch.session, launch.request),
        (retry_ids.session, retry_ids.request)
    );
    let ready = executor.block_on(launch.receive()).unwrap();
    assert!(model.accepts_spawn(retry_ids));
    let terminal = ready
        .attach(Default::default(), Default::default())
        .unwrap();
    assert_eq!(terminal.id, retry_ids.session.get());
    assert_eq!(model.spawn_succeeded(retry_ids), SpawnDisposition::Attach);
    let handle = terminal.shutdown_handle();
    model.close_pane(retry).unwrap();
    terminal.request_close();
    wait(&executor, handle.wait());
    assert!(model.closed(retry_ids.session));
    wait(&executor, pool.shutdown());
    drop(terminal);
}

#[test]
fn reserved_attachment_cancellation_reports_failure_before_model_commit() {
    let executor = SharedExecutor::new().unwrap();
    let mut pool = LaunchPool::new(limits(1, 1));
    let mut model = Model::default();
    let (_, ids) = reserve(&mut model);
    let token = model.claim_launch(ids).unwrap();
    let launch = executor
        .enter(|| pool.start_reserved(token, fixture_settings()))
        .unwrap();
    let cancel = launch.cancellation();
    let ready = executor.block_on(launch.receive()).unwrap();
    let handle = ready.shutdown_handle();
    assert!(model.accepts_spawn(ids));
    let result = ready.attach_with(|prepared| {
        let terminal = prepared.attach(Default::default(), Default::default());
        cancel.cancel();
        terminal
    });
    assert!(matches!(result, Err(LaunchError::Canceled)));
    assert!(model.spawn_failed(ids, Failure::Canceled));
    assert_eq!(
        model.sessions().get(ids.session).unwrap().lifecycle(),
        Lifecycle::Failed(Failure::Canceled)
    );
    assert!(!model.accepts_spawn(ids));
    assert!(model.claim_launch(ids).is_none());
    wait(&executor, handle.wait());
    wait(&executor, pool.shutdown());
}
