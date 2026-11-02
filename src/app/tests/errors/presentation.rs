//! Offscreen public-widget recovery acceptance; real PTY, synthetic input.
use super::*;

#[test]
#[ignore = "requires a local graphics device; run explicitly for recovery widget acceptance"]
fn passive_banner_reserves_space_and_dismiss_click_restores_current_widget_typing() {
    use iced::advanced::{
        Layout, Shell, clipboard, layout, mouse,
        renderer::Headless,
        widget::{
            Tree,
            operation::{self, Operation, Outcome},
        },
    };
    use iced::{Element, Point, Rectangle};
    fn view(app: &App, id: SessionId) -> Element<'_, Message> {
        let terminal = iced_term::TerminalView::show(app.terminal(id).unwrap())
            .map(move |event| Message::Terminal((id, event)));
        let mut content = iced::widget::column![terminal];
        if let Some(error) = app.error_view(Owner::Session(id)) {
            content = content.push(error);
        }
        content.into()
    }
    fn dispatch(
        app: &App,
        id: SessionId,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        event: iced::Event,
        cursor: Point,
    ) -> Vec<Message> {
        let mut element = view(app, id);
        tree.diff(element.as_widget());
        let area = Size::new(640.0, 480.0);
        let node = element
            .as_widget_mut()
            .layout(tree, renderer, &layout::Limits::new(area, area));
        let mut messages = Vec::new();
        element.as_widget_mut().update(
            tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Available(cursor),
            renderer,
            &mut clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(area),
        );
        messages
    }
    let executor = SharedExecutor::new().unwrap();
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .expect("headless renderer");
    let (mut app, healthy, other) = pair(&executor);
    let before = app.terminal(healthy.session).unwrap().input_status();
    route(
        &mut app,
        healthy.session,
        B::InputRejected(iced_term::InputError::Full {
            requested: 1,
            available: 0,
        }),
    );
    assert_eq!(app.active_session(), Some(healthy.session));
    assert_eq!(
        app.terminal(healthy.session).unwrap().input_status(),
        before
    );
    let mut tree = Tree::new(view(&app, healthy.session).as_widget());
    let cursor = {
        let mut element = view(&app, healthy.session);
        let area = Size::new(640.0, 480.0);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(area, area));
        let mut focus = operation::focusable::focus::<()>(
            app.terminal(healthy.session).unwrap().widget_id().clone(),
        );
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut focus);
        let mut children = Layout::new(&node).children();
        let terminal = children.next().unwrap().bounds();
        let banner = children.next().unwrap().bounds();
        assert_eq!(banner.height, 96.0);
        assert!(terminal.y + terminal.height <= banner.y);
        assert_eq!(terminal.height + banner.height, area.height);
        Point::new(banner.x + 10.0, banner.y + 50.0)
    };
    for event in [
        iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),
        iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left),
    ] {
        let messages = dispatch(
            &app,
            healthy.session,
            &mut tree,
            &renderer,
            iced::Event::Mouse(event),
            cursor,
        );
        for message in messages {
            let task = app.update(message);
            if let Some(mut stream) = iced_runtime::task::into_stream(task) {
                while let Some(action) = executor.block_on(stream.next()) {
                    if let iced_runtime::Action::Widget(mut op) = action {
                        loop {
                            let mut element = view(&app, healthy.session);
                            tree.diff(element.as_widget());
                            let area = Size::new(640.0, 480.0);
                            let node = element.as_widget_mut().layout(
                                &mut tree,
                                &renderer,
                                &layout::Limits::new(area, area),
                            );
                            element.as_widget_mut().operate(
                                &mut tree,
                                Layout::new(&node),
                                &renderer,
                                op.as_mut(),
                            );
                            match op.finish() {
                                Outcome::Chain(next) => op = next,
                                _ => break,
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(app.latest_error(Owner::Session(healthy.session)).is_none());
    let key = iced::keyboard::Event::KeyPressed {
        key: iced::keyboard::Key::Character("x".into()),
        modified_key: iced::keyboard::Key::Character("x".into()),
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::empty(),
        text: Some("x".into()),
        repeat: false,
    };
    let messages = dispatch(
        &app,
        healthy.session,
        &mut tree,
        &renderer,
        iced::Event::Keyboard(key),
        cursor,
    );
    for message in messages {
        drop(app.update(message));
    }
    let after = app.terminal(healthy.session).unwrap().input_status();
    assert_eq!(
        after.written_bytes + after.outstanding_bytes + after.cancelled_bytes,
        before.written_bytes + before.outstanding_bytes + before.cancelled_bytes + 1
    );
    assert_eq!(app.active_session(), Some(healthy.session));
    assert_eq!(
        app.model.sessions().get(other.session).unwrap().lifecycle(),
        Lifecycle::Running
    );
    shutdown(&mut app, &executor);
}
