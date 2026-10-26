use super::*;
use crate::{AlacrittyEvent, BackendCommand, LinkAction};
use std::time::Duration;

fn terminal() -> Terminal {
    let prepared = crate::lifecycle_tests::prepare_terminal(12000, false);
    prepared.attach(Default::default(), Default::default())
}

fn finish(terminal: Terminal) {
    let handle = terminal.shutdown_handle();
    drop(terminal);
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(5), handle.wait())
                .await
                .unwrap();
        });
}

#[test]
fn metadata_input_and_noops_do_no_font_resize_snapshot_or_cache_work() {
    let mut terminal = terminal();
    let before = terminal.work_stats();
    let snapshots = terminal.snapshot_stats().snapshots;
    for _ in 0..1000 {
        for command in [
            BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Title(
                "synthetic".into(),
            )),
            BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::PtyWrite(
                String::new(),
            )),
            BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Bell),
            BackendCommand::Write(Vec::new()),
            BackendCommand::Scroll(0),
            BackendCommand::Resize(None, None),
            BackendCommand::ProcessLink(LinkAction::Clear, Default::default()),
            BackendCommand::SetHistoryLimit(Default::default()),
        ] {
            terminal.handle(Command::ProxyToBackend(command));
        }
        terminal.handle(Command::ChangeFont(Default::default()));
        terminal.handle(Command::AddBindings(Vec::new()));
    }
    assert_eq!(terminal.work_stats(), before);
    assert_eq!(terminal.snapshot_stats().snapshots, snapshots);
    finish(terminal);
}

#[test]
fn output_flood_never_measures_fonts_or_resizes_and_hidden_output_never_snapshots(
) {
    let mut terminal = terminal();
    let before = terminal.work_stats();
    let snapshots = terminal.snapshot_stats().snapshots;
    for _ in 0..1000 {
        terminal.handle(Command::ProxyToBackend(
            BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup),
        ));
    }
    let after = terminal.work_stats();
    assert_eq!(after.font_measurements, before.font_measurements);
    assert_eq!(after.pty_resizes, before.pty_resizes);
    assert_eq!(after.grid_resizes, before.grid_resizes);
    assert_eq!(after.cache_invalidations, before.cache_invalidations + 1000);
    assert_eq!(terminal.snapshot_stats().snapshots, snapshots + 1000);
    terminal.set_visible(false);
    let before = terminal.work_stats();
    let snapshots = terminal.snapshot_stats().snapshots;
    for _ in 0..1000 {
        terminal.handle(Command::ProxyToBackend(
            BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup),
        ));
    }
    assert_eq!(terminal.work_stats(), before);
    assert_eq!(terminal.snapshot_stats().snapshots, snapshots);
    terminal.set_visible(true);
    assert_eq!(terminal.snapshot_stats().snapshots, snapshots + 1);
    finish(terminal);
}

#[test]
fn font_changes_measure_once_and_paint_only_changes_do_not_snapshot() {
    let mut terminal = terminal();
    let before = terminal.work_stats();
    let snapshots = terminal.snapshot_stats().snapshots;
    terminal.handle(Command::ChangeTheme(Default::default()));
    assert_eq!(terminal.snapshot_stats().snapshots, snapshots);
    assert_eq!(
        terminal.work_stats().cache_invalidations,
        before.cache_invalidations + 1
    );
    for _ in 0..10 {
        terminal.handle(Command::ChangeFont(FontSettings {
            size: 20.0,
            ..Default::default()
        }));
    }
    assert_eq!(
        terminal.work_stats().font_measurements,
        before.font_measurements + 1
    );
    assert_eq!(terminal.work_stats().pty_resizes, before.pty_resizes + 1);
    assert_eq!(terminal.snapshot_stats().snapshots, snapshots + 1);
    let stable = terminal.work_stats();
    terminal.handle(Command::ChangeFont(FontSettings {
        size: f32::NAN,
        scale_factor: 0.0,
        ..Default::default()
    }));
    assert_eq!(terminal.work_stats(), stable);
    finish(terminal);
}
