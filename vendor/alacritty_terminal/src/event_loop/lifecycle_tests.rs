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
        assert!(matches!(completed.try_recv(), Err(mpsc::TryRecvError::Disconnected)));
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
