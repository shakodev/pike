//! Native IOCP/piper contract: preload more than two service budgets, then
//! block the source so no new producer write can rescue a missed continuation.
use super::*;
use crate::event::{Event as TerminalEvent, EventListener, OnResize, WindowSize};
use crate::event_loop::{EventLoop, ExitReason, Msg};
use crate::sync::FairMutex;
use crate::term::{test::TermSize, Term};
use crate::tty::{ChildEvent, EventedPty, EventedReadWrite, PTY_READ_WRITE_TOKEN};
use std::sync::mpsc;
use std::time::Duration;

struct Source {
    bytes: io::Cursor<Vec<u8>>,
    buffered: Option<mpsc::Sender<()>>,
    release: mpsc::Receiver<()>,
}
impl Read for Source {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.bytes.read(bytes)?;
        if count == 0 {
            if let Some(buffered) = self.buffered.take() {
                buffered.send(()).unwrap();
                self.release.recv_timeout(Duration::from_secs(5)).unwrap();
            }
        }
        Ok(count)
    }
}
struct Pty {
    reader: UnblockedReader<Source>,
    writer: io::Sink,
}
impl Drop for Pty {
    fn drop(&mut self) {
        self.reader.drain_and_join();
    }
}
impl OnResize for Pty {
    fn on_resize(&mut self, _: WindowSize) {}
}
impl EventedReadWrite for Pty {
    type Reader = UnblockedReader<Source>;
    type Writer = io::Sink;
    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        mut interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        interest.key = PTY_READ_WRITE_TOKEN;
        self.reader.register(poll, interest, mode);
        Ok(())
    }
    fn reregister(
        &mut self,
        poll: &Arc<Poller>,
        mut interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        interest.key = PTY_READ_WRITE_TOKEN;
        self.reader.register(poll, interest, mode);
        Ok(())
    }
    fn deregister(&mut self, _: &Arc<Poller>) -> io::Result<()> {
        self.reader.deregister();
        Ok(())
    }
    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.reader
    }
    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.writer
    }
}
impl EventedPty for Pty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        None
    }
    fn deregister_io(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        self.deregister(poll)
    }
}
#[derive(Clone)]
struct Listener(mpsc::Sender<String>);
impl EventListener for Listener {
    fn send_event(&self, event: TerminalEvent) {
        if let TerminalEvent::PtyWrite(reply) = event {
            let _ = self.0.send(reply);
        }
    }
}

#[test]
fn actual_iocp_reader_continues_prebuffered_tail_without_new_source_output() {
    let mut bytes = vec![0; u16::MAX as usize * 3];
    bytes.extend_from_slice(b"\x1b[5n");
    let total = bytes.len();
    let (buffered, ready) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let source = Source { bytes: io::Cursor::new(bytes), buffered: Some(buffered), release: gate };
    let reader = UnblockedReader::new(source, crate::event_loop::READ_BUFFER_SIZE).unwrap();
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(reader.pipe.len(), total, "all output buffered before first registration");
    let (reply, received) = mpsc::channel();
    let listener = Listener(reply);
    let terminal = Arc::new(FairMutex::new(Term::new(
        Default::default(),
        &TermSize::new(80, 24),
        listener.clone(),
    )));
    let loop_ = EventLoop::new(terminal, listener, Pty { reader, writer: io::sink() }, true, false)
        .unwrap();
    let sender = loop_.channel();
    let (complete, completed) = mpsc::channel();
    loop_.spawn_disposing(|| (), move |result| complete.send(result).unwrap());
    let result = received.recv_timeout(Duration::from_secs(2));
    // Release/join the genuine existing reader worker even for a failed oracle.
    release.send(()).unwrap();
    sender.send(Msg::Shutdown).unwrap();
    assert_eq!(
        completed.recv_timeout(Duration::from_secs(5)).unwrap().reason,
        ExitReason::Requested
    );
    assert!(sender.input_status().closed);
    assert_eq!(sender.input_status().written_bytes, 0);
    assert_eq!(result.unwrap(), "\x1b[0n");
}
