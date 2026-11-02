//! Fixed ASCII control protocol, bounded output chunks and one joined flood worker.
use std::{
    io::{self, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const CHUNK: usize = 16 * 1024;
pub const RATE: usize = 2 * 1024 * 1024;

pub fn run() -> io::Result<()> {
    crate::fixture_input::configure(None)?;
    title(&format!("ready:{}", std::process::id()))?;
    let mut flood = None;
    let stop = Arc::new(AtomicBool::new(false));
    while let Some(command) = crate::fixture_input::command(&mut io::stdin().lock())? {
        if command == "prepare empty" || command == "prepare saturated" {
            let mut out = io::stdout().lock();
            if command.ends_with("saturated") {
                // 2,200×120 fixed ASCII columns exceeds the 2,000-line cap even
                // in the widest supported pane. No retained transcript.
                let line = format!("{}\r\n", "S".repeat(120));
                for _ in 0..2200 {
                    out.write_all(line.as_bytes())?;
                }
            }
            write!(
                out,
                "\x1b[2J\x1b[HPIKE38 READY | wide: 界 | combining: e\u{301} | \x1b[7mreverse\x1b[0m\x1b[?25h"
            )?;
            out.flush()?;
            drop(out);
            title("prepared")?;
        } else if let Some(seconds) = command.strip_prefix("flood ") {
            let (seconds, epoch) = seconds
                .split_once(' ')
                .ok_or_else(|| io::Error::other("flood needs epoch"))?;
            let seconds: u64 = seconds.parse().map_err(io::Error::other)?;
            let epoch: u128 = epoch.parse().map_err(io::Error::other)?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(io::Error::other)?
                .as_millis();
            if epoch < now || epoch - now > 32_000 {
                return Err(io::Error::other("flood epoch out of bounds"));
            }
            let wait = Duration::from_millis((epoch - now) as u64);
            if !(1..=600).contains(&seconds) || flood.is_some() {
                return Err(io::Error::other("invalid flood budget"));
            }
            let stop = stop.clone();
            flood = Some(std::thread::spawn(move || {
                std::thread::sleep(wait);
                produce(Duration::from_secs(seconds), &stop)
            }));
        } else if let Some(nonce) = command.strip_prefix("echo ") {
            let nonce: usize = nonce.parse().map_err(io::Error::other)?;
            if nonce > 200 {
                return Err(io::Error::other("invalid echo nonce"));
            }
            let mut out = io::stdout().lock();
            write!(out, "\x1b7\x1b[1;1HPIKE38 ECHO {nonce:03}\x1b8")?;
            out.flush()?;
        } else if command == "exit" {
            stop.store(true, Ordering::Relaxed);
            if let Some(worker) = flood.take() {
                worker
                    .join()
                    .map_err(|_| io::Error::other("flood worker panic"))??;
            }
            title("final")?;
            return Ok(());
        } else {
            return Err(io::Error::other("unknown fixed fixture command"));
        }
    }
    stop.store(true, Ordering::Relaxed);
    if let Some(worker) = flood {
        worker
            .join()
            .map_err(|_| io::Error::other("flood worker panic"))??;
    }
    Ok(())
}

fn title(value: &str) -> io::Result<()> {
    let mut out = io::stdout().lock();
    write!(out, "\x1b]2;pike38:{value}\x07")?;
    out.flush()
}

fn produce(duration: Duration, stop: &AtomicBool) -> io::Result<()> {
    // Reserve the first row for exact input-to-snapshot/readback oracles.
    {
        let mut out = io::stdout().lock();
        write!(out, "\x1b[2;r\x1b[2;1H")?;
        out.flush()?;
    }
    title("flooding")?;
    let mut chunk = [b'F'; CHUNK];
    for line in chunk.chunks_mut(128) {
        line[126] = b'\r';
        line[127] = b'\n';
    }
    let start = Instant::now();
    let mut bytes = 0usize;
    while start.elapsed() < duration && !stop.load(Ordering::Relaxed) {
        {
            let mut out = io::stdout().lock();
            out.write_all(&chunk)?;
            out.flush()?;
        }
        bytes += chunk.len();
        let due = Duration::from_secs_f64(bytes as f64 / RATE as f64);
        if let Some(delay) = due.checked_sub(start.elapsed()) {
            std::thread::sleep(delay.min(Duration::from_millis(8)));
        }
    }
    // This title is emitted after all stdout writes; host observes it after the
    // existing parser. Bytes include synthetic data only, excluding VT setup.
    title(&format!("done:{bytes}:{}", start.elapsed().as_micros()))
}
