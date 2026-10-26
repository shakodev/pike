use super::*;
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct PartialWriter(usize);
impl Write for PartialWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0 == 0 {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let count = self.0.min(bytes.len()).min(2);
        self.0 -= count;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn successful_partial_os_writes_are_counted_before_a_later_error() {
    let progress = Arc::new(WriterProgress::default());
    let mut writer = CountingWriter { sink: PartialWriter(3), written: progress.clone() };
    assert_eq!(writer.write(b"abcd").unwrap(), 2);
    assert_eq!(writer.write(b"cd").unwrap(), 1);
    assert_eq!(progress.written(), 3);
    assert!(writer.write(b"d").is_err());
    assert_eq!(progress.written(), 3);
}

struct GatedWriter {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
impl Write for GatedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let _ = self.entered.send(());
        self.release.recv_timeout(Duration::from_secs(5)).map_err(io::Error::other)?;
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn buffered_and_blocked_bytes_are_not_acknowledged_and_failure_closes_admission() {
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut writer = UnblockedWriter::new(GatedWriter { entered, release: gate }, 1024).unwrap();
    let progress = writer.writer_progress();
    let queue = crate::input::InputQueue::new(progress.clone());
    queue.send(b"synthetic", || Ok(())).unwrap();
    let chunk = queue.take().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut accepted = 0;
    while accepted < chunk.len() {
        assert!(Instant::now() < deadline, "buffer admission deadline");
        accepted += writer.write(&chunk[accepted..]).unwrap();
        // piper can yield even with free capacity; it is a nonblocking writer.
        std::thread::yield_now();
    }
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(progress.written(), 0);
    assert_eq!(queue.status().outstanding_bytes, chunk.len());
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !queue.status().closed && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(queue.status().closed);
    assert_eq!(queue.status().cancelled_bytes, chunk.len());
    assert_eq!(queue.preflight(1), Err(crate::input::InputError::Closed));
}

#[test]
fn reader_distinguishes_pending_from_eof_and_cleanup_drains_a_full_pipe() {
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    struct GatedReader {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
        bytes: io::Cursor<Vec<u8>>,
    }
    impl Read for GatedReader {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if self.bytes.position() == 0 {
                self.entered.send(()).unwrap();
                self.release.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            self.bytes.read(bytes)
        }
    }
    let source = GatedReader { entered, release: gate, bytes: io::Cursor::new(vec![b'x'; 4096]) };
    let mut reader = UnblockedReader::new(source, 16).unwrap();
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(reader.read(&mut [0; 1]).unwrap_err().kind(), io::ErrorKind::WouldBlock);
    release.send(()).unwrap();
    reader.drain_and_join();
    assert!(reader.worker.is_none());
    assert_eq!(reader.read(&mut [0; 1]).unwrap(), 0);
}

#[test]
fn read_failure_is_distinguished_from_normal_broken_pipe_eof() {
    struct FailedReader(io::ErrorKind);
    impl Read for FailedReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(self.0.into())
        }
    }
    for kind in [io::ErrorKind::BrokenPipe, io::ErrorKind::Other] {
        let mut reader = UnblockedReader::new(FailedReader(kind), 16).unwrap();
        reader.drain_and_join();
        if kind == io::ErrorKind::BrokenPipe {
            assert_eq!(reader.read(&mut [0; 1]).unwrap(), 0);
        } else {
            assert_eq!(reader.read(&mut [0; 1]).unwrap_err().kind(), kind);
        }
    }
}

#[test]
fn reader_panic_is_not_reported_as_a_clean_eof() {
    struct PanickingReader;
    impl Read for PanickingReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("synthetic reader panic");
        }
    }
    let mut reader = UnblockedReader::new(PanickingReader, 16).unwrap();
    reader.drain_and_join();
    assert_eq!(reader.read(&mut [0; 1]).unwrap_err().kind(), io::ErrorKind::Other);
}

#[test]
fn emergency_reader_drain_reaches_eof_without_consuming_piper() {
    let (done, observed) = mpsc::channel();
    struct ObserveEof {
        bytes: io::Cursor<Vec<u8>>,
        done: mpsc::Sender<()>,
    }
    impl Read for ObserveEof {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let count = self.bytes.read(bytes)?;
            if count == 0 {
                self.done.send(()).unwrap();
            }
            Ok(count)
        }
    }
    let mut reader =
        UnblockedReader::new(ObserveEof { bytes: io::Cursor::new(vec![b'x'; 65536]), done }, 16)
            .unwrap();
    reader.drain_without_buffering();
    // No consumer drains piper until after EOF: direct servicing must bypass it.
    observed.recv_timeout(Duration::from_secs(5)).unwrap();
    reader.drain_and_join();
}
