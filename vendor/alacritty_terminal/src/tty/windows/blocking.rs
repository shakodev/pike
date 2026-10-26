//! Code for running a reader/writer on another thread while driving it through `polling`.

use crate::input::WriterProgress;
use std::io::prelude::*;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::{io, thread};

use piper::{pipe, Reader, Writer};
use polling::os::iocp::{CompletionPacket, PollerIocpExt};
use polling::{Event, PollMode, Poller};

use super::conpty::{fault, Failure};

struct Registration {
    interest: Mutex<Option<Interest>>,
    end: PipeEnd,
    // Reader-only emergency close mode; reuse this allocation for control.
    drain_directly: AtomicBool,
}

#[derive(Copy, Clone)]
enum PipeEnd {
    Reader,
    Writer,
}

struct Interest {
    /// The event to send about completion.
    event: Event,

    /// The poller to send the event to.
    poller: Arc<Poller>,

    /// The mode that we are in.
    mode: PollMode,
}

/// Poll a reader in another thread.
pub struct UnblockedReader<R> {
    /// The event to send about completion.
    interest: Arc<Registration>,

    /// The pipe that we are reading from.
    pipe: Reader,

    /// Is this the first time registering?
    first_register: bool,

    /// We logically own the reader, but we don't actually use it.
    _reader: PhantomData<R>,
    worker: Option<thread::JoinHandle<()>>,
    error: Arc<Mutex<Option<io::ErrorKind>>>,
}

impl<R: Read + Send + 'static> UnblockedReader<R> {
    /// Spawn a new unblocked reader.
    pub fn new(mut source: R, pipe_capacity: usize) -> io::Result<Self> {
        fault(Failure::ReaderWorker)?;
        // Create a new pipe.
        let (reader, mut writer) = pipe(pipe_capacity);
        let interest = Arc::new(Registration {
            interest: Mutex::<Option<Interest>>::new(None),
            end: PipeEnd::Reader,
            drain_directly: AtomicBool::new(false),
        });

        let error = Arc::new(Mutex::new(None));
        let source_error = error.clone();
        let cleanup = interest.clone();

        // Spawn the reader thread.
        let worker =
            thread::Builder::new().name("alacritty-tty-reader-thread".into()).spawn(move || {
                let _completion = ReaderCompletion(source_error.clone());
                let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
                let mut context = Context::from_waker(&waker);

                loop {
                    // Read from the reader into the pipe.
                    match writer.poll_fill(&mut context, &mut source) {
                        Poll::Ready(Ok(0)) => {
                            // Either the pipe is closed or the reader is at its EOF.
                            // In any case, we are done.
                            return;
                        },

                        Poll::Ready(Ok(_)) => {
                            // Keep reading.
                            continue;
                        },

                        Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {
                            // We were interrupted; continue.
                            continue;
                        },

                        Poll::Ready(Err(e)) => {
                            // BrokenPipe is normal ConPTY EOF. Other read failures
                            // must reach the I/O loop, not masquerade as a clean drain.
                            if e.kind() != io::ErrorKind::BrokenPipe {
                                *source_error.lock().unwrap() = Some(e.kind());
                            }
                            log::error!("error writing to pipe: {}", e);
                            return;
                        },

                        Poll::Pending => {
                            // If a close thread cannot start, keep serving the OS
                            // output pipe on this existing reader. Closing the
                            // source early can make OpenConsole exit before it
                            // sends close events to attached descendants.
                            if cleanup.drain_directly.load(Ordering::Acquire) {
                                if let Err(error) = io::copy(&mut source, &mut io::sink()) {
                                    if error.kind() != io::ErrorKind::BrokenPipe {
                                        *source_error.lock().unwrap() = Some(error.kind());
                                    }
                                }
                                return;
                            }
                            // An unpark permit closes the flag-check/park race.
                            thread::park();
                        },
                    }
                }
            })?;

        Ok(Self {
            interest,
            pipe: reader,
            first_register: true,
            _reader: PhantomData,
            worker: Some(worker),
            error,
        })
    }

    /// Register interest in the reader.
    pub fn register(&mut self, poller: &Arc<Poller>, event: Event, mode: PollMode) {
        let mut interest = self.interest.interest.lock().unwrap();
        *interest = Some(Interest { event, poller: poller.clone(), mode });

        // Send the event to start off with if we have any data.
        if (!self.pipe.is_empty() && event.readable) || self.first_register {
            self.first_register = false;
            poller.post(CompletionPacket::new(event)).ok();
        }
    }

    /// Deregister interest in the reader.
    pub fn deregister(&self) {
        let mut interest = self.interest.interest.lock().unwrap();
        *interest = None;
    }

    /// Try to read from the reader.
    pub fn try_read(&mut self, buf: &mut [u8]) -> usize {
        let waker = Waker::from(self.interest.clone());

        match self.pipe.poll_drain_bytes(&mut Context::from_waker(&waker), buf) {
            Poll::Pending => 0,
            Poll::Ready(n) => n,
        }
    }
}

impl<R> UnblockedReader<R> {
    /// Cleanup-only fallback: bypass piper backpressure on the existing OS
    /// reader so synchronous ConPTY close can finish terminating its clients.
    pub fn drain_without_buffering(&mut self) {
        self.interest.drain_directly.store(true, Ordering::Release);
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }

    /// Cleanup-only: keep the OS reader moving while ClosePseudoConsole waits.
    pub fn drain_and_join(&mut self) {
        let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
        let mut context = Context::from_waker(&waker);
        let mut buf = [0; 8192];
        loop {
            match self.pipe.poll_drain_bytes(&mut context, &mut buf) {
                Poll::Ready(0) => break,
                Poll::Ready(_) => (),
                Poll::Pending => thread::park(),
            }
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl<R: Read + Send + 'static> Read for UnblockedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let waker = Waker::from(self.interest.clone());
        match self.pipe.poll_drain_bytes(&mut Context::from_waker(&waker), buf) {
            Poll::Pending => Err(io::ErrorKind::WouldBlock.into()),
            Poll::Ready(0) => match *self.error.lock().unwrap() {
                Some(kind) => Err(kind.into()),
                None => Ok(0),
            },
            Poll::Ready(n) => Ok(n),
        }
    }
}

/// Poll a writer in another thread.
pub struct UnblockedWriter<W> {
    /// The interest to send about completion.
    interest: Arc<Registration>,

    /// The pipe that we are writing to.
    pipe: Writer,

    written: Arc<WriterProgress>,

    /// We logically own the writer, but we don't actually use it.
    _reader: PhantomData<W>,
    worker: Option<thread::JoinHandle<()>>,
}

impl<W: Write + Send + 'static> UnblockedWriter<W> {
    /// Spawn a new unblocked writer.
    pub fn new(mut sink: W, pipe_capacity: usize) -> io::Result<Self> {
        fault(Failure::WriterWorker)?;
        // Create a new pipe.
        let (mut reader, writer) = pipe(pipe_capacity);
        let written = Arc::new(WriterProgress::default());
        let writer_counter = written.clone();
        let interest = Arc::new(Registration {
            interest: Mutex::<Option<Interest>>::new(None),
            end: PipeEnd::Writer,
            drain_directly: AtomicBool::new(false),
        });

        let completion = WriterCompletion { progress: written.clone(), interest: interest.clone() };

        // Spawn the writer thread.
        let worker =
            thread::Builder::new().name("alacritty-tty-writer-thread".into()).spawn(move || {
                let _completion = completion;
                let mut sink = CountingWriter { sink: &mut sink, written: writer_counter };
                let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
                let mut context = Context::from_waker(&waker);

                loop {
                    // Write from the pipe into the writer.
                    match reader.poll_drain(&mut context, &mut sink) {
                        Poll::Ready(Ok(0)) => {
                            // Either the pipe is closed or the writer is full.
                            // In any case, we are done.
                            return;
                        },

                        Poll::Ready(Ok(_)) => {
                            // Keep writing.
                            continue;
                        },

                        Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {
                            // We were interrupted; continue.
                            continue;
                        },

                        Poll::Ready(Err(e)) => {
                            log::error!("error writing to pipe: {}", e);
                            return;
                        },

                        Poll::Pending => {
                            // We are now waiting on the other end to advance. Park the
                            // thread until they do.
                            thread::park();
                        },
                    }
                }
            })?;

        Ok(Self { interest, pipe: writer, written, _reader: PhantomData, worker: Some(worker) })
    }

    pub fn writer_progress(&self) -> Arc<WriterProgress> {
        self.written.clone()
    }

    /// Register interest in the writer.
    pub fn register(&self, poller: &Arc<Poller>, event: Event, mode: PollMode) {
        let mut interest = self.interest.interest.lock().unwrap();
        *interest = Some(Interest { event, poller: poller.clone(), mode });

        // Send the event to start off with if we have room for data.
        if !self.pipe.is_full() && event.writable {
            poller.post(CompletionPacket::new(event)).ok();
        }
    }

    /// Deregister interest in the writer.
    pub fn deregister(&self) {
        let mut interest = self.interest.interest.lock().unwrap();
        *interest = None;
    }

    /// Try to write to the writer.
    pub fn try_write(&mut self, buf: &[u8]) -> usize {
        let waker = Waker::from(self.interest.clone());

        match self.pipe.poll_fill_bytes(&mut Context::from_waker(&waker), buf) {
            Poll::Pending => 0,
            Poll::Ready(n) => n,
        }
    }
}

impl<W> UnblockedWriter<W> {
    pub fn close_and_join(&mut self) {
        // Close piper before joining a parked writer. The ConPTY has already
        // closed, so any in-progress OS write is released too.
        let (_, closed) = pipe(1);
        drop(std::mem::replace(&mut self.pipe, closed));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl<W: Write + Send + 'static> Write for UnblockedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.pipe.is_closed() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        Ok(self.try_write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        // Nothing to flush.
        Ok(())
    }
}

struct ThreadWaker(thread::Thread);

// Publish an abnormal source end before piper closes and wakes its reader.
// Otherwise a source panic would be indistinguishable from a clean EOF.
struct ReaderCompletion(Arc<Mutex<Option<io::ErrorKind>>>);
impl Drop for ReaderCompletion {
    fn drop(&mut self) {
        if thread::panicking() {
            *self.0.lock().unwrap_or_else(|error| error.into_inner()) = Some(io::ErrorKind::Other);
        }
    }
}

struct WriterCompletion {
    progress: Arc<WriterProgress>,
    interest: Arc<Registration>,
}

impl Drop for WriterCompletion {
    fn drop(&mut self) {
        self.progress.finish();
        // Failure must wake the PTY loop even if all input was transferred to
        // piper and writable interest is currently disabled.
        let interest = self.interest.interest.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(interest) = interest.as_ref() {
            let _ = interest.poller.notify();
        }
    }
}

/// Count only successful OS writes, not admission to the piper buffer. Partial
/// writes and errors after earlier writes retain the exact consumed count.
struct CountingWriter<W> {
    sink: W,
    written: Arc<WriterProgress>,
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self.sink.write(bytes)?;
        self.written.acknowledge(count);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.sink.flush()
    }
}

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

impl Wake for Registration {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let mut interest_lock = self.interest.lock().unwrap();
        if let Some(interest) = interest_lock.as_ref() {
            // Send the event to the poller.
            let send_event = match self.end {
                PipeEnd::Reader => interest.event.readable,
                PipeEnd::Writer => interest.event.writable,
            };

            if send_event {
                interest.poller.post(CompletionPacket::new(interest.event)).ok();

                // Clear the event if we're in oneshot mode.
                if matches!(interest.mode, PollMode::Oneshot | PollMode::EdgeOneshot) {
                    *interest_lock = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
