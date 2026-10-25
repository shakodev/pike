use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::time::Instant;

pub struct Report {
    file: BufWriter<File>,
    start: Instant,
    remaining: usize,
}

impl Report {
    pub fn new(directory: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(directory)?;
        let mut file = BufWriter::new(File::create(directory.join("events.csv"))?);
        writeln!(file, "ms,event,session,a,b")?;
        Ok(Self {
            file,
            start: Instant::now(),
            remaining: 512,
        })
    }

    pub fn record(
        &mut self,
        event: &str,
        session: u64,
        a: impl std::fmt::Display,
        b: impl std::fmt::Display,
    ) -> io::Result<()> {
        if self.remaining == 0 {
            return Err(io::Error::other("research event limit reached"));
        }
        self.remaining -= 1;
        writeln!(
            self.file,
            "{:.3},{event},{session},{a},{b}",
            self.start.elapsed().as_secs_f64() * 1000.0
        )?;
        self.file.flush()
    }
}

pub fn screenshot(path: &Path, shot: iced::window::Screenshot) -> io::Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    write!(file, "P6\n{} {}\n255\n", shot.size.width, shot.size.height)?;
    for pixel in shot.rgba.chunks_exact(4) {
        file.write_all(&pixel[..3])?;
    }
    file.flush()
}
