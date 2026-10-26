//! Opt-in allocation-footprint/CPU comparison against the R03 clone algorithm.
//! Uses the actual Alacritty parser/grid; it does not create a PTY or window.
use super::*;
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::term::{test::TermSize, Config};
use std::hint::black_box;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

type Parser = alacritty_terminal::vte::ansi::Processor;

fn phase(directory: &Path, name: &str) {
    std::fs::write(directory.join("phase.txt"), name).unwrap();
    // Measurement-only hold lets the external sampler observe retained memory.
    std::thread::sleep(Duration::from_millis(250));
}

fn transition(directory: &Path, name: &str) {
    std::fs::write(directory.join("phase.txt"), name).unwrap();
}

fn fill(term: &mut Term<VoidListener>, limit: usize, combining: usize) {
    let line =
        format!("x{}{}\r\n", "\u{301}".repeat(combining), " ".repeat(119));
    let mut parser = Parser::new();
    for _ in 0..(limit + 30) * 2 {
        parser.advance(term, line.as_bytes());
    }
}

#[test]
#[ignore = "opt-in release footprint profile; see tools/measure-history.ps1"]
fn profile_history_snapshot_cost() {
    let mode = std::env::var("PIKE_PROFILE_MODE").unwrap();
    assert!(["full", "viewport", "hidden"].contains(&mode.as_str()));
    let limit = HistoryLimit::try_from(
        std::env::var("PIKE_PROFILE_HISTORY")
            .unwrap()
            .parse::<usize>()
            .unwrap(),
    )
    .unwrap()
    .lines();
    let combining: usize = std::env::var("PIKE_PROFILE_COMBINING")
        .unwrap()
        .parse()
        .unwrap();
    assert!(combining <= 4096);
    let directory =
        std::path::absolute(std::env::var("PIKE_PROFILE_DIRECTORY").unwrap())
            .unwrap();
    std::fs::create_dir(&directory).expect("choose a fresh profile directory");
    let mut terminal = Term::new(
        Config {
            scrolling_history: limit,
            ..Default::default()
        },
        &TermSize::new(120, 30),
        VoidListener,
    );
    phase(&directory, "empty");
    transition(&directory, "filling");
    fill(&mut terminal, limit, combining);
    assert_eq!(terminal.history_size(), limit);
    phase(&directory, "saturated");
    transition(&directory, "refilling");
    fill(&mut terminal, limit, combining);
    assert_eq!(terminal.history_size(), limit);
    phase(&directory, "plateau");

    let mut snapshot = RenderableContent::default();
    let mut full = None;
    let mut durations = Vec::with_capacity(100);
    let mut viewport_allocation = None;
    transition(&directory, "copying");
    for _ in 0..100 {
        let start = Instant::now();
        if mode == "full" {
            // Exact pre-R11/R03 presentation algorithm, including Grid storage cache.
            full = Some(terminal.grid().clone());
            black_box(&full);
        } else if mode == "viewport" {
            snapshot.capture(&terminal, TerminalSize::default());
            let allocation = snapshot.viewport.cells.as_ptr();
            if let Some(previous) = viewport_allocation {
                assert_eq!(previous, allocation);
            }
            viewport_allocation = Some(allocation);
            black_box(&snapshot);
        }
        durations.push(start.elapsed().as_nanos());
    }
    durations.sort_unstable();
    let cells_per_snapshot = match mode.as_str() {
        "full" => terminal.total_lines() * terminal.columns(),
        "viewport" => snapshot.viewport.cells.len(),
        _ => 0,
    };
    let mut result =
        std::fs::File::create(directory.join("profile.csv")).unwrap();
    writeln!(result, "mode,history_limit,retained_history,columns,rows,combining_per_cell,iterations,logical_cells_per_snapshot,p50_us,p95_us,viewport_allocation_reused").unwrap();
    writeln!(result, "{mode},{limit},{},120,30,{combining},100,{cells_per_snapshot},{:.3},{:.3},{}", terminal.history_size(), durations[49] as f64 / 1000.0, durations[94] as f64 / 1000.0, viewport_allocation.is_some()).unwrap();
    phase(&directory, "snapshot");
    black_box((&terminal, &snapshot, &full));
    transition(&directory, "releasing");
    drop(full);
    drop(snapshot);
    drop(terminal);
    phase(&directory, "released");
}
