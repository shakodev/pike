//! Bounded startup on the application's shared runtime. No Terminal crosses a thread.
use std::future::Future;
use std::io;
use std::num::NonZeroU16;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use iced::futures::future::{self, Either};
use iced_term::settings::{BackendSettings, FontSettings, ThemeSettings};
use iced_term::{PreparedTerminal, Terminal};
use tokio::sync::{Semaphore, oneshot, watch};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionId(u64);

impl SessionId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LaunchLimits {
    /// Total running, queued and delivered-but-unattached launches.
    pub jobs: NonZeroU16,
    /// Includes starting sessions and sessions awaiting backend disposal.
    pub sessions: NonZeroU16,
}

impl Default for LaunchLimits {
    fn default() -> Self {
        Self {
            jobs: NonZeroU16::new(2).unwrap(),
            sessions: NonZeroU16::new(16).unwrap(),
        }
    }
}

/// Metadata only; never retain a command, path, environment or OS error text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchError {
    Busy,
    SessionLimit,
    Closed,
    IdsExhausted,
    RuntimeUnavailable,
    Canceled,
    WorkerFailed,
    Spawn(io::ErrorKind),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Busy => "Terminal startup is busy. Try again.",
            Self::SessionLimit => "Session limit reached. Close a terminal and try again.",
            Self::Closed => "Pike is shutting down.",
            Self::IdsExhausted => "No more session identifiers are available.",
            Self::Canceled => "Terminal startup was canceled.",
            Self::Spawn(io::ErrorKind::NotFound) => "Shell executable was not found.",
            Self::Spawn(io::ErrorKind::PermissionDenied) => {
                "Permission to start the shell was denied."
            }
            Self::Spawn(_) => "Could not start the shell.",
            Self::RuntimeUnavailable | Self::WorkerFailed => "Terminal startup failed. Try again.",
        })
    }
}

impl std::error::Error for LaunchError {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Handoff {
    Pending,
    Attached,
    Canceled,
}

#[derive(Clone)]
pub struct CancelLaunch(watch::Sender<Handoff>);

impl CancelLaunch {
    /// Cancel only pending startup; a committed UI attachment owns its session.
    pub fn cancel(&self) {
        self.0.send_if_modified(|state| {
            if *state != Handoff::Pending {
                return false;
            }
            *state = Handoff::Canceled;
            true
        });
    }

    fn attach(&self) -> bool {
        self.0.send_if_modified(|state| {
            if *state != Handoff::Pending {
                return false;
            }
            *state = Handoff::Attached;
            true
        })
    }
}

/// Dropping the receive future cancels admission/late results, even if the
/// spawn_blocking closure has already started and cannot itself be aborted.
pub struct LaunchRequest {
    pub request: RequestId,
    pub session: SessionId,
    cancel: Option<CancelLaunch>,
    result: oneshot::Receiver<Result<Box<PreparedSession>, LaunchError>>,
}

impl LaunchRequest {
    pub fn cancellation(&self) -> CancelLaunch {
        self.cancel.as_ref().unwrap().clone()
    }

    pub async fn receive(mut self) -> Result<Box<PreparedSession>, LaunchError> {
        let result = (&mut self.result)
            .await
            .unwrap_or(Err(LaunchError::WorkerFailed));
        // The returned prepared owner now controls disposal. Do not cancel it
        // merely because this completed receive future is dropped.
        self.cancel.take();
        result
    }
}

impl Drop for LaunchRequest {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
    }
}

/// Single-use handoff. Its Drop closes the backend when the target disappeared
/// or a completion message was discarded. It is Send; Terminal need not be.
pub struct PreparedSession {
    pub request: RequestId,
    pub session: SessionId,
    prepared: PreparedTerminal,
    attached: oneshot::Sender<()>,
    handoff: CancelLaunch,
    shutdown: watch::Receiver<bool>,
}

impl PreparedSession {
    pub fn attach(self, font: FontSettings, theme: ThemeSettings) -> Result<Terminal, LaunchError> {
        self.attach_with(move |prepared| prepared.attach(font, theme))
    }

    /// Attach a background session without producing an initial viewport.
    /// Admission, cancellation and handoff ownership are identical to attach.
    pub fn attach_hidden(
        self,
        font: FontSettings,
        theme: ThemeSettings,
    ) -> Result<Terminal, LaunchError> {
        self.attach_with(move |prepared| prepared.attach_hidden(font, theme))
    }

    fn attach_with(
        self,
        attach: impl FnOnce(PreparedTerminal) -> Terminal,
    ) -> Result<Terminal, LaunchError> {
        if *self.handoff.0.borrow() == Handoff::Canceled
            || *self.shutdown.borrow()
            || self.shutdown.has_changed().is_err()
        {
            return Err(LaunchError::Canceled);
        }
        let terminal = attach(self.prepared);
        // Cancellation can arrive while fonts are measured. Commit the handoff
        // under the same short watch lock as cancel; exactly one can win.
        if !self.handoff.attach() {
            return Err(LaunchError::Canceled);
        }
        let _ = self.attached.send(());
        Ok(terminal)
    }
}

impl std::fmt::Debug for PreparedSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedSession")
            .field("request", &self.request)
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

/// Owned by UI/session state, independent of pane/layout state. Admission never
/// awaits capacity and creates no task on rejection. Workers retain both permits
/// until handoff or acknowledged disposal, including when the UI stops receiving.
/// Dropping the pool also signals all its workers to close their backends.
pub struct LaunchPool {
    jobs: Arc<Semaphore>,
    sessions: Arc<Semaphore>,
    session_limit: u32,
    shutdown: watch::Sender<bool>,
    closed: bool,
}

impl Default for LaunchPool {
    fn default() -> Self {
        Self::new(LaunchLimits::default())
    }
}

impl LaunchPool {
    pub fn new(limits: LaunchLimits) -> Self {
        Self {
            jobs: Arc::new(Semaphore::new(usize::from(limits.jobs.get()))),
            sessions: Arc::new(Semaphore::new(usize::from(limits.sessions.get()))),
            session_limit: u32::from(limits.sessions.get()),
            shutdown: watch::channel(false).0,
            closed: false,
        }
    }

    /// Call from the shared executor's entered context, as Iced does for update.
    pub fn start(&mut self, settings: BackendSettings) -> Result<LaunchRequest, LaunchError> {
        self.start_with(move |id| PreparedTerminal::new(id.get(), settings))
    }

    fn start_with(
        &mut self,
        prepare: impl FnOnce(SessionId) -> io::Result<PreparedTerminal> + Send + 'static,
    ) -> Result<LaunchRequest, LaunchError> {
        if self.closed {
            return Err(LaunchError::Closed);
        }
        let runtime =
            tokio::runtime::Handle::try_current().map_err(|_| LaunchError::RuntimeUnavailable)?;
        let job = self
            .jobs
            .clone()
            .try_acquire_owned()
            .map_err(|_| LaunchError::Busy)?;
        let session = self
            .sessions
            .clone()
            .try_acquire_owned()
            .map_err(|_| LaunchError::SessionLimit)?;
        let id = NEXT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| LaunchError::IdsExhausted)?;
        let request_id = RequestId(id);
        let session_id = SessionId(id);
        let (cancel, mut canceled) = watch::channel(Handoff::Pending);
        let handoff = CancelLaunch(cancel.clone());
        let check_cancel = canceled.clone();
        let mut shutdown = self.shutdown.subscribe();
        let check_shutdown = shutdown.clone();
        let (send, result) = oneshot::channel();
        runtime.spawn(async move {
            // Admission happened before either async or blocking work was queued.
            let result = tokio::task::spawn_blocking(move || {
                if *check_cancel.borrow() == Handoff::Canceled
                    || *check_shutdown.borrow()
                    || check_shutdown.has_changed().is_err()
                {
                    return Err(LaunchError::Canceled);
                }
                prepare(session_id).map_err(|error| LaunchError::Spawn(error.kind()))
            })
            .await
            .unwrap_or(Err(LaunchError::WorkerFailed));
            match result {
                Ok(prepared) => {
                    let handle = prepared.shutdown_handle();
                    let (attached, mut attached_rx) = oneshot::channel();
                    if *canceled.borrow() == Handoff::Canceled
                        || *shutdown.borrow()
                        || send.is_closed()
                    {
                        drop(prepared);
                        let _ = send.send(Err(LaunchError::Canceled));
                    } else {
                        // On send failure the returned owner drops here and closes.
                        let _ = send.send(Ok(Box::new(PreparedSession {
                            request: request_id,
                            session: session_id,
                            prepared,
                            attached,
                            handoff,
                            shutdown: shutdown.clone(),
                        })));
                        let interrupted = async {
                            future::select(
                                Box::pin(cancellation(&mut canceled)),
                                Box::pin(stop(&mut shutdown)),
                            )
                            .await;
                        };
                        let attached =
                            match future::select(&mut attached_rx, Box::pin(interrupted)).await {
                                Either::Left((result, _)) => Some(result.is_ok()),
                                Either::Right(_) => None,
                            };
                        if attached == Some(true) {
                            drop(job);
                            match future::select(
                                Box::pin(handle.wait()),
                                Box::pin(stop(&mut shutdown)),
                            )
                            .await
                            {
                                Either::Left(_) => {}
                                Either::Right(_) => {
                                    handle.request_close();
                                    handle.wait().await;
                                }
                            }
                            drop(session);
                            return;
                        }
                        handle.request_close();
                        handle.wait().await;
                        // Keep undelivered prepared results within the job budget
                        // even after cancellation has released their native PTY.
                        if attached.is_none() {
                            let _ = attached_rx.await;
                        }
                    }
                    handle.request_close();
                    handle.wait().await;
                }
                Err(error) => {
                    let _ = send.send(Err(error));
                }
            }
            drop(job);
            drop(session);
        });
        Ok(LaunchRequest {
            request: request_id,
            session: session_id,
            cancel: Some(CancelLaunch(cancel)),
            result,
        })
    }

    /// Stop admission immediately; await all starting/live/disposal ownership
    /// without polling. Keep the shared runtime alive until this future resolves.
    pub fn shutdown(&mut self) -> impl Future<Output = ()> + Send + 'static {
        self.closed = true;
        self.shutdown.send_replace(true);
        let sessions = self.sessions.clone();
        let count = self.session_limit;
        async move {
            let _all = sessions.acquire_many_owned(count).await;
        }
    }
}

async fn stop(receiver: &mut watch::Receiver<bool>) {
    // A closed sender also means its UI/pool owner disappeared.
    let _ = receiver.wait_for(|stopped| *stopped).await;
}

async fn cancellation(receiver: &mut watch::Receiver<Handoff>) {
    if receiver
        .wait_for(|state| *state == Handoff::Canceled)
        .await
        .is_err()
    {
        // A successful receive disarms its Drop guard. Only explicit cancellation
        // should close an owner that has already been handed to the UI.
        std::future::pending::<()>().await;
    }
}

#[cfg(test)]
mod tests;
