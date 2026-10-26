//! Bounded PTY-to-UI traffic. No producer waits for consumer capacity.
//!
//! The short state lock never covers terminal access, I/O, or an await. The
//! consumer removes one item per poll; there is no second forwarding queue.
use crate::AlacrittyEvent as Event;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::Notify;

const CONTROL_LIMIT: usize = 256;
const PAYLOAD_LIMIT: usize = 64 * 1024;
const TITLE_LIMIT: usize = 4096;

/// Redacted, session-scoped failure; never includes terminal/clipboard text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventBridgeError {
    ControlOverflow,
    PayloadOverflow,
    /// The event source ended without a terminal Exit event.
    BackendStopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventBridgeEvent {
    Error(EventBridgeError),
    /// Event production ended, not a joined/drained PTY shutdown acknowledgement.
    Completed,
    /// PTY/parser destruction and owned worker joins have finished.
    BackendCompleted(crate::BackendCompletion),
}

pub(crate) enum Delivery {
    Terminal(Event),
    Bridge(EventBridgeEvent),
}

#[derive(Default)]
struct State {
    controls: VecDeque<(Event, usize)>,
    payload_bytes: usize,
    title: Option<Event>,
    dirty: bool,
    mouse_dirty: bool,
    cursor_dirty: bool,
    bell: bool,
    child_exit: Option<i32>,
    exit: bool,
    exit_delivered: bool,
    error: Option<EventBridgeError>,
    error_delivered: bool,
    completed: bool,
    close_requested: bool,
    completion_delivered: bool,
    backend_expected: bool,
    backend_completion: Option<crate::BackendCompletion>,
    backend_delivered: bool,
    producers: usize,
    receiver_closed: bool,
}

impl State {
    fn finish(&mut self) {
        self.completed = true;
        if !self.exit && !self.close_requested && self.error.is_none() {
            self.error = Some(EventBridgeError::BackendStopped);
        }
    }

    fn pop(&mut self) -> Option<Delivery> {
        let event = if let Some((event, bytes)) = self.controls.pop_front() {
            self.payload_bytes -= bytes;
            event
        } else if let Some(title) = self.title.take() {
            title
        } else if std::mem::take(&mut self.dirty) {
            Event::Wakeup
        } else if std::mem::take(&mut self.mouse_dirty) {
            Event::MouseCursorDirty
        } else if std::mem::take(&mut self.cursor_dirty) {
            Event::CursorBlinkingChange
        } else if std::mem::take(&mut self.bell) {
            Event::Bell
        } else if let Some(code) = self.child_exit.take() {
            Event::ChildExit(code)
        } else if let Some(error) = self.error.filter(|_| !self.error_delivered)
        {
            self.error_delivered = true;
            return Some(Delivery::Bridge(EventBridgeEvent::Error(error)));
        } else if self.exit && !self.exit_delivered {
            self.exit_delivered = true;
            Event::Exit
        } else if self.completed && !self.completion_delivered {
            self.completion_delivered = true;
            return Some(Delivery::Bridge(EventBridgeEvent::Completed));
        } else if self.completed && !self.backend_delivered {
            if let Some(completion) = self.backend_completion {
                self.backend_delivered = true;
                return Some(Delivery::Bridge(
                    EventBridgeEvent::BackendCompleted(completion),
                ));
            }
            return None;
        } else {
            return None;
        };
        Some(Delivery::Terminal(event))
    }
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    ready: Notify,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
}

pub(crate) struct Sender {
    shared: Arc<Shared>,
    completes_on_drop: bool,
}

pub(crate) struct Receiver(Arc<Shared>);

pub(crate) fn channel() -> (Sender, Arc<Receiver>) {
    let shared = Arc::new(Shared::default());
    shared.lock().producers = 1;
    (
        Sender {
            shared: shared.clone(),
            completes_on_drop: false,
        },
        Arc::new(Receiver(shared)),
    )
}

impl Clone for Sender {
    fn clone(&self) -> Self {
        self.shared.lock().producers += 1;
        // Only the actual event-loop proxy owns the completion marker.
        Self {
            shared: self.shared.clone(),
            completes_on_drop: false,
        }
    }
}

impl Sender {
    pub(crate) fn complete_source(&self) {
        self.shared.lock().finish();
        self.shared.ready.notify_one();
    }

    pub(crate) fn expect_backend(&self) {
        self.shared.lock().backend_expected = true;
    }

    pub(crate) fn backend_completed(
        &self,
        completion: crate::BackendCompletion,
    ) {
        let mut state = self.shared.lock();
        state.backend_completion.get_or_insert(completion);
        drop(state);
        self.shared.ready.notify_one();
    }

    pub(crate) fn expect_close(&self) {
        self.shared.lock().close_requested = true;
    }

    pub(crate) fn complete_on_drop(mut self) -> Self {
        self.completes_on_drop = true;
        self
    }

    pub(crate) fn send(&self, event: Event) {
        let mut state = self.shared.lock();
        if state.receiver_closed || state.completed {
            return;
        }
        // Lifecycle slots remain writable even after overload/Exit. Freeze
        // ordinary admission so queued traffic cannot starve termination.
        match event {
            Event::Exit => state.exit = true,
            Event::ChildExit(code) => state.child_exit = Some(code),
            _ if state.error.is_some() || state.exit => return,
            Event::Wakeup => state.dirty = true,
            Event::MouseCursorDirty => state.mouse_dirty = true,
            Event::CursorBlinkingChange => state.cursor_dirty = true,
            Event::Bell => state.bell = true,
            Event::ResetTitle => state.title = Some(Event::ResetTitle),
            Event::Title(title) => {
                let mut end = title.len().min(TITLE_LIMIT);
                while !title.is_char_boundary(end) {
                    end -= 1;
                }
                // Copy a bounded slice, not an arbitrarily large allocation's
                // capacity. Titles are latest-value presentation state.
                state.title = Some(Event::Title(title[..end].to_owned()));
            },
            event @ (Event::PtyWrite(_)
            | Event::ClipboardStore(..)
            | Event::ClipboardLoad(..)
            | Event::ColorRequest(..)
            | Event::TextAreaSizeRequest(..)) => {
                // Count retained String capacity, not just its current length.
                // Upstream formatter Arcs capture only fixed protocol metadata;
                // their inline event storage is bounded by CONTROL_LIMIT.
                let bytes = match &event {
                    Event::PtyWrite(text) | Event::ClipboardStore(_, text) => {
                        text.capacity()
                    },
                    _ => 0,
                };
                if state.controls.len() == CONTROL_LIMIT {
                    state.error = Some(EventBridgeError::ControlOverflow);
                } else if bytes > PAYLOAD_LIMIT - state.payload_bytes {
                    state.error = Some(EventBridgeError::PayloadOverflow);
                } else {
                    state.payload_bytes += bytes;
                    state.controls.push_back((event, bytes));
                }
            },
        }
        drop(state);
        self.shared.ready.notify_one();
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        state.producers -= 1;
        if self.completes_on_drop || state.producers == 0 {
            state.finish();
        }
        drop(state);
        self.shared.ready.notify_one();
    }
}

impl Receiver {
    pub(crate) async fn recv(&self) -> Option<Delivery> {
        loop {
            let ready = self.0.ready.notified();
            {
                let mut state = self.0.lock();
                if let Some(delivery) = state.pop() {
                    return Some(delivery);
                }
                if state.completed
                    && (!state.backend_expected || state.backend_delivered)
                {
                    return None;
                }
            }
            ready.await;
        }
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.receiver_closed = true;
        state.controls.clear();
        state.payload_bytes = 0;
        state.title = None;
    }
}

#[cfg(test)]
mod tests;
