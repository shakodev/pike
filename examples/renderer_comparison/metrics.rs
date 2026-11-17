use pike::{app::App, identity::SessionId};
use std::{
    fs::File,
    io::{self, BufWriter, Write},
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

static REDRAWS: AtomicU64 = AtomicU64::new(0);
static PREVIOUS: Mutex<Option<Instant>> = Mutex::new(None);
static FRAMES: [AtomicU64; 2002] = [const { AtomicU64::new(0) }; 2002];

pub fn observe(event: &iced::Event) {
    if let iced::Event::Window(iced::window::Event::RedrawRequested(at)) = event {
        REDRAWS.fetch_add(1, Ordering::Relaxed);
        let mut previous = PREVIOUS.lock().expect("fixture frame observer");
        if let Some(before) = previous.replace(*at) {
            let ms = at.saturating_duration_since(before).as_millis().min(2001) as usize;
            FRAMES[ms].fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub fn reset_frames() {
    REDRAWS.store(0, Ordering::Relaxed);
    *PREVIOUS.lock().expect("fixture frame observer") = None;
    for count in &FRAMES {
        count.store(0, Ordering::Relaxed);
    }
}

pub fn save_frames(path: &Path) -> io::Result<()> {
    let mut file = File::create(path.join("frame-intervals.csv"))?;
    writeln!(file, "lower_ms,count")?;
    for (ms, count) in FRAMES.iter().enumerate() {
        writeln!(file, "{ms},{}", count.load(Ordering::Relaxed))?;
    }
    std::fs::write(
        path.join("redraws.txt"),
        REDRAWS.load(Ordering::Relaxed).to_string(),
    )
}

pub fn counters(app: &App, path: &Path, name: &str) -> io::Result<()> {
    let mut file = File::create(path.join(format!("counters-{name}.csv")))?;
    writeln!(
        file,
        "session,visible,columns,rows,history,history_limit,snapshots,cells_copied,font_measurements,pty_resizes,grid_resizes,cache_invalidations,input_outstanding,input_queued,input_written,input_cancelled"
    )?;
    for (id, _) in app.model().sessions().iter() {
        if let Some(terminal) = app.terminal(id) {
            let s = terminal.snapshot_stats();
            let w = terminal.work_stats();
            let i = terminal.input_status();
            writeln!(
                file,
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                id.get(),
                terminal.is_visible(),
                s.columns,
                s.screen_lines,
                s.history_lines,
                s.history_limit.lines(),
                s.snapshots,
                s.cells_copied,
                w.font_measurements,
                w.pty_resizes,
                w.grid_resizes,
                w.cache_invalidations,
                i.outstanding_bytes,
                i.queued_bytes,
                i.written_bytes,
                i.cancelled_bytes
            )?;
        }
    }
    Ok(())
}

pub fn marker(app: &App, id: SessionId, marker: &str) -> bool {
    app.terminal(id)
        .and_then(|t| t.renderable_content())
        .is_some_and(|content| {
            // At most one viewport row of known fixture bytes; no transcript/log.
            content
                .viewport
                .cells()
                .iter()
                .take(content.viewport.columns())
                .map(|cell| cell.c)
                .collect::<String>()
                .contains(marker)
        })
}

pub fn screenshot(path: &Path, shot: &iced::window::Screenshot) -> io::Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    write!(file, "P6\n{} {}\n255\n", shot.size.width, shot.size.height)?;
    let mut row = Vec::with_capacity(shot.size.width as usize * 3);
    for pixels in shot.rgba.chunks_exact(shot.size.width as usize * 4) {
        row.clear();
        for pixel in pixels.chunks_exact(4) {
            row.extend_from_slice(&pixel[..3]);
        }
        file.write_all(&row)?;
    }
    file.flush()
}
