//! Byte admission shared by the input producer and the actual PTY writer.
use std::collections::VecDeque;
use std::fmt;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

pub const INPUT_BYTE_LIMIT: usize = 2 * 1024 * 1024;
pub const INPUT_CHUNK_SIZE: usize = 8 * 1024;

/// Progress reported by the actual writer, including buffered Windows workers.
#[derive(Default)]
pub struct WriterProgress {
    written: AtomicUsize,
    closed: AtomicBool,
}

impl WriterProgress {
    pub(crate) fn acknowledge(&self, bytes: usize) {
        self.written.fetch_add(bytes, Ordering::Release);
    }

    #[cfg(windows)]
    pub(crate) fn finish(&self) {
        self.closed.store(true, Ordering::Release);
    }

    pub(crate) fn written(&self) -> usize {
        self.written.load(Ordering::Acquire)
    }
}

/// Rejections contain metadata only, never input text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    Full { requested: usize, available: usize },
    Closed,
    WakeFailed,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full { requested, available } => {
                write!(f, "input rejected: {requested} bytes requested, {available} available")
            },
            Self::Closed => f.write_str("input rejected: session closed"),
            Self::WakeFailed => f.write_str("input rejected: could not wake PTY writer"),
        }
    }
}

impl std::error::Error for InputError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputStatus {
    pub outstanding_bytes: usize,
    pub queued_bytes: usize,
    pub written_bytes: usize,
    /// Remaining accepted bytes explicitly canceled by close or writer failure.
    /// An already executing OS write may still finish and reduce this count.
    pub cancelled_bytes: usize,
    pub closed: bool,
}

#[derive(Default)]
struct State {
    chunks: VecDeque<Vec<u8>>,
    queued: usize,
    accepted: usize,
    closed: bool,
}

struct Shared {
    state: Mutex<State>,
    progress: Arc<WriterProgress>,
}

#[derive(Clone)]
pub(crate) struct InputQueue(Arc<Shared>);

impl InputQueue {
    pub(crate) fn new(progress: Arc<WriterProgress>) -> Self {
        Self(Arc::new(Shared { state: Mutex::new(State::default()), progress }))
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        let mut state = self.0.state.lock().unwrap_or_else(|error| error.into_inner());
        if self.0.progress.closed.load(Ordering::Acquire) {
            state.closed = true;
            state.chunks.clear();
            state.queued = 0;
        }
        state
    }

    fn available(&self, state: &State) -> usize {
        // Both cumulative counters wrap together. Their distance is always
        // bounded by INPUT_BYTE_LIMIT, including at integer wraparound.
        let outstanding = state.accepted.wrapping_sub(self.0.progress.written());
        INPUT_BYTE_LIMIT - outstanding
    }

    pub(crate) fn preflight(&self, bytes: usize) -> Result<(), InputError> {
        self.check(&self.lock(), bytes)
    }

    fn check(&self, state: &State, bytes: usize) -> Result<(), InputError> {
        if state.closed {
            return Err(InputError::Closed);
        }
        let available = self.available(state);
        if bytes > available {
            return Err(InputError::Full { requested: bytes, available });
        }
        Ok(())
    }

    pub(crate) fn send(
        &self,
        bytes: &[u8],
        wake: impl FnOnce() -> io::Result<()>,
    ) -> Result<(), InputError> {
        let mut state = self.lock();
        self.check(&state, bytes.len())?;
        if bytes.is_empty() {
            return Ok(());
        }
        // Wake before modifying the queue, under its short lock. A woken
        // consumer acquires this lock after append. Failed wakes admit nothing.
        // No terminal lock or blocking PTY operation is acquired here.
        wake().map_err(|_| InputError::WakeFailed)?;
        state.accepted = state.accepted.wrapping_add(bytes.len());
        state.queued += bytes.len();
        let mut remaining = bytes;
        while !remaining.is_empty() {
            if state.chunks.back().is_none_or(|chunk| chunk.len() == INPUT_CHUNK_SIZE) {
                state.chunks.push_back(Vec::with_capacity(INPUT_CHUNK_SIZE));
            }
            let tail = state.chunks.back_mut().expect("chunk was just allocated");
            let count = remaining.len().min(INPUT_CHUNK_SIZE - tail.len());
            tail.extend_from_slice(&remaining[..count]);
            remaining = &remaining[count..];
        }
        Ok(())
    }

    pub(crate) fn take(&self) -> Option<Vec<u8>> {
        let mut state = self.lock();
        let chunk = state.chunks.pop_front()?;
        state.queued -= chunk.len();
        Some(chunk)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.lock().chunks.is_empty()
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.lock().closed
    }

    pub(crate) fn acknowledge(&self, bytes: usize) {
        self.0.progress.acknowledge(bytes);
    }

    pub(crate) fn close(&self) {
        let mut state = self.lock();
        state.closed = true;
        state.chunks.clear();
        state.queued = 0;
    }

    pub(crate) fn status(&self) -> InputStatus {
        let state = self.lock();
        let written_bytes = self.0.progress.written();
        let pending = state.accepted.wrapping_sub(written_bytes);
        InputStatus {
            outstanding_bytes: if state.closed { 0 } else { pending },
            queued_bytes: state.queued,
            written_bytes,
            cancelled_bytes: if state.closed { pending } else { 0 },
            closed: state.closed,
        }
    }
}

#[cfg(test)]
mod tests;
