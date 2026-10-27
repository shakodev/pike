//! Scratch evidence only. Uses the unchanged public merged-main EventLoop and
//! pinned piper. On Unix, one datagram stands for one Windows CompletionPacket;
//! it is consumed independently of bytes remaining in piper. No real PTY/child.
#![cfg(test)]
use alacritty_terminal::event::{Event, EventListener, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, ExitReason, Msg};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Term, test::TermSize};
use alacritty_terminal::tty::{ChildEvent, EventedPty, EventedReadWrite};
use polling::{Event as ReadyEvent, PollMode, Poller};
use std::io::{self, Read};
use std::os::unix::net::UnixDatagram;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

const CAP: usize = u16::MAX as usize;
const PIPE_CAPACITY: usize = 0x10_0000;

#[derive(Default)]
struct Stats {
    reads: AtomicUsize,
    consumed: AtomicUsize,
    buffered: AtomicUsize,
    posts: AtomicUsize,
    registrations: AtomicUsize,
    pending: AtomicUsize,
}

struct Signal {
    socket: UnixDatagram,
    stats: Arc<Stats>,
    registered: AtomicBool,
}
impl Signal {
    fn post(&self) {
        // Windows Registration::wake does nothing after deregistration.
        if !self.registered.load(Ordering::SeqCst) {
            return;
        }
        self.socket.send(&[1]).unwrap();
        self.stats.posts.fetch_add(1, Ordering::SeqCst);
    }
}
impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.post();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.post();
    }
}

struct PipeReader {
    pipe: piper::Reader,
    signal: Arc<Signal>,
    ready: UnixDatagram,
    stats: Arc<Stats>,
    control_repost: bool,
}
impl Read for PipeReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        // Consume the custom notification, not the buffered output. Windows
        // Poller::wait consumes posted CompletionPackets before invoking read.
        let _ = self.ready.recv(&mut [0]);
        self.stats.reads.fetch_add(1, Ordering::SeqCst);
        let waker = Waker::from(self.signal.clone());
        match self
            .pipe
            .poll_drain_bytes(&mut Context::from_waker(&waker), bytes)
        {
            Poll::Pending => {
                self.stats.pending.fetch_add(1, Ordering::SeqCst);
                Err(io::ErrorKind::WouldBlock.into())
            }
            Poll::Ready(n) => {
                self.stats.consumed.fetch_add(n, Ordering::SeqCst);
                self.stats.buffered.store(self.pipe.len(), Ordering::SeqCst);
                eprintln!(
                    "read n={n} buffered={} destination={} control_repost={}",
                    self.pipe.len(),
                    bytes.len(),
                    self.control_repost
                );
                // Positive control ONLY. Not baseline behavior or a production
                // fix: explicitly make remaining buffered bytes readable again.
                if self.control_repost && !self.pipe.is_empty() {
                    self.signal.post();
                }
                Ok(n)
            }
        }
    }
}

struct BufferedPty {
    reader: PipeReader,
    writer: io::Sink,
    registered: bool,
}
impl OnResize for BufferedPty {
    fn on_resize(&mut self, _: WindowSize) {}
}
impl EventedReadWrite for BufferedPty {
    type Reader = PipeReader;
    type Writer = io::Sink;
    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        _: ReadyEvent,
        mode: PollMode,
    ) -> io::Result<()> {
        // SAFETY: deregister deletes this owned source before it is dropped.
        unsafe {
            poll.add_with_mode(&self.reader.ready, ReadyEvent::readable(0), mode)?;
        }
        self.registered = true;
        self.reader.signal.registered.store(true, Ordering::SeqCst);
        self.reader
            .stats
            .registrations
            .fetch_add(1, Ordering::SeqCst);
        // Same first-register/nonempty notification as UnblockedReader::register.
        self.reader.signal.post();
        Ok(())
    }
    fn reregister(&mut self, _: &Arc<Poller>, event: ReadyEvent, _: PollMode) -> io::Result<()> {
        self.reader
            .stats
            .registrations
            .fetch_add(1, Ordering::SeqCst);
        if event.readable && !self.reader.pipe.is_empty() {
            self.reader.signal.post();
        }
        assert!(!event.writable, "test intentionally has no outgoing input");
        Ok(())
    }
    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        self.reader.signal.registered.store(false, Ordering::SeqCst);
        if self.registered {
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
impl EventedPty for BufferedPty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        None
    }
}

#[derive(Clone)]
struct Listener(mpsc::Sender<Event>);
impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        self.0.send(event).unwrap();
    }
}

fn progress_scenario(control_repost: bool, payload_size: usize) -> bool {
    // piper's fairness RNG is seeded before constructing this pipe. No crate
    // source, private layout or piper behavior is replaced.
    fastrand::seed(1234);
    let (pipe, mut producer) = piper::pipe(PIPE_CAPACITY);
    let mut payload = vec![0; payload_size]; // NUL: no terminal screen work or sync mode.
    payload.extend_from_slice(b"\x1b[5n"); // Status query after the service cap.
    assert_eq!(producer.try_fill(&payload), payload.len());
    let stats = Arc::new(Stats::default());
    stats.buffered.store(payload.len(), Ordering::SeqCst);
    let (ready, socket) = UnixDatagram::pair().unwrap();
    ready.set_nonblocking(true).unwrap();
    socket.set_nonblocking(true).unwrap();
    let signal = Arc::new(Signal {
        socket,
        stats: stats.clone(),
        registered: AtomicBool::new(false),
    });
    let pty = BufferedPty {
        reader: PipeReader {
            pipe,
            signal: signal.clone(),
            ready,
            stats: stats.clone(),
            control_repost,
        },
        writer: io::sink(),
        registered: false,
    };
    let (events_tx, events) = mpsc::channel();
    let listener = Listener(events_tx);
    let terminal = Arc::new(FairMutex::new(Term::new(
        Default::default(),
        &TermSize::new(80, 24),
        listener.clone(),
    )));
    let event_loop = EventLoop::new(terminal, listener, pty, true, false).unwrap();
    let sender = event_loop.channel();
    let (done_tx, done) = mpsc::channel();
    event_loop.spawn_disposing(|| (), move |completion| done_tx.send(completion).unwrap());
    let mut reply = false;
    // Bounded test observer deadline only, no production timeout/timer added.
    loop {
        match events.recv_timeout(Duration::from_millis(300)) {
            Ok(Event::PtyWrite(bytes)) => {
                assert_eq!(bytes, "\x1b[0n");
                reply = true;
                break;
            }
            Ok(_) => (),
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(error) => panic!("unexpected event disconnect: {error}"),
        }
    }
    eprintln!(
        "observation reply={reply} consumed={} buffered={} reads={} pending={} posts={} registrations={} input={:?}",
        stats.consumed.load(Ordering::SeqCst),
        stats.buffered.load(Ordering::SeqCst),
        stats.reads.load(Ordering::SeqCst),
        stats.pending.load(Ordering::SeqCst),
        stats.posts.load(Ordering::SeqCst),
        stats.registrations.load(Ordering::SeqCst),
        sender.input_status()
    );
    if !reply {
        assert_eq!(stats.consumed.load(Ordering::SeqCst), CAP);
        assert_eq!(stats.buffered.load(Ordering::SeqCst), payload.len() - CAP);
        assert_eq!(stats.registrations.load(Ordering::SeqCst), 1);
        assert_eq!(stats.posts.load(Ordering::SeqCst), 1);
        assert_eq!(stats.pending.load(Ordering::SeqCst), 0);
        // A plain poller notification / metadata delivery is NOT read readiness.
        sender
            .send(Msg::Resize(WindowSize {
                num_lines: 24,
                num_cols: 80,
                cell_width: 1,
                cell_height: 1,
            }))
            .unwrap();
        assert!(matches!(
            events.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert_eq!(stats.consumed.load(Ordering::SeqCst), CAP);
        // Independent causal rescue: one explicit readable packet, no new
        // output bytes, input writes, process exit, sync timeout or parser reset.
        signal.post();
        loop {
            match events.recv_timeout(Duration::from_millis(300)).unwrap() {
                Event::PtyWrite(bytes) => {
                    assert_eq!(bytes, "\x1b[0n");
                    break;
                }
                Event::Wakeup => {
                    if stats.buffered.load(Ordering::SeqCst) > 0 {
                        signal.post();
                    }
                }
                _ => (),
            }
        }
        eprintln!("manual-readable-rescue query parsed; no new producer bytes");
    }
    sender.send(Msg::Shutdown).unwrap();
    let completion = done.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(completion.reason, ExitReason::Requested);
    assert!(sender.input_status().closed);
    eprintln!("cleanup={completion:?}");
    drop(producer); // Keep pipe open through observation; EOF cannot rescue it.
    reply
}

#[test]
fn buffered_output_must_progress_without_new_readiness() {
    let rearm = std::env::var_os("HARNESS_REARM_CONTROL").is_some();
    assert!(
        progress_scenario(rearm, CAP * 3),
        "real merged EventLoop stranded pinned-piper output after 65535-byte service turn"
    );
}

#[test]
fn below_service_cap_progresses_without_rearm() {
    assert!(progress_scenario(false, CAP - 5));
}
