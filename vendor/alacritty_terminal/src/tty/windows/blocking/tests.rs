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
    let mut writer = UnblockedWriter::new(GatedWriter { entered, release: gate }, 1024);
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
