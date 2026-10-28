//! Backend lifetime observation independent of the Iced event subscription.
use crate::event_bridge::Sender;
use alacritty_terminal::event_loop::{EventLoopSender, Msg};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::watch;

pub use alacritty_terminal::event_loop::{
    Completion as BackendCompletion, ExitReason,
};

/// Cloning/awaiting this handle never owns a PTY or blocks the caller's thread.
/// Dropping a waiter cancels only that wait; it cannot cancel backend cleanup.
#[derive(Clone)]
pub struct ShutdownHandle {
    pub(crate) notifier: EventLoopSender,
    events: Sender,
    requested: Arc<AtomicBool>,
    completion: watch::Receiver<Option<BackendCompletion>>,
    pending_paste: Arc<crate::paste::PasteSlot>,
}

impl ShutdownHandle {
    pub(crate) fn new(
        notifier: EventLoopSender,
        events: Sender,
        completion: watch::Receiver<Option<BackendCompletion>>,
        pending_paste: Arc<crate::paste::PasteSlot>,
    ) -> Self {
        Self {
            notifier,
            events,
            requested: Arc::new(AtomicBool::new(false)),
            completion,
            pending_paste,
        }
    }

    /// Idempotent, nonblocking request. Accepted unwritten input is canceled.
    /// The return value says whether this handle initiated the request, not
    /// whether backend cleanup has completed. Observe `wait`/`completion` for that.
    pub fn request_close(&self) -> bool {
        self.pending_paste.cancel();
        if self.completion().is_some()
            || self.requested.swap(true, Ordering::AcqRel)
        {
            return false;
        }
        self.events.expect_close();
        let _ = self.notifier.send(Msg::Shutdown);
        true
    }

    pub fn completion(&self) -> Option<BackendCompletion> {
        *self.completion.borrow()
    }

    /// Wait without polling. Callers may wrap this in their own timeout; a
    /// timeout must remain pending cleanup, never be treated as completion.
    pub async fn wait(&self) -> BackendCompletion {
        let mut receiver = self.completion.clone();
        loop {
            if let Some(completion) = *receiver.borrow_and_update() {
                return completion;
            }
            // The worker always publishes after disposal, including unwind.
            if receiver.changed().await.is_err() {
                return BackendCompletion {
                    reason: ExitReason::Panicked,
                    exit_code: None,
                    output_drained: false,
                };
            }
        }
    }
}
