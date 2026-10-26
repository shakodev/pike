//! Synthetic release probe; records metadata/latency, never terminal transcripts.
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use iced::advanced::subscription::into_recipes;
use iced::futures::{StreamExt, stream};
use iced_term::{ExitReason, Terminal};

fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--fixture") {
        return fixture(&args[2], Path::new(&args[3]));
    }
    let directory = std::path::absolute(
        args.get(1)
            .map_or("target/shutdown-lifecycle", String::as_str),
    )?;
    if directory.exists() {
        return Err(io::Error::other("choose a fresh output directory"));
    }
    fs::create_dir_all(&directory)?;
    let mut report = fs::File::create(directory.join("samples.csv"))?;
    writeln!(
        report,
        "cycle,mode,pid,request_ms,completion_ms,within_one_second,reason,drained"
    )?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let executable = std::env::current_exe()?;
    for cycle in 0..10 {
        for mode in ["natural", "close", "cancel"] {
            let ready = directory.join(format!("{cycle}-{mode}.ready"));
            let start = Instant::now();
            let mut terminal = Terminal::new(
                8000 + cycle,
                iced_term::settings::Settings {
                    backend: iced_term::settings::BackendSettings {
                        program: argument(&executable)?,
                        args: vec!["--fixture".into(), mode.into(), argument(&ready)?],
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )?;
            let handle = terminal.shutdown_handle();
            let recipe = into_recipes(terminal.subscription())
                .pop()
                .ok_or_else(|| io::Error::other("missing subscription"))?;
            // A canceled subscription does not own the session. Cleanup and its
            // completion must work without any event consumer.
            drop(recipe.stream(stream::empty().boxed()));
            while !ready.exists() || fs::metadata(&ready)?.len() == 0 {
                if start.elapsed() > Duration::from_secs(10) {
                    return Err(io::Error::other("fixture startup deadline"));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let pid = fs::read_to_string(&ready)?;
            let close = Instant::now();
            let request_ms;
            let completion_start;
            if mode == "cancel" {
                drop(terminal);
                request_ms = close.elapsed().as_secs_f64() * 1000.0;
                completion_start = close;
            } else if mode == "close" {
                assert!(terminal.request_close());
                for _ in 0..100 {
                    assert!(!terminal.request_close());
                }
                request_ms = close.elapsed().as_secs_f64() * 1000.0;
                completion_start = close;
                drop(terminal);
            } else {
                let result = runtime
                    .block_on(async {
                        tokio::time::timeout(Duration::from_secs(5), handle.wait()).await
                    })
                    .map_err(|_| {
                        io::Error::other("natural completion deadline; backend still pending")
                    })?;
                assert_eq!(result.reason, ExitReason::ChildExit);
                assert_eq!(result.exit_code, Some(37));
                assert!(result.output_drained);
                let text: String = terminal
                    .final_content()
                    .ok_or_else(|| io::Error::other("missing final model"))?
                    .grid
                    .display_iter()
                    .map(|cell| cell.c)
                    .collect();
                assert!(text.contains("PIKE_FINAL_7"));
                request_ms = 0.0;
                completion_start = start;
                drop(terminal);
            }
            let result = runtime
                .block_on(async {
                    tokio::time::timeout(Duration::from_secs(5), handle.wait()).await
                })
                .map_err(|_| io::Error::other("cleanup deadline; backend still pending"))?;
            if mode != "natural" {
                assert_eq!(result.reason, ExitReason::Requested);
            }
            let elapsed = completion_start.elapsed();
            writeln!(
                report,
                "{cycle},{mode},{},{request_ms:.3},{:.3},{},{:?},{}",
                pid.trim(),
                elapsed.as_secs_f64() * 1000.0,
                elapsed <= Duration::from_secs(1),
                result.reason,
                result.output_drained
            )?;
            report.flush()?;
        }
    }
    Ok(())
}

fn argument(path: &Path) -> io::Result<String> {
    let value = path
        .to_str()
        .ok_or_else(|| io::Error::other("non-Unicode fixture path"))?;
    Ok(value.into())
}

fn fixture(mode: &str, ready: &Path) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    // Exceed one parser chunk and exercise output-worker backpressure.
    let block = b"012345678901234567890123456789012345678901234567890123456789\r\n".repeat(3000);
    stdout.write_all(&block)?;
    stdout.flush()?;
    fs::write(ready, std::process::id().to_string())?;
    if mode == "natural" {
        stdout.write_all(b"\r\nPIKE_FINAL_7\r\n")?;
        stdout.flush()?;
        std::process::exit(37);
    }
    loop {
        stdout.write_all(&block)?;
    }
}
