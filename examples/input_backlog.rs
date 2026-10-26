//! Headless Windows integration/resource probe using the existing terminal PTY.
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::advanced::subscription::into_recipes;
use iced::futures::{StreamExt, stream};
use iced_term::{BackendCommand, Command, INPUT_BYTE_LIMIT, InputError, Terminal};

fn main() -> io::Result<()> {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/input-backlog".into()),
    );
    let directory = std::path::absolute(directory)?;
    if directory.exists() {
        return Err(io::Error::other("choose a fresh output directory"));
    }
    fs::create_dir_all(&directory)?;
    let mut report = fs::File::create(directory.join("samples.csv"))?;
    writeln!(
        report,
        "ms,phase,outstanding,queued,written,cancelled,closed,rejections"
    )?;
    let start = Instant::now();
    let mut stalled = terminal(6001, "stall", 0, &directory.join("stall"))?;
    ready(&directory.join("stall"))?;
    sample(&mut report, start, "baseline", &stalled, 0)?;
    let rejected = vec![b'X'; INPUT_BYTE_LIMIT + 1];
    assert!(matches!(
        write(&mut stalled, rejected),
        iced_term::actions::Action::InputRejected(InputError::Full { .. })
    ));
    let block = vec![b'x'; INPUT_BYTE_LIMIT];
    assert_eq!(
        write(&mut stalled, block),
        iced_term::actions::Action::Ignore
    );
    let mut rejections = 0;
    // External benchmark sampling only; no production input task or timer.
    // Attempt at most 25 MiB more synthetic input while the child never reads.
    for _ in 0..400 {
        if matches!(
            write(&mut stalled, vec![b'y'; 64 * 1024]),
            iced_term::actions::Action::InputRejected(_)
        ) {
            rejections += 1;
        }
        assert!(stalled.input_status().outstanding_bytes <= INPUT_BYTE_LIMIT);
        sample(&mut report, start, "saturated", &stalled, rejections)?;
        std::thread::sleep(Duration::from_millis(25));
    }
    let text = format!("{}{}END!", "a".repeat(8191), "界🦀e\u{301}".repeat(9000));
    let expected_hash = text.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    let mut echo = terminal(6002, "echo", text.len(), &directory.join("echo"))?;
    ready(&directory.join("echo"))?;
    assert!(matches!(
        write(&mut echo, vec![b'X'; INPUT_BYTE_LIMIT + 1]),
        iced_term::actions::Action::InputRejected(InputError::Full { .. })
    ));
    assert_eq!(echo.input_status().outstanding_bytes, 0);
    let echo_start = Instant::now();
    assert_eq!(
        write(&mut echo, text.as_bytes().to_vec()),
        iced_term::actions::Action::Ignore
    );
    let result = directory.join("echo/result");
    wait_file(&directory.join("echo/done"))?;
    let expected = format!("{},{expected_hash}\n", text.len());
    assert_eq!(fs::read_to_string(&result)?, expected);
    let echo_ms = echo_start.elapsed().as_secs_f64() * 1000.0;
    let close_start = Instant::now();
    stalled.handle(Command::ProxyToBackend(BackendCommand::RequestShutdown));
    let request_ms = close_start.elapsed().as_secs_f64() * 1000.0;
    sample(&mut report, start, "cancelled", &stalled, rejections)?;
    assert_eq!(stalled.input_status().queued_bytes, 0);
    assert!(matches!(
        write(&mut stalled, b"late".to_vec()),
        iced_term::actions::Action::InputRejected(InputError::Closed)
    ));
    finish_stream(&stalled)?;
    let close_ms = close_start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(fs::read_to_string(&result)?, expected);
    echo.handle(Command::ProxyToBackend(BackendCommand::RequestShutdown));
    finish_stream(&echo)?;
    writeln!(
        report,
        "{:.3},finished,0,0,0,0,true,{rejections}",
        start.elapsed().as_secs_f64() * 1000.0
    )?;
    fs::write(
        directory.join("summary.csv"),
        format!(
            "echo_bytes,echo_ms,close_request_ms,source_completion_ms,rejections\n{},{echo_ms:.3},{request_ms:.3},{close_ms:.3},{rejections}\n",
            text.len()
        ),
    )?;
    Ok(())
}

fn terminal(id: u64, mode: &str, count: usize, directory: &Path) -> io::Result<Terminal> {
    let fixture = std::env::current_exe()?.with_file_name("input_fixture.exe");
    let fixture = fixture
        .to_str()
        .ok_or_else(|| io::Error::other("non-Unicode fixture path"))?;
    let output = directory
        .to_str()
        .ok_or_else(|| io::Error::other("non-Unicode output path"))?;
    if fixture.contains(['"', '\r', '\n', '%']) || output.contains(['"', '\r', '\n', '%']) {
        return Err(io::Error::other("unsupported controlled fixture path"));
    }
    Terminal::new(
        id,
        iced_term::settings::Settings {
            backend: iced_term::settings::BackendSettings {
                program: fixture.into(),
                args: vec![mode.into(), count.to_string(), output.into()],
                ..Default::default()
            },
            ..Default::default()
        },
    )
}

fn write(terminal: &mut Terminal, bytes: Vec<u8>) -> iced_term::actions::Action {
    terminal.handle(Command::ProxyToBackend(BackendCommand::Write(bytes)))
}

fn sample(
    report: &mut impl Write,
    start: Instant,
    phase: &str,
    terminal: &Terminal,
    rejections: usize,
) -> io::Result<()> {
    let status = terminal.input_status();
    writeln!(
        report,
        "{:.3},{phase},{},{},{},{},{},{rejections}",
        start.elapsed().as_secs_f64() * 1000.0,
        status.outstanding_bytes,
        status.queued_bytes,
        status.written_bytes,
        status.cancelled_bytes,
        status.closed
    )?;
    report.flush()
}

fn ready(directory: &Path) -> io::Result<()> {
    wait_file(&directory.join("ready"))
}

fn wait_file(path: &Path) -> io::Result<()> {
    let start = Instant::now();
    while !path.exists() || fs::metadata(path)?.len() == 0 {
        if start.elapsed() > Duration::from_secs(10) {
            return Err(io::Error::other("fixture deadline"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

fn finish_stream(terminal: &Terminal) -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let recipe = into_recipes(terminal.subscription())
        .into_iter()
        .next()
        .ok_or_else(|| io::Error::other("missing subscription"))?;
    runtime.block_on(async {
        let mut events = recipe.stream(stream::empty().boxed());
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut completed = false;
            while let Some(event) = events.next().await {
                if let iced_term::Event::BackendCall(
                    _,
                    BackendCommand::EventBridge(iced_term::EventBridgeEvent::Error(error)),
                ) = &event
                {
                    return Err(io::Error::other(format!(
                        "unexpected bridge failure: {error:?}"
                    )));
                }
                if matches!(
                    event,
                    iced_term::Event::BackendCall(
                        _,
                        BackendCommand::EventBridge(iced_term::EventBridgeEvent::Completed)
                    )
                ) {
                    completed = true;
                }
            }
            if !completed {
                return Err(io::Error::other("missing source completion"));
            }
            Ok(())
        })
        .await
        .map_err(io::Error::other)?
    })
}
