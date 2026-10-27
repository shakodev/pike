//! The main event loop which performs I/O on the pseudoterminal.

use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};
use std::fs::File;
use std::io::{self, ErrorKind, Read, Write};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use log::error;
use polling::{Event as PollingEvent, Events, PollMode};

use crate::event::{self, Event, EventListener, WindowSize};
use crate::input::{InputError, InputQueue, InputStatus, WriterProgress};
use crate::sync::FairMutex;
use crate::term::Term;
use crate::{thread, tty};
use vte::ansi;

/// Max bytes to read from the PTY before forced terminal synchronization.
pub(crate) const READ_BUFFER_SIZE: usize = 0x10_0000;

/// Max bytes to read from the PTY while the terminal is locked.
const MAX_LOCKED_READ: usize = u16::MAX as usize;

/// Messages that may be sent to the `EventLoop`.
#[derive(Debug)]
pub enum Msg {
    /// Data that should be written to the PTY.
    Input(Cow<'static, [u8]>),

    /// Indicates that the `EventLoop` should shut down, as Alacritty is shutting down.
    Shutdown,

    /// Instruction to resize the PTY.
    Resize(WindowSize),
}

/// Why the I/O loop stopped. Payloads contain no terminal text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitReason {
    Requested,
    ChildExit,
    IoError(ErrorKind),
    Panicked,
}

/// Published only after the disposing worker has dropped the PTY and parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Completion {
    pub reason: ExitReason,
    pub exit_code: Option<i32>,
    /// True only when natural-exit draining reached EOF.
    pub output_drained: bool,
}

#[derive(PartialEq)]
enum ReadStatus {
    Pending,
    More,
    Eof,
}

/// The main event loop.
///
/// Handles all the PTY I/O and runs the PTY parser which updates terminal
/// state.
pub struct EventLoop<T: tty::EventedPty, U: EventListener> {
    poll: Arc<polling::Poller>,
    pty: T,
    rx: PeekableReceiver<Msg>,
    tx: Sender<Msg>,
    terminal: Arc<FairMutex<Term<U>>>,
    event_proxy: U,
    drain_on_exit: bool,
    ref_test: bool,
    input: InputQueue,
    count_writes: bool,
    shutdown_requested: Arc<AtomicBool>,
}

impl<T, U> EventLoop<T, U>
where
    T: tty::EventedPty + event::OnResize + Send + 'static,
    U: EventListener + Send + 'static,
{
    /// Create a new event loop.
    pub fn new(
        terminal: Arc<FairMutex<Term<U>>>,
        event_proxy: U,
        pty: T,
        drain_on_exit: bool,
        ref_test: bool,
    ) -> io::Result<EventLoop<T, U>> {
        let (tx, rx) = mpsc::channel();
        let poll = match polling::Poller::new() {
            Ok(poll) => Arc::new(poll),
            Err(error) => {
                // The caller may be the UI thread and already owns a PTY.
                // Its destructor can block; failed preparation must dispose
                // that PTY on a transient cleanup worker as well.
                drop(thread::spawn_named("PTY failed preparation", move || drop(pty)));
                return Err(error);
            },
        };
        let counter = pty.input_writer_progress();
        let count_writes = counter.is_none();
        let input = InputQueue::new(counter.unwrap_or_else(|| Arc::new(WriterProgress::default())));
        Ok(EventLoop {
            poll,
            pty,
            tx,
            rx: PeekableReceiver::new(rx),
            terminal,
            event_proxy,
            drain_on_exit,
            ref_test,
            input,
            count_writes,
            shutdown_requested: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn channel(&self) -> EventLoopSender {
        EventLoopSender {
            sender: self.tx.clone(),
            poller: Arc::downgrade(&self.poll),
            input: self.input.clone(),
            shutdown_requested: self.shutdown_requested.clone(),
        }
    }

    /// Drain the channel.
    ///
    /// Returns `false` when a shutdown message was received.
    fn drain_recv_channel(&mut self, read_eof: bool) -> bool {
        while let Some(msg) = self.rx.recv() {
            if self.shutdown_requested.load(Ordering::Acquire)
                || (!read_eof && self.input.is_closed())
            {
                return false;
            }
            match msg {
                Msg::Input(_) => unreachable!("input is admitted through the bounded queue"),
                Msg::Resize(_) if read_eof => (),
                Msg::Resize(window_size) => self.pty.on_resize(window_size),
                Msg::Shutdown => return false,
            }
        }

        true
    }

    #[inline]
    fn pty_read<X>(
        &mut self,
        state: &mut State,
        buf: &mut [u8],
        mut writer: Option<&mut X>,
    ) -> io::Result<ReadStatus>
    where
        X: Write,
    {
        let mut processed = 0;
        loop {
            let count = match self.pty.reader().read(&mut buf[..MAX_LOCKED_READ]) {
                Ok(0) => return Ok(ReadStatus::Eof),
                Ok(count) => count,
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) if err.kind() == ErrorKind::WouldBlock => return Ok(ReadStatus::Pending),
                // Linux reports PTY hangup as EIO rather than EOF.
                #[cfg(target_os = "linux")]
                Err(err) if err.raw_os_error() == Some(libc::EIO) => return Ok(ReadStatus::Eof),
                Err(err) => return Err(err),
            };
            if let Some(writer) = &mut writer {
                writer.write_all(&buf[..count])?;
            }
            // Parse each successful read before another read can fail. Keep the
            // lock bounded to one chunk and never retain unparsed trailing data.
            {
                let mut terminal = self.terminal.lock();
                state.parser.advance(&mut *terminal, &buf[..count]);
            }
            processed += count;
            if state.parser.sync_bytes_count() < count {
                self.event_proxy.send_event(Event::Wakeup);
            }
            if processed >= MAX_LOCKED_READ {
                return Ok(ReadStatus::More);
            }
        }
    }

    #[inline]
    fn pty_write(&mut self, state: &mut State) -> io::Result<()> {
        state.ensure_next(&self.input);

        'write_many: while let Some(mut current) = state.take_current() {
            'write_one: loop {
                if self.input.is_closed() {
                    return Ok(());
                }
                match self.pty.writer().write(current.remaining_bytes()) {
                    Ok(0) => {
                        state.set_current(Some(current));
                        break 'write_many;
                    },
                    Ok(n) => {
                        if self.count_writes {
                            self.input.acknowledge(n);
                        }
                        current.advance(n);
                        if current.finished() {
                            state.goto_next(&self.input);
                            break 'write_one;
                        }
                    },
                    Err(err) => {
                        state.set_current(Some(current));
                        match err.kind() {
                            ErrorKind::Interrupted | ErrorKind::WouldBlock => break 'write_many,
                            _ => return Err(err),
                        }
                    },
                }
            }
        }

        Ok(())
    }

    fn prepare_io_turn(&mut self, state: &mut State, events: &Events, now: Instant) -> bool {
        // Sync expiration must not depend on input being empty. Retaining an
        // expired deadline under backpressure would make subsequent waits
        // return immediately instead of sleeping for writer capacity.
        if state.parser.sync_timeout().sync_timeout().is_some_and(|deadline| deadline <= now) {
            state.parser.stop_sync(&mut *self.terminal.lock());
            self.event_proxy.send_event(Event::Wakeup);
        }
        !events.is_empty() || self.rx.peek().is_some() || state.needs_write(&self.input)
    }

    fn finish_io_turn(
        &mut self,
        state: &mut State,
        interest: &mut PollingEvent,
        read_eof: bool,
    ) -> io::Result<()> {
        if read_eof {
            return Ok(());
        }
        // Input admission wakes Poller::wait without a readiness event. On
        // Windows, piper keeps a writer waker only while full, so a previous
        // writable event is not continuously level-triggered. Service pending
        // bytes on this wake even when writable interest is already enabled.
        // The PTY writer is nonblocking; backpressure retains current bytes
        // and waits for capacity readiness, with no timer or polling loop.
        if state.needs_write(&self.input) {
            self.pty_write(state)?;
        }
        let needs_write = state.needs_write(&self.input);
        if needs_write != interest.writable {
            interest.writable = needs_write;
            self.pty.reregister(&self.poll, *interest, PollMode::Level)?;
        }
        Ok(())
    }

    /// Compatibility entry point: ownership of returned backend state belongs
    /// to the caller. UI adapters should use `spawn_disposing` instead.
    pub fn spawn(self) -> JoinHandle<(Self, State)> {
        thread::spawn_named("PTY reader", move || {
            let (event_loop, state, _) = self.run();
            (event_loop, state)
        })
    }

    /// Run and dispose on the existing PTY worker. The handle retains only (),
    /// never completed EventLoop/PTY state. No UI join or extra waiting worker.
    pub fn spawn_disposing(
        self,
        stopped: impl FnOnce() + Send + 'static,
        complete: impl FnOnce(Completion) + Send + 'static,
    ) {
        drop(thread::spawn_named("PTY reader", move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let (event_loop, state, completion) = self.run();
                stopped();
                drop(state);
                drop(event_loop);
                completion
            }));
            complete(result.unwrap_or(Completion {
                reason: ExitReason::Panicked,
                exit_code: None,
                output_drained: false,
            }));
        }));
    }

    fn run(mut self) -> (Self, State, Completion) {
        let mut state = State::default();
        let mut buf = [0u8; READ_BUFFER_SIZE];
        let mut completion =
            Completion { reason: ExitReason::Requested, exit_code: None, output_drained: false };
        let poll_opts = PollMode::Level;
        let mut interest = PollingEvent::readable(0);

        // SAFETY: self owns the PTY until after deregistration below.
        if let Err(err) = unsafe { self.pty.register(&self.poll, interest, poll_opts) } {
            completion.reason = ExitReason::IoError(err.kind());
            let _ = self.pty.deregister(&self.poll);
            return (self, state, completion);
        }
        let mut events = Events::with_capacity(NonZeroUsize::new(1024).unwrap());
        let mut pipe = if self.ref_test {
            Some(File::create("./alacritty.recording").expect("create alacritty recording"))
        } else {
            None
        };

        let mut read_eof = false;
        let mut read_pending = false;
        'event_loop: loop {
            if self.shutdown_requested.load(Ordering::Acquire)
                || (!read_eof && self.input.is_closed())
            {
                if !self.shutdown_requested.load(Ordering::Acquire) {
                    completion.reason = ExitReason::IoError(ErrorKind::BrokenPipe);
                }
                break;
            }
            let handler = state.parser.sync_timeout();
            let timeout =
                handler.sync_timeout().map(|st| st.saturating_duration_since(Instant::now()));
            events.clear();
            // One bounded next turn, not an idle timer or a polling loop.
            // Re-notify after Interrupted too: pending work must never rely on
            // an already-consumed notification. An exact-budget read gets a
            // final attempt to reach Pending and arm the reader's waker.
            if read_pending {
                if let Err(err) = self.poll.notify() {
                    completion.reason = ExitReason::IoError(err.kind());
                    break;
                }
            }
            if let Err(err) = self.poll.wait(&mut events, timeout) {
                if err.kind() == ErrorKind::Interrupted {
                    continue;
                }
                completion.reason = ExitReason::IoError(err.kind());
                break;
            }
            if self.shutdown_requested.load(Ordering::Acquire)
                || (!read_eof && self.input.is_closed())
            {
                continue;
            }
            // Prepare deadlines on every turn, including productive buffered
            // reads; a pending continuation must not skip sync expiration.
            let has_work = self.prepare_io_turn(&mut state, &events, Instant::now());
            if !has_work && !read_pending {
                continue;
            }
            if !self.drain_recv_channel(read_eof) {
                break;
            }
            // Buffered completion sources need not produce another readiness
            // event after a capped read. Continue on the existing worker while
            // still servicing channel, write and child events between turns.
            let continuation = std::mem::take(&mut read_pending)
                .then_some(PollingEvent::readable(tty::PTY_READ_WRITE_TOKEN));
            let mut read_serviced = false;
            for event in events.iter().chain(continuation) {
                match event.key {
                    tty::PTY_CHILD_EVENT_TOKEN => {
                        if let Some(tty::ChildEvent::Exited(code)) = self.pty.next_child_event() {
                            self.input.close();
                            completion.reason = ExitReason::ChildExit;
                            completion.exit_code = code;
                            if let Some(code) = code {
                                self.event_proxy.send_event(Event::ChildExit(code));
                            }
                            if self.drain_on_exit {
                                // Closing ConPTY must run concurrently with draining its
                                // output. Other PTYs need no special close preparation.
                                let drained = self.pty.begin_shutdown().and_then(|()| {
                                    if read_eof {
                                        Ok(())
                                    } else {
                                        self.drain_output(&mut state, &mut buf, pipe.as_mut())
                                    }
                                });
                                match drained {
                                    Ok(()) => completion.output_drained = true,
                                    Err(err) => completion.reason = ExitReason::IoError(err.kind()),
                                }
                            }
                            state.parser.stop_sync(&mut *self.terminal.lock());
                            self.terminal.lock().exit();
                            self.event_proxy.send_event(Event::Wakeup);
                            break 'event_loop;
                        }
                    },
                    tty::PTY_READ_WRITE_TOKEN => {
                        if read_eof {
                            continue;
                        }
                        if !read_serviced && (event.readable || event.is_interrupt()) {
                            read_serviced = true;
                            match self.pty_read(&mut state, &mut buf, pipe.as_mut()) {
                                Ok(ReadStatus::Eof) => {
                                    // EOF/HUP remains level-triggered until removed.
                                    // Keep only the child watcher and shutdown wake
                                    // alive, preserving a later exit code without spin.
                                    read_eof = true;
                                    self.input.close();
                                    if let Err(err) = self.pty.deregister_io(&self.poll) {
                                        completion.reason = ExitReason::IoError(err.kind());
                                        break 'event_loop;
                                    }
                                    continue;
                                },
                                Ok(ReadStatus::More) => read_pending = true,
                                Ok(ReadStatus::Pending) => (),
                                Err(err) => {
                                    error!("Error reading from PTY in event loop: {err}");
                                    completion.reason = ExitReason::IoError(err.kind());
                                    break 'event_loop;
                                },
                            }
                        }
                    },
                    _ => (),
                }
            }
            if let Err(err) = self.finish_io_turn(&mut state, &mut interest, read_eof) {
                error!("Error servicing PTY input in event loop: {err}");
                completion.reason = ExitReason::IoError(err.kind());
                break;
            }
        }
        if completion.reason == ExitReason::Requested
            && !self.shutdown_requested.load(Ordering::Acquire)
        {
            completion.reason = ExitReason::IoError(ErrorKind::BrokenPipe);
        }
        self.input.close();
        // Successful reads can still be buffered by synchronized updates when
        // a later read/write fails or close is requested. Preserve those bytes
        // in the retained model before dropping the parser, even without Exit.
        if state.parser.sync_timeout().sync_timeout().is_some() {
            state.parser.stop_sync(&mut *self.terminal.lock());
            self.event_proxy.send_event(Event::Wakeup);
        }
        let _ = self.pty.deregister(&self.poll);
        (self, state, completion)
    }

    fn drain_output<X: Write>(
        &mut self,
        state: &mut State,
        buf: &mut [u8],
        mut pipe: Option<&mut X>,
    ) -> io::Result<()> {
        // A misbehaving descendant must not keep the parser running forever.
        // Destruction may still block in the OS; completion then stays pending.
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut events = Events::new();
        self.pty.reregister(&self.poll, PollingEvent::readable(0), PollMode::Level)?;
        loop {
            if Instant::now() >= deadline {
                return Err(ErrorKind::TimedOut.into());
            }
            match self.pty_read(state, buf, pipe.as_deref_mut())? {
                ReadStatus::Eof => return Ok(()),
                ReadStatus::More => continue,
                ReadStatus::Pending => {
                    events.clear();
                    match self
                        .poll
                        .wait(&mut events, Some(deadline.saturating_duration_since(Instant::now())))
                    {
                        Err(err) if err.kind() == ErrorKind::Interrupted => (),
                        result => {
                            result?;
                        },
                    }
                },
            }
        }
    }
}

impl<T: tty::EventedPty, U: EventListener> Drop for EventLoop<T, U> {
    fn drop(&mut self) {
        self.input.close();
    }
}

/// Helper type which tracks how much of a buffer has been written.
struct Writing {
    source: Cow<'static, [u8]>,
    written: usize,
}

pub struct Notifier(pub EventLoopSender);

impl event::Notify for Notifier {
    fn notify<B>(&self, bytes: B)
    where
        B: Into<Cow<'static, [u8]>>,
    {
        let bytes = bytes.into();
        // Terminal hangs if we send 0 bytes through.
        if bytes.is_empty() {
            return;
        }

        let _ = self.0.send(Msg::Input(bytes));
    }
}

impl event::OnResize for Notifier {
    fn on_resize(&mut self, window_size: WindowSize) {
        let _ = self.0.send(Msg::Resize(window_size));
    }
}

#[derive(Debug)]
pub enum EventLoopSendError {
    Input(InputError),
    /// Error polling the event loop.
    Io(io::Error),

    /// Error sending a message to the event loop.
    Send(mpsc::SendError<Msg>),
}

impl Display for EventLoopSendError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            EventLoopSendError::Input(err) => err.fmt(f),
            EventLoopSendError::Io(err) => err.fmt(f),
            EventLoopSendError::Send(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for EventLoopSendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EventLoopSendError::Input(err) => Some(err),
            EventLoopSendError::Io(err) => err.source(),
            EventLoopSendError::Send(err) => err.source(),
        }
    }
}

#[derive(Clone)]
pub struct EventLoopSender {
    sender: Sender<Msg>,
    poller: std::sync::Weak<polling::Poller>,
    input: InputQueue,
    shutdown_requested: Arc<AtomicBool>,
}

impl EventLoopSender {
    pub fn send(&self, msg: Msg) -> Result<(), EventLoopSendError> {
        match msg {
            Msg::Input(input) => return self.send_input(&input).map_err(EventLoopSendError::Input),
            Msg::Shutdown => {
                self.shutdown_requested.store(true, Ordering::Release);
                self.input.close();
                return self
                    .poller
                    .upgrade()
                    .map_or(Ok(()), |poller| poller.notify().map_err(EventLoopSendError::Io));
            },
            Msg::Resize(_) if self.input.is_closed() => {
                return Err(EventLoopSendError::Input(InputError::Closed));
            },
            Msg::Resize(_) => {},
        }
        self.sender.send(msg).map_err(EventLoopSendError::Send)?;
        self.poller
            .upgrade()
            .ok_or(EventLoopSendError::Input(InputError::Closed))?
            .notify()
            .map_err(EventLoopSendError::Io)
    }

    /// Atomically accept the entire input, or reject it before copying/queueing
    /// any bytes. Chunking preserves byte order, including split UTF-8 scalars.
    pub fn send_input(&self, bytes: &[u8]) -> Result<(), InputError> {
        self.input.send(bytes, || {
            self.poller.upgrade().ok_or_else(|| io::Error::from(ErrorKind::BrokenPipe))?.notify()
        })
    }

    /// Advisory preflight; send_input repeats this check atomically at admission.
    pub fn preflight_input(&self, bytes: usize) -> Result<(), InputError> {
        self.input.preflight(bytes)
    }

    pub fn input_status(&self) -> InputStatus {
        self.input.status()
    }
}

/// All of the mutable state needed to run the event loop.
///
/// Contains list of items to write, current write state, etc. Anything that
/// would otherwise be mutated on the `EventLoop` goes here.
#[derive(Default)]
pub struct State {
    writing: Option<Writing>,
    parser: ansi::Processor,
}

impl State {
    #[inline]
    fn ensure_next(&mut self, input: &InputQueue) {
        if self.writing.is_none() {
            self.goto_next(input);
        }
    }

    #[inline]
    fn goto_next(&mut self, input: &InputQueue) {
        self.writing = input.take().map(|bytes| Writing::new(Cow::Owned(bytes)));
    }

    #[inline]
    fn take_current(&mut self) -> Option<Writing> {
        self.writing.take()
    }

    #[inline]
    fn needs_write(&self, input: &InputQueue) -> bool {
        !input.is_closed() && (self.writing.is_some() || !input.is_empty())
    }

    #[inline]
    fn set_current(&mut self, new: Option<Writing>) {
        self.writing = new;
    }
}

impl Writing {
    #[inline]
    fn new(c: Cow<'static, [u8]>) -> Writing {
        Writing { source: c, written: 0 }
    }

    #[inline]
    fn advance(&mut self, n: usize) {
        self.written += n;
    }

    #[inline]
    fn remaining_bytes(&self) -> &[u8] {
        &self.source[self.written..]
    }

    #[inline]
    fn finished(&self) -> bool {
        self.written >= self.source.len()
    }
}

struct PeekableReceiver<T> {
    rx: Receiver<T>,
    peeked: Option<T>,
}

impl<T> PeekableReceiver<T> {
    fn new(rx: Receiver<T>) -> Self {
        Self { rx, peeked: None }
    }

    fn peek(&mut self) -> Option<&T> {
        if self.peeked.is_none() {
            self.peeked = self.rx.try_recv().ok();
        }

        self.peeked.as_ref()
    }

    fn recv(&mut self) -> Option<T> {
        if self.peeked.is_some() {
            self.peeked.take()
        } else {
            match self.rx.try_recv() {
                Err(TryRecvError::Disconnected) => panic!("event loop channel closed"),
                res => res.ok(),
            }
        }
    }
}

#[cfg(test)]
mod input_tests;

#[cfg(test)]
mod lifecycle_tests;

#[cfg(test)]
mod read_tests;
