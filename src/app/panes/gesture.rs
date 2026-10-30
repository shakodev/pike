//! Cancel the pinned grid's private Action with an unavailable release, keeping
//! terminal widget trees intact. Header gestures never reach terminal bodies.
use std::{cell::Cell, rc::Rc};

use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::{Element, Event, Length, Rectangle, Size, Vector};
use iced_term::{Terminal, WindowFocus};

use super::{DragAction, Gestures};
use crate::app::{Control, Message};

#[derive(Default)]
pub(super) struct Boundary(Rc<Cell<bool>>);

impl Boundary {
    pub(super) fn body<'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        Element::new(Guard {
            content: content.into(),
            blocked: self.0.clone(),
            gestures: None,
            window: None,
            active: None,
            enabled: true,
        })
    }

    pub(super) fn grid<'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
        gestures: &Gestures,
        window: &WindowFocus,
        active: Option<&'a Terminal>,
        enabled: bool,
    ) -> Element<'a, Message> {
        Element::new(Guard {
            content: content.into(),
            blocked: self.0.clone(),
            gestures: Some(gestures.clone()),
            window: Some(window.clone()),
            active,
            enabled,
        })
    }
}

struct Guard<'a> {
    content: Element<'a, Message>,
    blocked: Rc<Cell<bool>>,
    gestures: Option<Gestures>,
    window: Option<WindowFocus>,
    active: Option<&'a Terminal>,
    enabled: bool,
}

#[derive(Default)]
struct State {
    armed: Option<DragAction>,
    blurred: bool,
    escape: bool,
}

fn pointer(event: &Event) -> bool {
    matches!(event, Event::Mouse(_) | Event::Touch(_))
}

impl Widget<Message, iced::Theme, iced::Renderer> for Guard<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(self.content.as_widget())]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let Some(gestures) = &self.gestures else {
            if self.blocked.get() && pointer(event) {
                shell.capture_event();
                return;
            }
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
            return;
        };
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Window(iced::window::Event::Unfocused) => {
                state.blurred = true;
                gestures.cancel();
            }
            Event::Window(iced::window::Event::Focused) => state.blurred = false,
            Event::Mouse(mouse::Event::CursorLeft)
            | Event::Touch(iced::touch::Event::FingerLost { .. }) => gestures.cancel(),
            _ => {}
        }
        let allowed = self.enabled
            && !state.blurred
            && self.window.as_ref().is_some_and(WindowFocus::is_focused);
        let escape = matches!(
            event,
            Event::Keyboard(
                iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                } | iced::keyboard::Event::KeyReleased {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }
            )
        );
        if state.armed.is_some() && escape {
            state.escape = true;
            gestures.cancel();
        }
        if state.armed.is_some_and(|action| {
            !allowed
                || !gestures.current(action)
                || self.active.is_some_and(|terminal| terminal.is_composing())
        }) {
            self.blocked.set(true);
            let mut ignored = Vec::new();
            let mut cancellation = Shell::new(&mut ignored);
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                layout,
                mouse::Cursor::Unavailable,
                renderer,
                clipboard,
                &mut cancellation,
                viewport,
            );
            self.blocked.set(false);
            shell.request_redraw_at(cancellation.redraw_request());
            state.armed = None;
            gestures.cancel();
        }
        if state.escape && escape {
            if matches!(
                event,
                Event::Keyboard(iced::keyboard::Event::KeyReleased { .. })
            ) {
                state.escape = false;
            }
            shell.capture_event();
            return;
        }
        if !allowed && pointer(event) {
            shell.capture_event();
            return;
        }
        self.blocked.set(state.armed.is_some() && pointer(event));
        let mut messages = Vec::new();
        {
            let mut child = Shell::new(&mut messages);
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                layout,
                cursor,
                renderer,
                clipboard,
                &mut child,
                viewport,
            );
            shell.request_redraw_at(child.redraw_request());
            shell.request_input_method(child.input_method());
            if child.is_event_captured() {
                shell.capture_event();
            }
            if child.is_layout_invalid() {
                shell.invalidate_layout();
            }
            if child.are_widgets_invalid() {
                shell.invalidate_widgets();
            }
        }
        self.blocked.set(false);
        for message in messages {
            match message {
                Message::Control(Control::PaneDrag(action)) => match action.event {
                    iced::widget::pane_grid::DragEvent::Picked { .. } => {
                        if gestures.begin(action) {
                            state.armed = Some(action);
                        }
                    }
                    iced::widget::pane_grid::DragEvent::Canceled { .. } => {
                        state.armed = None;
                        gestures.cancel();
                    }
                    iced::widget::pane_grid::DragEvent::Dropped { .. } => {
                        if state
                            .armed
                            .take()
                            .is_some_and(|armed| gestures.current(armed))
                        {
                            shell.publish(Message::Control(Control::PaneDrag(action)));
                        }
                    }
                },
                message => shell.publish(message),
            }
        }
        // Pinned PaneGrid clears its action on release even when no cursor is
        // available, but emits no DragEvent in that case. Retire our matching
        // body gate too. A valid Dropped receipt has already taken `armed` and
        // keeps its admission until the App consumes that one layout action.
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                | Event::Touch(
                    iced::touch::Event::FingerLifted { .. } | iced::touch::Event::FingerLost { .. }
                )
        ) && state.armed.take().is_some()
        {
            gestures.cancel();
        }
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// TitleBar excludes its content's rectangle from the drag area (to allow
/// interactive titles). This inert label paints a bounded child but occupies
/// zero pick width, making the visible title draggable too, without a vendor
/// patch or another implementation of pane-grid hit testing.
pub(super) fn title(content: impl Into<Element<'static, Message>>) -> Element<'static, Message> {
    Element::new(Title(content.into()))
}

struct Title(Element<'static, Message>);

impl Widget<Message, iced::Theme, iced::Renderer> for Title {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Fixed(crate::app::split::HEADER))
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(self.0.as_widget())]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.0));
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let max = Size::new(
            (limits.max().width - 120.0).max(0.0),
            crate::app::split::HEADER,
        );
        let child = self.0.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(Size::ZERO, max),
        );
        layout::Node::with_children(Size::new(0.0, max.height), vec![child])
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let child = layout.children().next().unwrap();
        if let Some(clip) = child.bounds().intersection(viewport) {
            use iced::advanced::Renderer as _;
            renderer.with_layer(clip, |renderer| {
                self.0.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    child,
                    cursor,
                    &clip,
                )
            });
        }
    }
}
