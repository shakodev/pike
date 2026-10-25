//! One runtime per Iced application, shared by all tasks and subscriptions.

use std::future::Future;
use std::io;
use tokio::runtime::{Builder, Runtime};

/// A small Tokio executor. Terminal PTY workers remain owned by Alacritty.
///
/// The blocking-worker limit bounds concurrently running work, not Tokio's
/// pending task queue. Callers must bound admission before spawning blocking
/// work. On application exit, Runtime drops async tasks and joins its workers;
/// blocking tasks must be finite. Session teardown belongs before app exit.
pub struct SharedExecutor(Runtime);

impl iced::Executor for SharedExecutor {
    fn new() -> io::Result<Self> {
        Builder::new_multi_thread()
            .worker_threads(1)
            .max_blocking_threads(2)
            .thread_name("pike-runtime")
            .enable_time()
            .build()
            .map(Self)
    }

    fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        // Iced owns cancellation through its subscription/task futures.
        drop(self.0.spawn(future));
    }

    fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = self.0.enter();
        f()
    }

    fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        self.0.block_on(future)
    }
}

#[cfg(test)]
mod tests {
    use super::SharedExecutor;
    use iced::Executor;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    const DEADLINE: Duration = Duration::from_secs(5);

    #[test]
    fn enter_spawn_block_on_and_timers_share_one_runtime() {
        let executor = SharedExecutor::new().unwrap();
        let handle = executor.enter(tokio::runtime::Handle::current);
        assert_eq!(handle.metrics().num_workers(), 1);
        let timer = executor.enter(|| tokio::time::sleep(Duration::from_millis(1)));
        let (send, recv) = tokio::sync::oneshot::channel();
        executor.spawn(async move {
            timer.await;
            send.send(tokio::runtime::Handle::current().id()).unwrap();
        });
        let id = executor
            .block_on(async { tokio::time::timeout(DEADLINE, recv).await.unwrap().unwrap() });
        assert_eq!(id, handle.id());
        assert!(tokio::runtime::Handle::try_current().is_err());
    }

    #[test]
    fn canceled_task_drops_its_future_and_executor_keeps_running() {
        let executor = SharedExecutor::new().unwrap();
        let (started, ready) = mpsc::channel();
        let (dropped, observed) = mpsc::channel();
        struct NotifyDrop(mpsc::Sender<()>);
        impl Drop for NotifyDrop {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let task = executor.enter(|| {
            tokio::spawn(async move {
                let _guard = NotifyDrop(dropped);
                started.send(()).unwrap();
                std::future::pending::<()>().await;
            })
        });
        ready.recv_timeout(DEADLINE).unwrap();
        task.abort();
        observed.recv_timeout(DEADLINE).unwrap();
        assert!(executor.block_on(task).unwrap_err().is_cancelled());
        assert_eq!(executor.block_on(async { 42 }), 42);
    }

    #[test]
    fn blocking_work_queues_after_two_workers() {
        let executor = SharedExecutor::new().unwrap();
        let gate = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
        let (started, ready) = mpsc::channel();
        let mut tasks = Vec::new();
        for _ in 0..2 {
            let gate = Arc::clone(&gate);
            let started = started.clone();
            tasks.push(executor.enter(|| {
                tokio::task::spawn_blocking(move || {
                    started.send(()).unwrap();
                    let (lock, signal) = &*gate;
                    let _result = signal
                        .wait_timeout_while(lock.lock().unwrap(), DEADLINE, |open| !*open)
                        .unwrap();
                })
            }));
        }
        ready.recv_timeout(DEADLINE).unwrap();
        ready.recv_timeout(DEADLINE).unwrap();
        let third = executor.enter(|| tokio::task::spawn_blocking(move || started.send(())));
        let queued = ready.recv_timeout(Duration::from_millis(50));
        let (lock, signal) = &*gate;
        *lock.lock().unwrap() = true;
        signal.notify_all();
        assert_eq!(queued, Err(mpsc::RecvTimeoutError::Timeout));
        ready.recv_timeout(DEADLINE).unwrap();
        executor.block_on(async {
            for task in tasks {
                task.await.unwrap();
            }
            third.await.unwrap().unwrap();
        });
    }

    #[test]
    fn drop_cancels_pending_async_work_and_joins_finite_blocking_work() {
        let executor = SharedExecutor::new().unwrap();
        let (started, ready) = mpsc::channel();
        let (send, recv) = tokio::sync::oneshot::channel::<()>();
        executor.spawn(async move {
            let _send = send;
            started.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        ready.recv_timeout(DEADLINE).unwrap();
        let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (blocking_started, blocking_ready) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        executor.enter(|| {
            let finished = Arc::clone(&finished);
            drop(tokio::task::spawn_blocking(move || {
                blocking_started.send(()).unwrap();
                gate.recv_timeout(DEADLINE).unwrap();
                finished.store(true, std::sync::atomic::Ordering::SeqCst);
            }));
        });
        blocking_ready.recv_timeout(DEADLINE).unwrap();
        let (stopped, shutdown) = mpsc::channel();
        let shutdown_thread = std::thread::spawn(move || {
            drop(executor);
            stopped.send(()).unwrap();
        });
        let waiting = shutdown.recv_timeout(Duration::from_millis(50));
        release.send(()).unwrap();
        assert_eq!(waiting, Err(mpsc::RecvTimeoutError::Timeout));
        shutdown.recv_timeout(DEADLINE).unwrap();
        shutdown_thread.join().unwrap();
        assert!(recv.blocking_recv().is_err());
        assert!(finished.load(std::sync::atomic::Ordering::SeqCst));
    }
}
