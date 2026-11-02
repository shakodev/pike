//! Independent exact-App ordering regression. No native child is created.
use super::*;
use crate::executor::SharedExecutor;
use iced::Executor;
use std::{sync::mpsc, time::Duration};

#[test]
fn timeout_then_renderer_failure_must_enqueue_native_choice() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App::default();
    let (release, gate) = mpsc::channel();
    let (started, observed) = mpsc::channel();
    let request = executor.enter(|| app.launches.start_with(move |_| {
        started.send(()).unwrap();
        gate.recv_timeout(Duration::from_secs(10)).unwrap();
        Err(std::io::ErrorKind::Other.into())
    })).unwrap();
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    // Pending pool work also includes earlier removed/undelivered preparations.
    drop(app.update(Message::CloseWindow));
    let token = app.close_policy.timer.unwrap();
    assert!(app.exiting);
    assert!(!app.launches.shutdown_complete());
    let original_timeout = app.update(Message::CloseTimeout(token));
    assert_eq!(original_timeout.units(), 0);
    assert!(app.close_policy.timed_out);
    assert!(app.close_policy.native_prompt.is_none());
    let response = app.update(Message::RendererFailed(
        iced_runtime::system::RendererFailure::OutOfMemory,
    ));
    let enqueued = app.close_policy.native_prompt.is_some();
    let units = response.units();
    drop(response);
    eprintln!("timeout_before_renderer_failure=true native_choice_enqueued={enqueued} renderer_response_units={units}");
    // Always release and acknowledge owned work before reporting the RED.
    release.send(()).unwrap();
    executor.block_on(async {
        tokio::time::timeout(Duration::from_secs(3), app.launches.shutdown()).await.unwrap();
    });
    assert!(executor.block_on(request.receive()).is_err());
    assert!(app.launches.shutdown_complete());
    drop(app.shutdown_complete());
    assert!(enqueued && units > 0, "renderer failure after the ordinary timeout must transfer the existing Wait/Force choice to the native fallback");
}
