use super::*;
use crate::term::test::TermSize;
use crate::tty::{ChildEvent, EventedPty, EventedReadWrite};
use std::sync::mpsc;
use std::thread::ThreadId;

#[derive(Clone)]
struct Listener;
impl EventListener for Listener {
    fn send_event(&self, _: Event) {}
}

struct TestReader {
    bytes: io::Cursor<Vec<u8>>,
    pending: bool,
}
impl Read for TestReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.pending {
            Err(ErrorKind::WouldBlock.into())
        } else {
            self.bytes.read(buffer)
        }
    }
}

struct FailedPty<W = io::Sink> {
    reader: TestReader,
    writer: W,
    dropping: mpsc::Sender<ThreadId>,
    release: mpsc::Receiver<()>,
    panic: bool,
}
impl<W> Drop for FailedPty<W> {
    fn drop(&mut self) {
        self.dropping.send(std::thread::current().id()).unwrap();
        self.release.recv_timeout(Duration::from_secs(5)).unwrap();
    }
}
impl<W> event::OnResize for FailedPty<W> {
    fn on_resize(&mut self, _: WindowSize) {}
}
impl<W: Write> EventedReadWrite for FailedPty<W> {
    type Reader = TestReader;
    type Writer = W;
    unsafe fn register(
        &mut self,
        _: &Arc<polling::Poller>,
        _: PollingEvent,
        _: PollMode,
    ) -> io::Result<()> {
        assert!(!self.panic, "synthetic worker panic");
        Err(ErrorKind::BrokenPipe.into())
    }
    fn reregister(
        &mut self,
        _: &Arc<polling::Poller>,
        _: PollingEvent,
        _: PollMode,
    ) -> io::Result<()> {
        Ok(())
    }
    fn deregister(&mut self, _: &Arc<polling::Poller>) -> io::Result<()> {
        Ok(())
    }
    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.reader
    }
    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.writer
    }
}
impl<W: Write> EventedPty for FailedPty<W> {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        None
    }
}

#[test]
fn input_admission_wake_progresses_with_writable_interest_already_enabled() {
    let (dropping, _dropped) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let terminal =
        Arc::new(FairMutex::new(Term::new(Default::default(), &TermSize::new(80, 24), Listener)));
    let pty = FailedPty {
        reader: TestReader { bytes: io::Cursor::new(Vec::new()), pending: true },
        writer: io::sink(),
        dropping,
        release: gate,
        panic: false,
    };
    let mut event_loop = EventLoop::new(terminal, Listener, pty, true, false).unwrap();
    let sender = event_loop.channel();
    let mut state = State::default();
    let mut interest = PollingEvent::readable(0);
    interest.writable = true;
    // Reproduce the observed turn boundary without a scheduling race: a
    // notify-only admission wake, queued bytes and already-enabled interest.
    // No separate writable event is supplied. The nonblocking sink is ready.
    sender.send_input(&[b'x'; 148]).unwrap();
    let mut notifications = Events::new();
    event_loop.poll.wait(&mut notifications, Some(Duration::from_secs(1))).unwrap();
    assert!(notifications.is_empty(), "admission notify is not a writable readiness event");
    event_loop.finish_io_turn(&mut state, &mut interest, false).unwrap();
    let status = sender.input_status();
    release.send(()).unwrap();
    assert_eq!(status.queued_bytes, 0, "admitted replies stranded after notification");
    assert_eq!(status.written_bytes, 148);
    assert_eq!(status.outstanding_bytes, 0);
}

struct ScriptedWriter {
    steps: std::collections::VecDeque<Result<usize, ErrorKind>>,
    bytes: Vec<u8>,
    calls: usize,
    blocked: Option<mpsc::Sender<()>>,
    release_error: Option<Arc<AtomicBool>>,
}
impl Write for ScriptedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls > 2
            && self.release_error.as_ref().is_some_and(|ready| !ready.load(Ordering::Acquire))
        {
            return Ok(0);
        }
        let count = self.steps.pop_front().unwrap_or(Ok(usize::MAX))?.min(bytes.len());
        self.bytes.extend_from_slice(&bytes[..count]);
        if count == 0
            && let Some(blocked) = self.blocked.take()
        {
            blocked.send(()).unwrap();
        }
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn admission_wake_keeps_bounded_order_and_returns_on_backpressure_without_idle_writes() {
    let (dropping, _dropped) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let terminal =
        Arc::new(FairMutex::new(Term::new(Default::default(), &TermSize::new(80, 24), Listener)));
    let pty = FailedPty {
        reader: TestReader { bytes: io::Cursor::new(Vec::new()), pending: true },
        writer: ScriptedWriter {
            steps: [Ok(2), Ok(0), Err(ErrorKind::WouldBlock)].into(),
            bytes: Vec::new(),
            calls: 0,
            blocked: None,
            release_error: None,
        },
        dropping,
        release: gate,
        panic: false,
    };
    let mut event_loop = EventLoop::new(terminal, Listener, pty, true, false).unwrap();
    let sender = event_loop.channel();
    let mut state = State::default();
    let mut interest = PollingEvent::readable(0);
    interest.writable = true;
    let mut expected = vec![b'x'; crate::input::INPUT_BYTE_LIMIT];
    sender.send_input(&expected).unwrap();
    release.send(()).unwrap();

    event_loop.finish_io_turn(&mut state, &mut interest, true).unwrap();
    assert_eq!(event_loop.pty.writer.calls, 0, "EOF must not admit late replies to the PTY");
    event_loop.finish_io_turn(&mut state, &mut interest, false).unwrap();
    assert_eq!(event_loop.pty.writer.calls, 2, "zero write must return, not spin");
    assert_eq!(sender.input_status().written_bytes, 2);
    sender.send_input(b"YZ").unwrap();
    assert!(matches!(sender.send_input(b"overflow"), Err(InputError::Full { .. })));

    event_loop.finish_io_turn(&mut state, &mut interest, false).unwrap();
    assert_eq!(event_loop.pty.writer.calls, 3, "WouldBlock must return, not spin");
    assert_eq!(sender.input_status().written_bytes, 2);
    event_loop.finish_io_turn(&mut state, &mut interest, false).unwrap();
    expected.extend_from_slice(b"YZ");
    assert_eq!(event_loop.pty.writer.bytes, expected);
    assert_eq!(sender.input_status().outstanding_bytes, 0);
    assert_eq!(sender.input_status().queued_bytes, 0);
    assert!(!interest.writable);
    let calls = event_loop.pty.writer.calls;
    event_loop.finish_io_turn(&mut state, &mut interest, false).unwrap();
    assert_eq!(event_loop.pty.writer.calls, calls, "idle wake must not call the writer");
    sender.send(Msg::Shutdown).unwrap();
    event_loop.finish_io_turn(&mut state, &mut interest, false).unwrap();
    assert_eq!(event_loop.pty.writer.calls, calls, "closed session must not write");
    assert_eq!(sender.send_input(b"late"), Err(InputError::Closed));
}

struct NotifyOnlyPty {
    reader: TestReader,
    writer: ScriptedWriter,
    disposed: mpsc::Sender<ThreadId>,
}
impl Drop for NotifyOnlyPty {
    fn drop(&mut self) {
        self.disposed.send(std::thread::current().id()).unwrap();
    }
}
impl event::OnResize for NotifyOnlyPty {
    fn on_resize(&mut self, _: WindowSize) {}
}
impl EventedReadWrite for NotifyOnlyPty {
    type Reader = TestReader;
    type Writer = ScriptedWriter;
    unsafe fn register(
        &mut self,
        poll: &Arc<polling::Poller>,
        _: PollingEvent,
        _: PollMode,
    ) -> io::Result<()> {
        poll.notify()
    }
    fn reregister(
        &mut self,
        _: &Arc<polling::Poller>,
        _: PollingEvent,
        _: PollMode,
    ) -> io::Result<()> {
        Ok(())
    }
    fn deregister(&mut self, _: &Arc<polling::Poller>) -> io::Result<()> {
        Ok(())
    }
    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.reader
    }
    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.writer
    }
}
impl EventedPty for NotifyOnlyPty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        None
    }
}

#[test]
fn notify_only_wake_resumes_partial_write_then_reports_error_after_disposal() {
    let (disposed, disposal) = mpsc::channel();
    let (blocked, waiting) = mpsc::channel();
    let release_error = Arc::new(AtomicBool::new(false));
    let terminal =
        Arc::new(FairMutex::new(Term::new(Default::default(), &TermSize::new(80, 24), Listener)));
    let pty = NotifyOnlyPty {
        reader: TestReader { bytes: io::Cursor::new(Vec::new()), pending: true },
        writer: ScriptedWriter {
            steps: [Ok(2), Ok(0), Err(ErrorKind::BrokenPipe)].into(),
            bytes: Vec::new(),
            calls: 0,
            blocked: Some(blocked),
            release_error: Some(release_error.clone()),
        },
        disposed,
    };
    let event_loop = EventLoop::new(terminal, Listener, pty, true, false).unwrap();
    let sender = event_loop.channel();
    sender.send_input(b"abcde").unwrap();
    let (complete, completed) = mpsc::channel();
    event_loop.spawn_disposing(|| {}, move |result| complete.send(result).unwrap());
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    // All remaining bytes belong to the retained partial write, with no
    // queued input or control message. A bare notification must service them.
    let status = sender.input_status();
    assert_eq!(status.written_bytes, 2);
    assert_eq!(status.queued_bytes, 0);
    assert_eq!(status.outstanding_bytes, 3);
    release_error.store(true, Ordering::Release);
    sender.poller.upgrade().unwrap().notify().unwrap();
    let result = completed.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(result.reason, ExitReason::IoError(ErrorKind::BrokenPipe));
    assert!(!result.output_drained);
    assert_eq!(result.exit_code, None);
    let status = sender.input_status();
    assert!(status.closed);
    assert_eq!(status.written_bytes, 2);
    assert_eq!(status.cancelled_bytes, 3);
    assert_eq!(status.outstanding_bytes, 0);
    assert_eq!(status.queued_bytes, 0);
    assert_eq!(sender.send_input(b"late"), Err(InputError::Closed));
    assert_ne!(disposal.recv_timeout(Duration::from_secs(5)).unwrap(), std::thread::current().id());
    assert!(sender.poller.upgrade().is_none());
}

#[test]
fn expired_sync_deadline_is_cleared_while_partial_input_remains_blocked() {
    let (dropping, _dropped) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let terminal =
        Arc::new(FairMutex::new(Term::new(Default::default(), &TermSize::new(80, 24), Listener)));
    let pty = FailedPty {
        reader: TestReader { bytes: io::Cursor::new(Vec::new()), pending: true },
        writer: ScriptedWriter {
            steps: [Ok(0)].into(),
            bytes: Vec::new(),
            calls: 0,
            blocked: None,
            release_error: None,
        },
        dropping,
        release: gate,
        panic: false,
    };
    let mut event_loop = EventLoop::new(terminal.clone(), Listener, pty, true, false).unwrap();
    let sender = event_loop.channel();
    let mut state = State::default();
    state.parser.advance(&mut *terminal.lock(), b"\x1b[?2026hSYNC_WAIT");
    let deadline = state.parser.sync_timeout().sync_timeout().unwrap();
    sender.send_input(b"x").unwrap();
    state.ensure_next(&event_loop.input);
    assert_eq!(sender.input_status().queued_bytes, 0);
    assert!(state.needs_write(&event_loop.input));
    assert!(event_loop.prepare_io_turn(&mut state, &Events::new(), deadline));
    let mut interest = PollingEvent::readable(0);
    interest.writable = true;
    event_loop.finish_io_turn(&mut state, &mut interest, false).unwrap();
    release.send(()).unwrap();
    assert!(
        state.parser.sync_timeout().sync_timeout().is_none(),
        "an expired deadline must not force zero-timeout polls under input backpressure"
    );
    assert_eq!(event_loop.pty.writer.calls, 1);
    assert_eq!(sender.input_status().outstanding_bytes, 1);
    let text: String = terminal.lock().grid().display_iter().map(|cell| cell.c).collect();
    assert!(text.contains("SYNC_WAIT"));
}

#[derive(Clone)]
struct WakeCounter(Arc<std::sync::atomic::AtomicUsize>);
impl EventListener for WakeCounter {
    fn send_event(&self, event: Event) {
        if matches!(event, Event::Wakeup) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn actual_readiness_batch(poll: &Arc<polling::Poller>) -> Events {
    struct Registration {
        socket: std::net::TcpStream,
        poll: Arc<polling::Poller>,
    }
    impl Drop for Registration {
        fn drop(&mut self) {
            let _ = self.poll.delete(&self.socket);
        }
    }
    let (socket, mut peer) = socket_pair();
    let source = Registration { socket, poll: poll.clone() };
    // SAFETY: Registration removes its owned socket before the socket drops,
    // including when a test assertion unwinds.
    unsafe { poll.add_with_mode(&source.socket, PollingEvent::readable(77), PollMode::Level) }
        .unwrap();
    peer.write_all(b"ready").unwrap();
    let mut events = Events::new();
    let deadline = Instant::now() + Duration::from_secs(1);
    while events.is_empty() {
        // Poller permits spurious/notification-only returns. Wait on the
        // actual ready socket with one bounded deadline instead of assuming
        // the first wake contains readiness.
        poll.wait(&mut events, Some(deadline.saturating_duration_since(Instant::now()))).unwrap();
        assert!(
            !events.is_empty() || Instant::now() < deadline,
            "fixture must provide actual socket readiness"
        );
    }
    events
}

#[test]
fn sync_deadlines_respect_future_and_expiry_for_idle_queued_partial_and_readiness() {
    for expired in [false, true] {
        for input in 0..3 {
            for readiness in [false, true] {
                let (dropping, _dropped) = mpsc::channel();
                let (release, gate) = mpsc::channel();
                let wakeups = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let listener = WakeCounter(wakeups.clone());
                let terminal = Arc::new(FairMutex::new(Term::new(
                    Default::default(),
                    &TermSize::new(80, 24),
                    listener.clone(),
                )));
                let pty = FailedPty {
                    reader: TestReader { bytes: io::Cursor::new(Vec::new()), pending: true },
                    writer: ScriptedWriter {
                        steps: [Ok(0)].into(),
                        bytes: Vec::new(),
                        calls: 0,
                        blocked: None,
                        release_error: None,
                    },
                    dropping,
                    release: gate,
                    panic: false,
                };
                let mut event_loop =
                    EventLoop::new(terminal.clone(), listener, pty, true, false).unwrap();
                let sender = event_loop.channel();
                let mut state = State::default();
                state.parser.advance(&mut *terminal.lock(), b"\x1b[?2026hSYNC_MATRIX");
                let deadline = state.parser.sync_timeout().sync_timeout().unwrap();
                if input != 0 {
                    sender.send_input(b"x").unwrap();
                    if input == 2 {
                        state.ensure_next(&event_loop.input);
                    }
                } else {
                    event_loop.poll.notify().unwrap();
                }
                let mut events = Events::new();
                event_loop.poll.wait(&mut events, Some(Duration::from_secs(1))).unwrap();
                assert!(events.is_empty(), "admission/idle notification has no readiness event");
                if readiness {
                    events = actual_readiness_batch(&event_loop.poll);
                }
                let before = wakeups.load(Ordering::Relaxed);
                // Inject the observation instant to avoid scheduling-dependent
                // sleeps around vte's real 150ms deadline.
                let now = if expired { deadline } else { deadline - Duration::from_nanos(1) };
                let active = event_loop.prepare_io_turn(&mut state, &events, now);
                release.send(()).unwrap();
                assert_eq!(active, readiness || input != 0);
                assert_eq!(state.parser.sync_timeout().sync_timeout().is_none(), expired);
                assert_eq!(wakeups.load(Ordering::Relaxed) - before, usize::from(expired));
                let text: String =
                    terminal.lock().grid().display_iter().map(|cell| cell.c).collect();
                assert_eq!(text.contains("SYNC_MATRIX"), expired);
                if active {
                    let mut interest = PollingEvent::readable(0);
                    interest.writable = true;
                    event_loop.finish_io_turn(&mut state, &mut interest, false).unwrap();
                }
                assert_eq!(event_loop.pty.writer.calls, usize::from(input != 0));
                assert_eq!(sender.input_status().outstanding_bytes, usize::from(input != 0));
            }
        }
    }
}

#[test]
fn io_failure_and_unwind_complete_only_after_backend_disposal_on_worker() {
    for panic in [false, true] {
        let (dropping, dropped) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let terminal = Arc::new(FairMutex::new(Term::new(
            Default::default(),
            &TermSize::new(80, 24),
            Listener,
        )));
        let pty = FailedPty {
            reader: TestReader { bytes: io::Cursor::new(Vec::new()), pending: false },
            writer: io::sink(),
            dropping,
            release: gate,
            panic,
        };
        let event_loop = EventLoop::new(terminal, Listener, pty, true, false).unwrap();
        let sender = event_loop.channel();
        let (complete, completed) = mpsc::channel();
        let (source_stopped, source_stop) = mpsc::channel();
        event_loop.spawn_disposing(
            move || source_stopped.send(()).unwrap(),
            move |result| complete.send(result).unwrap(),
        );
        let worker = dropped.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_ne!(worker, std::thread::current().id());
        if !panic {
            source_stop.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        assert!(matches!(completed.try_recv(), Err(mpsc::TryRecvError::Empty)));
        release.send(()).unwrap();
        let result = completed.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            result.reason,
            if panic { ExitReason::Panicked } else { ExitReason::IoError(ErrorKind::BrokenPipe) }
        );
        assert_eq!(result.exit_code, None);
        assert!(!result.output_drained);
        assert!(sender.input_status().closed);
        assert!(sender.poller.upgrade().is_none());
        // Publication precedes the callback dropping its sender. Wait for
        // disconnection rather than racing that final metadata-only drop.
        assert!(matches!(
            completed.recv_timeout(Duration::from_secs(5)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }
}

#[test]
fn drain_reads_past_one_parser_chunk_to_eof() {
    let (dropping, _dropped) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let terminal =
        Arc::new(FairMutex::new(Term::new(Default::default(), &TermSize::new(80, 24), Listener)));
    let mut bytes = b"synthetic\r\n".repeat(20_000);
    bytes.extend_from_slice(b"PIKE_FINAL_7\r\n");
    let pty = FailedPty {
        reader: TestReader { bytes: io::Cursor::new(bytes), pending: false },
        writer: io::sink(),
        dropping,
        release: gate,
        panic: false,
    };
    let mut event_loop = EventLoop::new(terminal.clone(), Listener, pty, true, false).unwrap();
    let mut state = State::default();
    let mut buf = vec![0; READ_BUFFER_SIZE];
    event_loop.drain_output::<io::Sink>(&mut state, &mut buf, None).unwrap();
    let text: String = terminal.lock().grid().display_iter().map(|cell| cell.c).collect();
    assert!(text.contains("PIKE_FINAL_7"));
    release.send(()).unwrap();
}

#[test]
fn drain_deadline_reports_timeout_instead_of_claiming_eof() {
    let (dropping, _dropped) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let terminal =
        Arc::new(FairMutex::new(Term::new(Default::default(), &TermSize::new(80, 24), Listener)));
    let pty = FailedPty {
        reader: TestReader { bytes: io::Cursor::new(Vec::new()), pending: true },
        writer: io::sink(),
        dropping,
        release: gate,
        panic: false,
    };
    let mut event_loop = EventLoop::new(terminal, Listener, pty, true, false).unwrap();
    let start = Instant::now();
    let error = event_loop
        .drain_output::<io::Sink>(&mut State::default(), &mut vec![0; READ_BUFFER_SIZE], None)
        .unwrap_err();
    eprintln!("drain timeout observation: {:?}", start.elapsed());
    assert_eq!(error.kind(), ErrorKind::TimedOut);
    release.send(()).unwrap();
}

// Two sockets model independent PTY-I/O and child-exit readiness without a
// second PTY implementation. Closing output must not remove the child watcher.
fn socket_pair() -> (std::net::TcpStream, std::net::TcpStream) {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let peer = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (stream, _) = listener.accept().unwrap();
    stream.set_nonblocking(true).unwrap();
    (stream, peer)
}

struct EofReader {
    stream: std::net::TcpStream,
    reads: Arc<std::sync::atomic::AtomicUsize>,
    error_after_bytes: Option<usize>,
}
impl Read for EofReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        if self.error_after_bytes == Some(0) {
            return Err(ErrorKind::BrokenPipe.into());
        }
        let count = self.stream.read(buf)?;
        if let Some(remaining) = &mut self.error_after_bytes {
            *remaining = remaining.saturating_sub(count);
        }
        Ok(count)
    }
}

struct EofPty {
    reader: EofReader,
    writer: io::Sink,
    child: std::net::TcpStream,
    registered: bool,
    exhausted: Option<mpsc::Sender<()>>,
    metadata: Option<mpsc::Sender<Msg>>,
}
impl event::OnResize for EofPty {
    fn on_resize(&mut self, _: WindowSize) {
        panic!("EOF session must ignore queued resizes");
    }
}
impl EventedReadWrite for EofPty {
    type Reader = EofReader;
    type Writer = io::Sink;
    unsafe fn register(
        &mut self,
        poll: &Arc<polling::Poller>,
        _: PollingEvent,
        mode: PollMode,
    ) -> io::Result<()> {
        // SAFETY: both owned sockets outlive their registrations below.
        unsafe {
            poll.add_with_mode(
                &self.reader.stream,
                PollingEvent::readable(tty::PTY_READ_WRITE_TOKEN),
                mode,
            )?;
            poll.add_with_mode(
                &self.child,
                PollingEvent::readable(tty::PTY_CHILD_EVENT_TOKEN),
                mode,
            )?;
        }
        self.registered = true;
        Ok(())
    }
    fn reregister(
        &mut self,
        poll: &Arc<polling::Poller>,
        mut interest: PollingEvent,
        mode: PollMode,
    ) -> io::Result<()> {
        assert!(self.registered, "exhausted I/O must not be rearmed");
        interest.key = tty::PTY_READ_WRITE_TOKEN;
        poll.modify_with_mode(&self.reader.stream, interest, mode)
    }
    fn deregister(&mut self, poll: &Arc<polling::Poller>) -> io::Result<()> {
        self.deregister_io(poll)?;
        poll.delete(&self.child)
    }
    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.reader
    }
    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.writer
    }
}
impl EventedPty for EofPty {
    fn deregister_io(&mut self, poll: &Arc<polling::Poller>) -> io::Result<()> {
        if self.registered {
            poll.delete(&self.reader.stream)?;
            self.registered = false;
            self.metadata
                .as_ref()
                .unwrap()
                .send(Msg::Resize(WindowSize {
                    num_lines: 24,
                    num_cols: 80,
                    cell_width: 1,
                    cell_height: 1,
                }))
                .unwrap();
            self.exhausted.take().unwrap().send(()).unwrap();
        }
        Ok(())
    }
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        let mut code = [0];
        match self.child.read(&mut code) {
            Ok(1) => Some(ChildEvent::Exited(Some(i32::from(code[0])))),
            _ => None,
        }
    }
}

#[test]
fn eof_disables_io_but_preserves_later_child_exit_and_explicit_close() {
    for close in [false, true] {
        let (stream, output) = socket_pair();
        let (child, mut exit) = socket_pair();
        let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (exhausted, eof) = mpsc::channel();
        let pty = EofPty {
            reader: EofReader { stream, reads: reads.clone(), error_after_bytes: None },
            writer: io::sink(),
            child,
            registered: false,
            exhausted: Some(exhausted),
            metadata: None,
        };
        let terminal = Arc::new(FairMutex::new(Term::new(
            Default::default(),
            &TermSize::new(80, 24),
            Listener,
        )));
        let mut event_loop = EventLoop::new(terminal, Listener, pty, true, false).unwrap();
        event_loop.pty.metadata = Some(event_loop.tx.clone());
        let sender = event_loop.channel();
        let (complete, result) = mpsc::channel();
        drop(output);
        event_loop.spawn_disposing(|| (), move |value| complete.send(value).unwrap());
        eof.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(sender.input_status().closed);
        assert!(matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)));
        assert!(sender
            .send(Msg::Resize(WindowSize {
                num_lines: 24,
                num_cols: 80,
                cell_width: 1,
                cell_height: 1
            }))
            .is_err());
        if close {
            sender.send(Msg::Shutdown).unwrap();
        } else {
            exit.write_all(&[37]).unwrap();
        }
        let completion = result.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(reads.load(Ordering::Relaxed), 1);
        assert_eq!(
            completion.reason,
            if close { ExitReason::Requested } else { ExitReason::ChildExit }
        );
        assert_eq!(completion.exit_code, if close { None } else { Some(37) });
        assert_eq!(completion.output_drained, !close);
    }
}

#[test]
fn io_failure_flushes_already_read_synchronized_output_without_exit() {
    let (stream, mut output) = socket_pair();
    let (child, _exit) = socket_pair();
    let (exhausted, _eof) = mpsc::channel();
    let pty = EofPty {
        reader: EofReader {
            stream,
            reads: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            error_after_bytes: Some(b"\x1b[?2026hPIKE_SYNC_FINAL".len()),
        },
        writer: io::sink(),
        child,
        registered: false,
        exhausted: Some(exhausted),
        metadata: None,
    };
    let terminal =
        Arc::new(FairMutex::new(Term::new(Default::default(), &TermSize::new(80, 24), Listener)));
    let mut event_loop = EventLoop::new(terminal.clone(), Listener, pty, true, false).unwrap();
    event_loop.pty.metadata = Some(event_loop.tx.clone());
    output.write_all(b"\x1b[?2026hPIKE_SYNC_FINAL").unwrap();
    let (complete, result) = mpsc::channel();
    event_loop.spawn_disposing(|| (), move |value| complete.send(value).unwrap());
    let completion = result.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(completion.reason, ExitReason::IoError(ErrorKind::BrokenPipe));
    assert_eq!(completion.exit_code, None);
    assert!(!completion.output_drained);
    let text: String = terminal.lock().grid().display_iter().map(|cell| cell.c).collect();
    assert!(text.contains("PIKE_SYNC_FINAL"));
}
