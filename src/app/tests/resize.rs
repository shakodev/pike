use super::*;
use iced::{Size, widget::pane_grid::ResizeEvent};

#[test]
fn resize_minimize_restore_font_and_repeated_dimensions_keep_native_session() {
    let executor = SharedExecutor::new().unwrap();
    let mut app = App {
        profile: waiting(),
        ..App::default()
    };
    drop(app.update(Message::WindowFocused(true)));
    let (ids, ready) = launch(&mut app, &executor);
    drop(app.launched(ids, Ok(ready)));
    let target = app.active_target().unwrap();
    let original = app.terminal(ids.session).unwrap().widget_id().clone();
    let last_good = Size::new(960.0, 600.0);
    drop(app.update(Message::GridSize(target.tab, last_good)));
    let request = executor
        .enter(|| app.begin_split(target, Direction::Right))
        .unwrap();
    let next = Reservation {
        session: request.session,
        request: request.request,
    };
    let ready = executor.block_on(request.receive()).unwrap();
    drop(app.launched(next, Ok(ready)));
    let split = *app
        .model
        .workspace()
        .tab(target.tab)
        .unwrap()
        .grid()
        .layout()
        .splits()
        .next()
        .unwrap();
    let snapshot = |app: &App| {
        format!(
            "{:?}",
            app.model
                .workspace()
                .tab(target.tab)
                .unwrap()
                .grid()
                .layout()
        )
    };
    let initial = snapshot(&app);
    drop(app.update(Message::WindowFocused(false)));
    drop(app.update(Message::Control(Control::ResizeKey(
        next.session.get(),
        ResizeStep::Right,
    ))));
    drop(app.update(Message::Control(Control::ResizeStep(ResizeStep::Right))));
    assert_eq!(snapshot(&app), initial);
    drop(app.update(Message::WindowFocused(true)));
    // A key captured before an owner transfer cannot resize the new owner's
    // ancestor after the update queue selects it.
    drop(app.update(Message::Control(Control::Focus(target))));
    drop(app.update(Message::Control(Control::ResizeKey(
        next.session.get(),
        ResizeStep::Right,
    ))));
    assert_eq!(snapshot(&app), initial);
    let effect = app.update(Message::Control(Control::ResizeKey(
        ids.session.get(),
        ResizeStep::Right,
    )));
    assert!(
        effect.units() > 0,
        "keyboard resize must restore actual widget focus"
    );
    drop(app.update(Message::Control(Control::Resize(
        target.tab,
        ResizeEvent { split, ratio: 0.0 },
    ))));
    let before = format!(
        "{:?}",
        app.model
            .workspace()
            .tab(target.tab)
            .unwrap()
            .grid()
            .layout()
    );
    for size in [Size::ZERO, Size::new(f32::NAN, 2.0)] {
        drop(app.update(Message::GridSize(target.tab, size)));
        assert_eq!(app.grid_size, Some((target.tab, last_good)));
    }
    for area in [Size::new(1.0, 1.0), Size::new(1920.0, 1080.0), last_good] {
        drop(app.update(Message::GridSize(target.tab, area)));
        assert_eq!(
            before,
            format!(
                "{:?}",
                app.model
                    .workspace()
                    .tab(target.tab)
                    .unwrap()
                    .grid()
                    .layout()
            )
        );
        let presented = app.resize_layout.as_ref().unwrap();
        for bounds in presented
            .grid
            .layout()
            .pane_regions(4.0, 0.0, presented.size)
            .values()
        {
            assert!(bounds.width > 0.0 && bounds.height > 0.0);
        }
    }
    let bounded = app.resize_layout.as_ref().unwrap().size;
    drop(app.update(Message::GridSize(target.tab, Size::new(20_000.0, 20_000.0))));
    assert_eq!(app.resize_layout.as_ref().unwrap().size, bounded);
    drop(app.update(Message::GridSize(target.tab, last_good)));
    app.change_terminal_font(
        ids.session,
        iced_term::settings::FontSettings {
            size: 18.0,
            ..Default::default()
        },
    );
    let size = Size::new(320.0, 240.0);
    let event = || {
        Message::Terminal((
            ids.session,
            Event::BackendCall(
                ids.session.get(),
                iced_term::BackendCommand::Resize(Some(size), None),
            ),
        ))
    };
    drop(app.update(event()));
    let counts = app.terminal(ids.session).unwrap().work_stats();
    for _ in 0..100 {
        drop(app.update(event()));
    }
    assert_eq!(counts, app.terminal(ids.session).unwrap().work_stats());
    for id in [ids.session, next.session] {
        assert_eq!(
            app.model.sessions().get(id).unwrap().lifecycle(),
            Lifecycle::Running
        );
    }
    assert_eq!(app.terminal(ids.session).unwrap().widget_id(), &original);
    assert!(!app.terminal(ids.session).unwrap().input_status().closed);
    drop(app.update(Message::GridSize(target.tab, Size::new(20_000.0, 20_000.0))));
    let native_before = app.terminal(ids.session).unwrap().work_stats().pty_resizes;
    drop(app.update(Message::Terminal((
        ids.session,
        Event::BackendCall(
            ids.session.get(),
            iced_term::BackendCommand::Resize(Some(Size::new(1000.0, 500.0)), None),
        ),
    ))));
    assert_eq!(
        app.terminal(ids.session).unwrap().work_stats().pty_resizes,
        native_before
    );
    app.close(app.model.target(next.session).unwrap());
    let presented = app.resize_layout.as_ref().unwrap();
    assert_eq!(
        presented.grid.panes,
        app.model.workspace().tab(target.tab).unwrap().grid().panes
    );
    assert_eq!(presented.grid.len(), 1);
    assert!(presented.frozen);
    shutdown(&mut app, &executor);
}
