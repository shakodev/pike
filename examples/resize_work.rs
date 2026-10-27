//! Opt-in native ConPTY geometry/counter probe, using only synthetic output.
use iced::advanced::subscription::into_recipes;
use iced::futures::{
    StreamExt,
    stream::{self, BoxStream},
};
use iced::{Executor, Size};
use iced_term::settings::{BackendSettings, FontSettings};
use iced_term::{AlacrittyEvent, BackendCommand, Command, Event, PreparedTerminal, Terminal};
use pike::executor::SharedExecutor;
use std::io::{self, Write};
use std::time::{Duration, Instant};

fn main() -> io::Result<()> {
    if !cfg!(windows) {
        return Err(io::Error::other("requires native Windows ConPTY"));
    }
    let fixture = std::env::current_exe()?.with_file_name("pty_fixture.exe");
    let prepared = PreparedTerminal::new(
        12001,
        BackendSettings {
            program: fixture.to_string_lossy().into_owned(),
            args: vec!["1".into()],
            ..Default::default()
        },
    )?;
    let mut terminal = prepared.attach(Default::default(), Default::default());
    let mut events = into_recipes(terminal.subscription())
        .pop()
        .unwrap()
        .stream(stream::empty().boxed());
    let executor = SharedExecutor::new()?;
    let result = executor.block_on(async {
        tokio::time::timeout(Duration::from_secs(30), run(&mut terminal, &mut events))
            .await
            .map_err(io::Error::other)?
    });
    let handle = terminal.shutdown_handle();
    drop(events);
    drop(terminal);
    // Join even on verification failure; a timeout never claims cleanup success.
    executor
        .block_on(async { tokio::time::timeout(Duration::from_secs(5), handle.wait()).await })
        .map_err(io::Error::other)?;
    result
}

fn send(terminal: &mut Terminal, command: BackendCommand) {
    let action = terminal.handle(Command::ProxyToBackend(command));
    assert!(
        !matches!(action, iced_term::actions::Action::InputRejected(_)),
        "synthetic input rejected"
    );
}

async fn title(
    terminal: &mut Terminal,
    events: &mut BoxStream<'static, Event>,
    prefix: &str,
) -> io::Result<String> {
    while let Some(Event::BackendCall(_, command)) = events.next().await {
        let found = match &command {
            BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Title(text))
                if text.starts_with(prefix) =>
            {
                Some(text.clone())
            }
            BackendCommand::EventBridge(iced_term::EventBridgeEvent::Error(_)) => {
                return Err(io::Error::other("fixture event bridge failed"));
            }
            _ => None,
        };
        terminal.handle(Command::ProxyToBackend(command));
        if let Some(text) = found {
            return Ok(text);
        }
    }
    Err(io::Error::other("fixture event stream ended early"))
}

async fn size(
    terminal: &mut Terminal,
    events: &mut BoxStream<'static, Event>,
    expected: (usize, usize),
    nonce: &mut u64,
) -> io::Result<()> {
    // Resize/input delivery can be batched by the native loop. Retry only this
    // opt-in fixture query, never a resize, until the child observes the change.
    loop {
        *nonce += 1;
        send(
            terminal,
            BackendCommand::Write(format!("size {nonce}\r").into_bytes()),
        );
        let text = title(terminal, events, "pike:size:1:").await?;
        let fields: Vec<_> = text.split(':').collect();
        if fields.len() == 6
            && fields[3].parse::<usize>() == Ok(expected.0)
            && fields[4].parse::<usize>() == Ok(expected.1)
            && fields[5].parse::<u64>() == Ok(*nonce)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn record(
    output: &mut impl Write,
    name: &str,
    terminal: &Terminal,
    start: Instant,
) -> io::Result<()> {
    let work = terminal.work_stats();
    let snapshot = terminal.snapshot_stats();
    writeln!(
        output,
        "{name},{},{},{},{},{},{},{},{}",
        start.elapsed().as_micros(),
        work.font_measurements,
        work.pty_resizes,
        work.grid_resizes,
        work.cache_invalidations,
        snapshot.snapshots,
        snapshot.columns,
        snapshot.screen_lines
    )
}

async fn run(terminal: &mut Terminal, events: &mut BoxStream<'static, Event>) -> io::Result<()> {
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "case,elapsed_us,font_measurements,pty_resizes,grid_resizes,cache_invalidations,snapshots,columns,rows"
    )?;
    let start = Instant::now();
    title(terminal, events, "pike:ready:1:").await?;
    send(
        terminal,
        BackendCommand::Resize(Some(Size::new(1020.0, 730.0)), Some(Size::new(8.5, 18.25))),
    );
    let mut nonce = 0;
    size(terminal, events, (120, 40), &mut nonce).await?;
    record(&mut output, "initial-layout", terminal, start)?;
    let before = terminal.work_stats();
    let snapshots = terminal.snapshot_stats().snapshots;
    for step in 0..1000 {
        let delta = step as f32 / 1000.0;
        send(
            terminal,
            BackendCommand::Resize(Some(Size::new(1020.0 + delta, 730.0 + delta)), None),
        );
    }
    assert_eq!(terminal.work_stats(), before);
    assert_eq!(terminal.snapshot_stats().snapshots, snapshots);
    record(&mut output, "1000-subcell-drags", terminal, start)?;
    for (scale, cols, rows) in [(1.0, 120, 40), (1.25, 96, 32), (1.5, 80, 26), (2.0, 60, 20)] {
        let before = terminal.work_stats();
        for _ in 0..100 {
            send(
                terminal,
                BackendCommand::Resize(Some(Size::new(1020.0 / scale, 730.0 / scale)), None),
            );
        }
        let stats = terminal.snapshot_stats();
        // Each scale after 100% changes effective cell geometry exactly once.
        assert_eq!(
            terminal.work_stats().pty_resizes - before.pty_resizes,
            u64::from(scale != 1.0)
        );
        assert_eq!((stats.columns, stats.screen_lines), (cols, rows));
        assert_eq!(
            terminal.work_stats().grid_resizes - before.grid_resizes,
            u64::from(scale != 1.0)
        );
        assert_eq!(
            terminal.work_stats().font_measurements,
            before.font_measurements
        );
        size(terminal, events, (cols, rows), &mut nonce).await?;
        record(&mut output, &format!("scale-{scale}"), terminal, start)?;
    }
    let before = terminal.work_stats();
    for value in [0.0, 0.01, -1.0, f32::NAN, f32::INFINITY] {
        send(
            terminal,
            BackendCommand::Resize(Some(Size::new(value, value)), None),
        );
    }
    send(
        terminal,
        BackendCommand::Resize(Some(Size::new(510.0, 365.0)), None),
    );
    assert_eq!(terminal.work_stats(), before);
    size(terminal, events, (60, 20), &mut nonce).await?;
    record(&mut output, "minimize-invalid-restore", terminal, start)?;
    for font in [
        FontSettings {
            size: 20.0,
            ..Default::default()
        },
        FontSettings {
            size: 20.0,
            scale_factor: 1.6,
            ..Default::default()
        },
    ] {
        let before = terminal.work_stats();
        for _ in 0..100 {
            terminal.handle(Command::ChangeFont(font.clone()));
        }
        assert_eq!(
            terminal.work_stats().font_measurements,
            before.font_measurements + 1
        );
        assert_eq!(terminal.work_stats().pty_resizes, before.pty_resizes + 1);
        let stats = terminal.snapshot_stats();
        size(
            terminal,
            events,
            (stats.columns, stats.screen_lines),
            &mut nonce,
        )
        .await?;
        record(&mut output, "font-change", terminal, start)?;
    }
    // A smaller font must use the newest positive pane area even when the
    // preceding font could not fit there. Native acknowledgement catches a
    // stale remembered area as well as an incorrect in-memory grid.
    let before = terminal.work_stats();
    let previous = terminal.snapshot_stats();
    send(
        terminal,
        BackendCommand::Resize(Some(Size::new(12.0, 12.0)), None),
    );
    assert_eq!(terminal.work_stats(), before);
    assert_eq!(terminal.snapshot_stats().columns, previous.columns);
    assert_eq!(
        terminal.snapshot_stats().screen_lines,
        previous.screen_lines
    );
    terminal.handle(Command::ChangeFont(FontSettings {
        size: 1.0,
        ..Default::default()
    }));
    let cells = terminal.renderable_content().unwrap().terminal_size;
    let expected = (
        (12.0 / f64::from(cells.cell_width)).floor() as usize,
        (12.0 / f64::from(cells.cell_height)).floor() as usize,
    );
    assert_eq!(terminal.work_stats().pty_resizes, before.pty_resizes + 1);
    let stats = terminal.snapshot_stats();
    assert_eq!((stats.columns, stats.screen_lines), expected);
    size(terminal, events, expected, &mut nonce).await?;
    record(&mut output, "font-reduction-in-tiny-pane", terminal, start)?;
    let before = terminal.work_stats();
    terminal.handle(Command::ChangeFont(FontSettings {
        size: 20.0,
        scale_factor: 1.6,
        ..Default::default()
    }));
    assert_eq!(terminal.work_stats().pty_resizes, before.pty_resizes);
    send(
        terminal,
        BackendCommand::Resize(Some(Size::new(510.0, 365.0)), None),
    );
    assert_eq!(terminal.work_stats().pty_resizes, before.pty_resizes + 1);
    size(
        terminal,
        events,
        (previous.columns, previous.screen_lines),
        &mut nonce,
    )
    .await?;
    record(&mut output, "font-and-pane-restore", terminal, start)?;
    for (hidden, nonce) in [(false, 1), (true, 2)] {
        terminal.set_visible(!hidden);
        let before = terminal.work_stats();
        let snapshots = terminal.snapshot_stats().snapshots;
        send(
            terminal,
            BackendCommand::Write(format!("flood {nonce}\r").into_bytes()),
        );
        title(terminal, events, &format!("pike:flood:1:{nonce}")).await?;
        assert_eq!(
            terminal.work_stats().font_measurements,
            before.font_measurements
        );
        assert_eq!(terminal.work_stats().pty_resizes, before.pty_resizes);
        if hidden {
            assert_eq!(terminal.snapshot_stats().snapshots, snapshots);
            assert_eq!(
                terminal.work_stats().cache_invalidations,
                before.cache_invalidations
            );
        }
        record(
            &mut output,
            if hidden {
                "hidden-flood"
            } else {
                "visible-flood"
            },
            terminal,
            start,
        )?;
    }
    terminal.set_visible(true);
    send(terminal, BackendCommand::Write(b"exit\r".to_vec()));
    while let Some(Event::BackendCall(_, command)) = events.next().await {
        terminal.handle(Command::ProxyToBackend(command));
    }
    let result = terminal
        .shutdown_handle()
        .completion()
        .ok_or_else(|| io::Error::other("missing completion"))?;
    assert!(result.output_drained && result.exit_code == Some(41));
    record(&mut output, "completed", terminal, start)
}
