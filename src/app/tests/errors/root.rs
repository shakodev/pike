//! Actual production App::view, root capture and pane grid with real PTYs.
use super::*;
use iced::advanced::{
    Layout, Shell, clipboard, layout, mouse,
    renderer::Headless,
    widget::{
        Id, Operation, Tree,
        operation::{self, Outcome},
    },
};
use iced::{Point, Rectangle, Vector};

struct Bounds {
    targets: Vec<Id>,
    found: Vec<(Id, Rectangle)>,
    translation: Vector,
    pending: Option<Vector>,
}
impl Operation for Bounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        let previous = self.translation;
        if let Some(translation) = self.pending.take() {
            self.translation += translation;
        }
        visit(self);
        self.translation = previous;
    }
    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if let Some(id) = id.filter(|id| self.targets.contains(id)) {
            self.found.push((id.clone(), bounds - self.translation));
        }
    }
    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _: &mut dyn operation::Focusable) {
        self.container(id, bounds);
    }
    fn scrollable(
        &mut self,
        _: Option<&Id>,
        _: Rectangle,
        _: Rectangle,
        translation: Vector,
        _: &mut dyn operation::Scrollable,
    ) {
        self.pending = Some(translation);
    }
}

struct Host {
    app: App,
    executor: SharedExecutor,
    renderer: iced::Renderer,
    tree: Tree,
    size: Size,
    clipboard: Option<String>,
    clipboard_reads: usize,
}
impl Host {
    fn apply(&mut self, message: Message) {
        let task = self.executor.enter(|| self.app.update(message));
        self.task(task);
    }
    fn task(&mut self, task: Task<Message>) {
        let Some(mut actions) = iced_runtime::task::into_stream(task) else {
            return;
        };
        while let Some(action) = self.executor.block_on(async {
            tokio::time::timeout(DEADLINE, actions.next())
                .await
                .unwrap()
        }) {
            match action {
                iced_runtime::Action::Output(message) => self.apply(message),
                iced_runtime::Action::Widget(mut operation) => loop {
                    let mut view = self.app.view();
                    self.tree.diff(view.as_widget());
                    let node = view.as_widget_mut().layout(
                        &mut self.tree,
                        &self.renderer,
                        &layout::Limits::new(self.size, self.size),
                    );
                    view.as_widget_mut().operate(
                        &mut self.tree,
                        Layout::new(&node),
                        &self.renderer,
                        operation.as_mut(),
                    );
                    match operation.finish() {
                        Outcome::Chain(next) => operation = next,
                        _ => break,
                    }
                },
                iced_runtime::Action::Clipboard(iced_runtime::clipboard::Action::Read {
                    channel,
                    ..
                }) => {
                    self.clipboard_reads += 1;
                    channel.send(self.clipboard.clone()).unwrap();
                }
                iced_runtime::Action::Clipboard(
                    iced_runtime::clipboard::Action::WriteWithResult { channel, .. },
                ) => {
                    channel.send(Ok(())).unwrap();
                }
                _ => {}
            }
        }
    }
    fn dispatch(&mut self, event: iced::Event, cursor: Point) {
        let messages = {
            let mut view = self.app.view();
            self.tree.diff(view.as_widget());
            let node = view.as_widget_mut().layout(
                &mut self.tree,
                &self.renderer,
                &layout::Limits::new(self.size, self.size),
            );
            let mut messages = Vec::new();
            view.as_widget_mut().update(
                &mut self.tree,
                &event,
                Layout::new(&node),
                mouse::Cursor::Available(cursor),
                &self.renderer,
                &mut clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(self.size),
            );
            messages
        };
        for message in messages {
            self.apply(message);
        }
    }
    fn settle(&mut self) {
        // A finite fixture turn lets the production sensor/resize messages
        // settle. No product polling or redraw subscription is added.
        for _ in 0..4 {
            self.dispatch(
                iced::Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now(),
                )),
                Point::ORIGIN,
            );
        }
    }
    fn bounds(&mut self, ids: Vec<Id>) -> Vec<(Id, Rectangle)> {
        let mut bounds = Bounds {
            targets: ids,
            found: Vec::new(),
            translation: Vector::ZERO,
            pending: None,
        };
        let mut view = self.app.view();
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.size, self.size),
        );
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            &mut bounds,
        );
        bounds.found
    }
    fn click(&mut self, id: Id) {
        self.settle();
        let rectangle = self
            .bounds(vec![id.clone()])
            .into_iter()
            .find(|(found, _)| *found == id)
            .expect("visible control")
            .1;
        let point = rectangle.center();
        self.dispatch(
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
            point,
        );
        self.dispatch(
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                iced::mouse::Button::Left,
            )),
            point,
        );
        self.settle();
    }
    fn type_one(&mut self) {
        self.dispatch(
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Character("x".into()),
                modified_key: iced::keyboard::Key::Character("x".into()),
                physical_key: iced::keyboard::key::Physical::Unidentified(
                    iced::keyboard::key::NativeCode::Unidentified,
                ),
                location: iced::keyboard::Location::Standard,
                modifiers: iced::keyboard::Modifiers::empty(),
                text: Some("x".into()),
                repeat: false,
            }),
            Point::ORIGIN,
        );
    }
}
fn accepted(app: &App, id: SessionId) -> usize {
    let status = app.terminal(id).unwrap().input_status();
    status.written_bytes + status.outstanding_bytes + status.cancelled_bytes
}

#[test]
#[ignore = "requires a local graphics device; explicitly validates production root/grid recovery"]
fn root_recovery_row_keeps_background_controls_passive_and_routes_fresh_clipboard() {
    let executor = SharedExecutor::new().unwrap();
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .expect("headless renderer");
    let (mut app, healthy, origin) = pair(&executor);
    app.model
        .move_to_tab(
            app.model.target(origin.session).unwrap(),
            app.model.target(healthy.session).unwrap(),
            iced::widget::pane_grid::Axis::Vertical,
        )
        .unwrap();
    drop(app.select(app.model.target(healthy.session).unwrap()));
    let tree = Tree::new(app.view().as_widget());
    let mut host = Host {
        app,
        executor,
        renderer,
        tree,
        size: Size::new(960.0, 700.0),
        clipboard: None,
        clipboard_reads: 0,
    };
    host.settle();
    host.task(host.app.focus_active());
    host.type_one();
    let before = accepted(&host.app, healthy.session);
    host.apply(Message::Terminal((
        origin.session,
        Event::BackendCall(
            origin.session.get(),
            B::InputRejected(iced_term::InputError::Full {
                requested: 1,
                available: 0,
            }),
        ),
    )));
    host.settle();
    assert_eq!(host.app.active_session(), Some(healthy.session));
    assert_eq!(accepted(&host.app, healthy.session), before);
    // No explicit control/focus task may be needed after passive admission.
    host.type_one();
    assert_eq!(accepted(&host.app, healthy.session), before + 1);
    let notice = host
        .app
        .latest_error(Owner::Session(origin.session))
        .unwrap();
    let row_id = Id::from("workspace-recovery");
    let terminal_id = host
        .app
        .terminal(healthy.session)
        .unwrap()
        .widget_id()
        .clone();
    let bounds = host.bounds(vec![row_id.clone(), terminal_id.clone()]);
    let recovery = bounds.iter().find(|(id, _)| *id == row_id).unwrap().1;
    let terminal = bounds.iter().find(|(id, _)| *id == terminal_id).unwrap().1;
    assert_eq!(recovery.height, 128.0);
    assert!(terminal.y >= recovery.y + recovery.height);
    assert_eq!(
        host.app.grid_size.unwrap().1.height,
        host.size.height - crate::ui::tokens::TAB_HEIGHT - 128.0
    );
    host.click(Id::from(format!("recovery-dismiss-{}", notice.id.get())));
    assert!(
        host.app
            .latest_error(Owner::Session(origin.session))
            .is_none()
    );
    assert_eq!(host.app.active_session(), Some(healthy.session));
    host.type_one();
    assert_eq!(accepted(&host.app, healthy.session), before + 2);

    // Last-notice resolution is equally passive, including widget identity.
    let failed_write = host.app.clipboard_writes.begin(Some(origin)).unwrap();
    host.apply(Message::ClipboardWritten(
        failed_write,
        Err(iced_runtime::clipboard::WriteError::WriteFailed),
    ));
    host.settle();
    let before_resolution = accepted(&host.app, healthy.session);
    host.type_one();
    assert_eq!(accepted(&host.app, healthy.session), before_resolution + 1);
    let successful_write = host.app.clipboard_writes.begin(Some(origin)).unwrap();
    host.apply(Message::ClipboardWritten(successful_write, Ok(())));
    host.settle();
    assert!(
        host.app
            .latest_error(Owner::Session(origin.session))
            .is_none()
    );
    host.type_one();
    assert_eq!(accepted(&host.app, healthy.session), before_resolution + 2);

    // Related constraint rows also keep the same pane child position.
    for size in [Size::new(220.0, 200.0), Size::new(960.0, 700.0)] {
        let before_resize = accepted(&host.app, healthy.session);
        host.size = size;
        host.settle();
        host.type_one();
        assert_eq!(accepted(&host.app, healthy.session), before_resize + 1);
    }

    // A background launch retry also has no authority to select its origin.
    let (_, Effect::Spawn(failed)) = host.app.model.create_tab().unwrap() else {
        panic!("spawn")
    };
    host.app.profiles.insert(failed.session, waiting());
    host.apply(Message::Launched(
        failed,
        Err(LaunchError::Spawn(std::io::ErrorKind::NotFound)),
    ));
    let target = host.app.model.target(healthy.session).unwrap();
    let focus = host.app.select(target);
    host.task(focus);
    let retry = host
        .app
        .latest_error(Owner::Session(failed.session))
        .unwrap();
    host.click(Id::from(format!("recovery-retry-{}", retry.id.get())));
    assert!(host.app.model.sessions().get(failed.session).is_none());
    assert_eq!(host.app.active_session(), Some(healthy.session));
    let before_key = accepted(&host.app, healthy.session);
    host.type_one();
    assert_eq!(accepted(&host.app, healthy.session), before_key + 1);

    // Fresh runtime clipboard reads use the same production lease consumer.
    host.app.set_diagnostics_enabled(true);
    host.app.report_error(
        Owner::Session(healthy.session),
        Error::Clipboard(iced_term::PasteError::ClipboardUnavailable),
    );
    let paste = host
        .app
        .latest_error(Owner::Session(healthy.session))
        .unwrap();
    host.clipboard = Some("SECRET_ROOT_CLIPBOARD34".into());
    let survivor = accepted(&host.app, origin.session);
    let before_paste = accepted(&host.app, healthy.session);
    host.click(Id::from(format!("recovery-retry-{}", paste.id.get())));
    assert_eq!(host.clipboard_reads, 1);
    assert_eq!(
        accepted(&host.app, healthy.session),
        before_paste + "SECRET_ROOT_CLIPBOARD34".len()
    );
    assert_eq!(accepted(&host.app, origin.session), survivor);
    assert_eq!(host.app.active_session(), Some(healthy.session));
    assert!(
        !host
            .app
            .diagnostic_capture()
            .contains("SECRET_ROOT_CLIPBOARD34")
    );
    shutdown(&mut host.app, &host.executor);
}
