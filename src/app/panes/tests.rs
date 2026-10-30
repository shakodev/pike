use super::*;
use crate::workspace::Model;
use iced::widget::pane_grid::{Axis, Edge, Target};

fn fixture() -> (Model, Gestures, Context, Context) {
    let mut model = Model::default();
    let (a, _) = model.create_tab().unwrap();
    let (b, _) = model.split(a, Axis::Vertical).unwrap();
    let context = |target| Context {
        target,
        owner: model
            .sessions()
            .get(model.workspace().pane(target).unwrap().session)
            .unwrap()
            .reservation(),
        zoom: None,
    };
    let (a, b) = (context(a), context(b));
    let gestures = Gestures::default();
    gestures.synchronize(Some(a));
    (model, gestures, a, b)
}

#[test]
fn header_focus_is_allowed_once_but_owner_round_trip_retires_the_receipt() {
    let (_, gestures, a, b) = fixture();
    let action = gestures.action(
        b.target,
        b.owner,
        DragEvent::Picked {
            pane: b.target.pane,
        },
    );
    assert!(gestures.begin(action));
    gestures.synchronize(Some(b));
    assert!(gestures.current(action));
    gestures.synchronize(Some(a));
    gestures.synchronize(Some(b));
    assert!(!gestures.current(action));
    assert!(!gestures.begin(action));
    let fresh = gestures.action(b.target, b.owner, action.event);
    assert!(gestures.begin(fresh));
    assert!(gestures.current(fresh));
}

#[test]
fn blur_modal_zoom_and_restart_each_require_a_fresh_press() {
    for transition in 0..4 {
        let (_, gestures, a, _) = fixture();
        let action = gestures.action(
            a.target,
            a.owner,
            DragEvent::Picked {
                pane: a.target.pane,
            },
        );
        assert!(gestures.begin(action));
        match transition {
            0 => gestures.cancel(), // raw blur, before its queued App message
            1 => {
                gestures.synchronize(None);
                gestures.synchronize(Some(a));
            }
            2 => gestures.synchronize(Some(Context {
                zoom: Some(a.target.pane),
                ..a
            })),
            3 => gestures.synchronize(Some(Context {
                owner: Reservation::allocate().unwrap(),
                ..a
            })),
            _ => unreachable!(),
        }
        assert!(!gestures.current(action));
        assert!(!gestures.begin(action));
    }
}

#[test]
fn stale_app_release_and_replayed_drop_cannot_change_the_tree() {
    let (model, gestures, a, b) = fixture();
    let mut app = App {
        model,
        pane_gestures: gestures,
        ..App::default()
    };
    drop(app.update(Message::WindowFocused(true)));
    drop(app.select(a.target));
    let action = app.pane_gestures.action(
        a.target,
        a.owner,
        DragEvent::Dropped {
            pane: a.target.pane,
            target: Target::Edge(Edge::Bottom),
        },
    );
    assert!(app.pane_gestures.begin(action));
    drop(app.pane_drag(action));
    let moved = format!("{:?}", app.model);
    drop(app.pane_drag(action));
    assert_eq!(moved, format!("{:?}", app.model));
    let action = app.pane_gestures.action(
        a.target,
        a.owner,
        DragEvent::Dropped {
            pane: a.target.pane,
            target: Target::Edge(Edge::Left),
        },
    );
    assert!(app.pane_gestures.begin(action));
    drop(app.select(b.target));
    drop(app.select(a.target));
    let before = format!("{:?}", app.model);
    drop(app.pane_drag(action));
    assert_eq!(before, format!("{:?}", app.model));
}

#[test]
fn failed_panes_keep_host_move_zoom_controls_and_stale_controls_are_harmless() {
    let (model, gestures, a, b) = fixture();
    let mut app = App {
        model,
        pane_gestures: gestures,
        ..App::default()
    };
    drop(app.launched(a.owner, Err(crate::launch::LaunchError::WorkerFailed)));
    drop(app.launched(b.owner, Err(crate::launch::LaunchError::WorkerFailed)));
    drop(app.update(Message::WindowFocused(true)));
    drop(app.select(a.target));
    let owners = format!("{:?}", app.model.sessions());
    let action = app.pane_gestures.action(
        a.target,
        a.owner,
        DragEvent::Dropped {
            pane: a.target.pane,
            target: Target::Edge(Edge::Bottom),
        },
    );
    assert!(app.pane_gestures.begin(action));
    drop(app.pane_drag(action));
    let layout = format!(
        "{:?}",
        app.model
            .workspace()
            .tab(a.target.tab)
            .unwrap()
            .grid()
            .layout()
    );
    for _ in 0..2 {
        drop(app.update(Message::Control(super::super::Control::Zoom(a.target))));
        drop(app.view());
    }
    assert_eq!(owners, format!("{:?}", app.model.sessions()));
    assert_eq!(
        layout,
        format!(
            "{:?}",
            app.model
                .workspace()
                .tab(a.target.tab)
                .unwrap()
                .grid()
                .layout()
        )
    );
    app.close(a.target);
    let before = format!("{:?}", app.model);
    drop(app.update(Message::Control(super::super::Control::Zoom(a.target))));
    drop(app.pane_drag(action));
    assert_eq!(before, format!("{:?}", app.model));
}

#[test]
fn moved_starting_owner_attaches_hidden_without_reviving_old_focus_intent() {
    use crate::app::{Control, Direction};
    use crate::executor::SharedExecutor;
    use iced::{Executor, Size};
    use iced_term::settings::BackendSettings;
    use std::time::Duration;

    let executor = SharedExecutor::new().unwrap();
    let profile = if cfg!(windows) {
        BackendSettings {
            program: "cmd.exe".into(),
            args: vec![
                "/D".into(),
                "/Q".into(),
                "/C".into(),
                "set /p PIKE27_WAIT=".into(),
            ],
            #[cfg(windows)]
            escape_args: false,
            ..Default::default()
        }
    } else {
        BackendSettings {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "read pike27_wait".into()],
            ..Default::default()
        }
    };
    let mut app = App {
        profile,
        ..App::default()
    };
    drop(app.update(Message::WindowFocused(true)));
    let request = executor.enter(|| app.begin_open()).unwrap();
    let first = Reservation {
        session: request.session,
        request: request.request,
    };
    let ready = executor.block_on(request.receive());
    drop(app.launched(first, ready));
    let source = app.active_target().unwrap();
    drop(app.update(Message::GridSize(source.tab, Size::new(1200.0, 800.0))));
    let request = executor
        .enter(|| app.begin_split(source, Direction::Right))
        .unwrap();
    let pending = Reservation {
        session: request.session,
        request: request.request,
    };
    let moved = app.model.target(pending.session).unwrap();
    drop(app.update(Message::Control(Control::Focus(moved))));
    let action = app.pane_gestures.action(
        moved,
        pending,
        DragEvent::Dropped {
            pane: moved.pane,
            target: Target::Edge(Edge::Bottom),
        },
    );
    assert!(app.pane_gestures.begin(action));
    drop(app.pane_drag(action));
    drop(app.update(Message::Control(Control::Zoom(source))));
    let layout = format!(
        "{:?}",
        app.model
            .workspace()
            .tab(source.tab)
            .unwrap()
            .grid()
            .layout()
    );
    let ready = executor.block_on(request.receive());
    drop(app.launched(pending, ready));
    assert_eq!(app.model.target(pending.session), Some(moved));
    assert_eq!(app.active_session(), Some(first.session));
    let terminal = app.terminal(pending.session).unwrap();
    assert!(!terminal.is_visible());
    assert!(terminal.renderable_content().is_none());
    assert_eq!(terminal.snapshot_stats().snapshots, 0);
    assert_eq!(terminal.snapshot_stats().viewport_cells, 0);
    assert_eq!(terminal.work_stats().grid_resizes, 0);
    drop(app.update(Message::Control(Control::Zoom(source))));
    assert_eq!(app.active_session(), Some(first.session));
    assert_eq!(
        layout,
        format!(
            "{:?}",
            app.model
                .workspace()
                .tab(source.tab)
                .unwrap()
                .grid()
                .layout()
        )
    );
    for id in [first.session, pending.session] {
        let handle = app.terminal(id).unwrap().shutdown_handle();
        handle.request_close();
        executor.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), handle.wait())
                .await
                .unwrap();
        });
    }
}
