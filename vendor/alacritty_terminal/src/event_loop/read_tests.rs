//! Completion readiness is consumed independently of buffered bytes. Use the
//! real piper reader, public loop and parser; a socket carries custom readiness
//! on portable hosts. Windows additionally tests its actual UnblockedReader.
use super::*;
use crate::term::test::TermSize;
use crate::tty::{ChildEvent, EventedReadWrite};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;
use std::sync::Mutex;
use std::task::{Context, Poll, Wake, Waker};

const DEADLINE: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Observations {
    bytes: AtomicUsize,
    calls: AtomicUsize,
    wakeups: AtomicUsize,
    order: Mutex<Vec<&'static str>>,
}

struct ReadySignal {
    socket: TcpStream,
    active: AtomicBool,
}
impl ReadySignal {
    fn post(&self) {
        if self.active.load(Ordering::Acquire) {
            (&self.socket).write_all(b"r").unwrap();
        }
    }
}
impl Wake for ReadySignal {
    fn wake(self: Arc<Self>) {
        self.post();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.post();
    }
}

struct BufferedReader {
    pipe: piper::Reader,
    ready: TcpStream,
    signal: Arc<ReadySignal>,
    observed: Arc<Observations>,
    results: mpsc::Sender<io::Result<usize>>,
    first_read: Option<Box<dyn FnOnce() + Send>>,
    eof_error: Option<ErrorKind>,
}
impl Read for BufferedReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        // A posted packet is consumed even if piper still has unread bytes.
        let _ = self.ready.read(&mut [0; 128]);
        self.observed.calls.fetch_add(1, Ordering::Relaxed);
        assert!(bytes.len() <= MAX_LOCKED_READ);
        let waker = Waker::from(self.signal.clone());
        let result: io::Result<usize> =
            match self.pipe.poll_drain_bytes(&mut Context::from_waker(&waker), bytes) {
                Poll::Pending => Err(ErrorKind::WouldBlock.into()),
                Poll::Ready(0) => self.eof_error.map_or(Ok(0), |kind| Err(kind.into())),
                Poll::Ready(count) => {
                    self.observed.bytes.fetch_add(count, Ordering::Relaxed);
                    self.observed.order.lock().unwrap().push("read");
                    if let Some(first) = self.first_read.take() {
                        first();
                    }
                    Ok(count)
                },
            };
        let _ = self.results.send(result.as_ref().copied().map_err(|error| error.kind().into()));
        result
    }
}

struct BufferedPty {
    reader: BufferedReader,
    writer: RecordingWriter,
    registered: bool,
    exhausted: mpsc::Sender<()>,
}
struct RecordingWriter(Arc<Observations>);
impl Write for RecordingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.order.lock().unwrap().push("write");
        assert_eq!(bytes, b"fair input");
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl event::OnResize for BufferedPty {
    fn on_resize(&mut self, _: WindowSize) {
        self.reader.observed.order.lock().unwrap().push("resize");
    }
}
impl EventedReadWrite for BufferedPty {
    type Reader = BufferedReader;
    type Writer = RecordingWriter;
    unsafe fn register(
        &mut self,
        poll: &Arc<polling::Poller>,
        _: PollingEvent,
        mode: PollMode,
    ) -> io::Result<()> {
        // SAFETY: this owned socket is deleted before destruction.
        unsafe {
            poll.add_with_mode(
                &self.reader.ready,
                PollingEvent::readable(tty::PTY_READ_WRITE_TOKEN),
                mode,
            )?;
        }
        self.registered = true;
        self.reader.signal.active.store(true, Ordering::Release);
        self.reader.signal.post();
        Ok(())
    }
    fn reregister(
        &mut self,
        poll: &Arc<polling::Poller>,
        mut interest: PollingEvent,
        mode: PollMode,
    ) -> io::Result<()> {
        interest.key = tty::PTY_READ_WRITE_TOKEN;
        poll.modify_with_mode(&self.reader.ready, interest, mode)?;
        // Same nonempty registration rule as Windows UnblockedReader.
        if interest.readable && !self.reader.pipe.is_empty() {
            self.reader.signal.post();
        }
        Ok(())
    }
    fn deregister(&mut self, poll: &Arc<polling::Poller>) -> io::Result<()> {
        if self.registered {
            self.reader.signal.active.store(false, Ordering::Release);
            poll.delete(&self.reader.ready)?;
            self.registered = false;
        }
        Ok(())
    }
    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.reader
    }
    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.writer
    }
}
impl tty::EventedPty for BufferedPty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        None
    }
    fn deregister_io(&mut self, poll: &Arc<polling::Poller>) -> io::Result<()> {
        self.deregister(poll)?;
        self.exhausted.send(()).unwrap();
        Ok(())
    }
}

#[derive(Clone)]
struct Listener {
    events: mpsc::Sender<Event>,
    observed: Arc<Observations>,
}
impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        if matches!(event, Event::Wakeup) {
            self.observed.wakeups.fetch_add(1, Ordering::Relaxed);
        }
        let _ = self.events.send(event);
    }
}

struct Fixture {
    loop_: EventLoop<BufferedPty, Listener>,
    producer: piper::Writer,
    events: mpsc::Receiver<Event>,
    reads: mpsc::Receiver<io::Result<usize>>,
    eof: mpsc::Receiver<()>,
    observed: Arc<Observations>,
}
fn fixture(bytes: &[u8]) -> Fixture {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (ready, _) = listener.accept().unwrap();
    ready.set_nonblocking(true).unwrap();
    let observed = Arc::new(Observations::default());
    let signal = Arc::new(ReadySignal { socket, active: AtomicBool::new(false) });
    let (reader, mut producer) = piper::pipe(READ_BUFFER_SIZE);
    assert_eq!(producer.try_fill(bytes), bytes.len());
    let (results, reads) = mpsc::channel();
    let (exhausted, eof) = mpsc::channel();
    let pty = BufferedPty {
        reader: BufferedReader {
            pipe: reader,
            ready,
            signal,
            observed: observed.clone(),
            results,
            first_read: None,
            eof_error: None,
        },
        writer: RecordingWriter(observed.clone()),
        registered: false,
        exhausted,
    };
    let (events_tx, events) = mpsc::channel();
    let listener = Listener { events: events_tx, observed: observed.clone() };
    let terminal = Arc::new(FairMutex::new(Term::new(
        Default::default(),
        &TermSize::new(80, 24),
        listener.clone(),
    )));
    Fixture {
        loop_: EventLoop::new(terminal, listener, pty, true, false).unwrap(),
        producer,
        events,
        reads,
        eof,
        observed,
    }
}
fn start(loop_: EventLoop<BufferedPty, Listener>) -> (EventLoopSender, mpsc::Receiver<Completion>) {
    let sender = loop_.channel();
    let (done, completion) = mpsc::channel();
    loop_.spawn_disposing(|| (), move |result| done.send(result).unwrap());
    (sender, completion)
}
fn query(events: &mpsc::Receiver<Event>) {
    loop {
        if let Event::PtyWrite(reply) = events.recv_timeout(DEADLINE).unwrap() {
            assert_eq!(reply, "\x1b[0n");
            return;
        }
    }
}
fn pending(reads: &mpsc::Receiver<io::Result<usize>>, expected: usize) {
    let mut consumed = 0;
    loop {
        match reads.recv_timeout(DEADLINE).unwrap() {
            Ok(count) => consumed += count,
            Err(error) => {
                assert_eq!(error.kind(), ErrorKind::WouldBlock);
                // piper can yield and wake itself even before it is empty.
                if consumed == expected {
                    return;
                }
            },
        }
    }
}

#[test]
fn buffered_tail_progresses_without_fresh_producer_and_then_sleeps() {
    for size in [MAX_LOCKED_READ - 5, MAX_LOCKED_READ * 3] {
        let mut bytes = vec![0; size];
        bytes.extend_from_slice(b"\x1b[5n");
        let Fixture { loop_, producer, events, reads, observed, .. } = fixture(&bytes);
        let (sender, completion) = start(loop_);
        // The producer stays open and makes no more writes or notifications.
        query(&events);
        pending(&reads, bytes.len());
        assert_eq!(observed.bytes.load(Ordering::Relaxed), bytes.len());
        let calls = observed.calls.load(Ordering::Relaxed);
        let wakeups = observed.wakeups.load(Ordering::Relaxed);
        assert!(reads.recv_timeout(Duration::from_millis(100)).is_err());
        assert_eq!(observed.calls.load(Ordering::Relaxed), calls);
        assert_eq!(observed.wakeups.load(Ordering::Relaxed), wakeups);
        assert_eq!(sender.input_status().written_bytes, 0);
        sender.send(Msg::Shutdown).unwrap();
        assert_eq!(completion.recv_timeout(DEADLINE).unwrap().reason, ExitReason::Requested);
        drop(producer);
    }
}

#[test]
fn exact_budget_empty_boundary_arms_waker_for_future_output() {
    let Fixture { loop_, mut producer, events, reads, observed, .. } =
        fixture(&vec![0; MAX_LOCKED_READ]);
    let (sender, completion) = start(loop_);
    pending(&reads, MAX_LOCKED_READ);
    assert_eq!(observed.bytes.load(Ordering::Relaxed), MAX_LOCKED_READ);
    assert_eq!(producer.try_fill(b"\x1b[5n"), 4);
    query(&events);
    sender.send(Msg::Shutdown).unwrap();
    assert_eq!(completion.recv_timeout(DEADLINE).unwrap().reason, ExitReason::Requested);
    drop(producer);
}

#[test]
fn continuation_services_input_and_resize_between_bounded_read_turns() {
    let mut bytes = vec![0; MAX_LOCKED_READ * 3];
    bytes.extend_from_slice(b"\x1b[5n");
    let Fixture { mut loop_, producer, events, observed, .. } = fixture(&bytes);
    let sender = loop_.channel();
    loop_.pty.reader.first_read = Some(Box::new(move || {
        sender.send_input(b"fair input").unwrap();
        sender
            .send(Msg::Resize(WindowSize {
                num_lines: 24,
                num_cols: 80,
                cell_width: 1,
                cell_height: 1,
            }))
            .unwrap();
    }));
    let (sender, completion) = start(loop_);
    query(&events);
    sender.send(Msg::Shutdown).unwrap();
    assert_eq!(completion.recv_timeout(DEADLINE).unwrap().reason, ExitReason::Requested);
    let order = observed.order.lock().unwrap();
    let reads: Vec<_> =
        order.iter().enumerate().filter_map(|(i, entry)| (*entry == "read").then_some(i)).collect();
    assert!(order.iter().position(|entry| *entry == "resize").unwrap() < reads[1]);
    // A readiness event may carry both read and write flags. Preserve the
    // existing read-then-write order, but never postpone input past that turn.
    assert!(order.iter().position(|entry| *entry == "write").unwrap() < reads[2]);
    assert_eq!(sender.input_status().written_bytes, b"fair input".len());
    drop(producer);
}

#[test]
fn continuation_reaches_eof_or_error_and_preserves_tail() {
    for error in [None, Some(ErrorKind::Other)] {
        let mut bytes = vec![0; MAX_LOCKED_READ * 3];
        bytes.extend_from_slice(b"\x1b[5n");
        let Fixture { mut loop_, producer, events, eof, observed, .. } = fixture(&bytes);
        loop_.pty.reader.eof_error = error;
        drop(producer);
        let (sender, completion) = start(loop_);
        query(&events);
        if error.is_none() {
            eof.recv_timeout(DEADLINE).unwrap();
            assert!(sender.input_status().closed);
            sender.send(Msg::Shutdown).unwrap();
        }
        let done = completion.recv_timeout(DEADLINE).unwrap();
        assert_eq!(done.reason, error.map_or(ExitReason::Requested, ExitReason::IoError));
        assert_eq!(observed.bytes.load(Ordering::Relaxed), bytes.len());
        assert!(sender.input_status().closed);
    }
}

#[test]
fn requested_shutdown_between_budgets_does_not_read_remainder() {
    let Fixture { mut loop_, producer, observed, .. } = fixture(&vec![0; MAX_LOCKED_READ * 3]);
    let sender = loop_.channel();
    loop_.pty.reader.first_read = Some(Box::new(move || {
        sender.send(Msg::Shutdown).unwrap();
    }));
    let (_, completion) = start(loop_);
    assert_eq!(completion.recv_timeout(DEADLINE).unwrap().reason, ExitReason::Requested);
    assert_eq!(observed.bytes.load(Ordering::Relaxed), MAX_LOCKED_READ);
    drop(producer);
}
