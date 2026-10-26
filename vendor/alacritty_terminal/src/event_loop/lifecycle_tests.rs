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

struct FailedPty {
    reader: TestReader,
    writer: io::Sink,
    dropping: mpsc::Sender<ThreadId>,
    release: mpsc::Receiver<()>,
    panic: bool,
}
impl Drop for FailedPty {
    fn drop(&mut self) {
        self.dropping.send(std::thread::current().id()).unwrap();
        self.release.recv_timeout(Duration::from_secs(5)).unwrap();
    }
}
impl event::OnResize for FailedPty {
    fn on_resize(&mut self, _: WindowSize) {}
}
impl EventedReadWrite for FailedPty {
    type Reader = TestReader;
    type Writer = io::Sink;
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
impl EventedPty for FailedPty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        None
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
