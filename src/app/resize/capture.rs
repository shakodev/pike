//! A single pre-child interceptor, deliberately limited to resize. R26 can
//! extend this boundary. Event subscriptions run too late to prevent shell input.
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::keyboard::{Event as KeyEvent, Key, Modifiers, key::Named};
use iced::{Element, Event, Length, Rectangle, Size, Vector};

use super::ResizeStep;
use crate::app::{Control, Message};
use crate::identity::SessionId;

pub(crate) fn capture<'a>(
    content: impl Into<Element<'a, Message>>,
    suspended: bool,
    scope: Option<SessionId>,
) -> Element<'a, Message> {
    Element::new(Capture {
        content: content.into(),
        suspended,
        scope,
    })
}

struct Capture<'a> {
    content: Element<'a, Message>,
    suspended: bool,
    scope: Option<SessionId>,
}

#[derive(Default)]
struct State {
    scope: Option<SessionId>,
    composing: bool,
}

fn shortcut(event: &Event) -> Option<(ResizeStep, bool)> {
    let Event::Keyboard(event) = event else {
        return None;
    };
    let (key, modifiers, pressed) = match event {
        KeyEvent::KeyPressed { key, modifiers, .. } => (key, modifiers, true),
        KeyEvent::KeyReleased { key, modifiers, .. } => (key, modifiers, false),
        _ => return None,
    };
    if *modifiers != (Modifiers::ALT | Modifiers::SHIFT) {
        return None;
    }
    let step = match key {
        Key::Named(Named::ArrowLeft) => ResizeStep::Left,
        Key::Named(Named::ArrowRight) => ResizeStep::Right,
        Key::Named(Named::ArrowUp) => ResizeStep::Up,
        Key::Named(Named::ArrowDown) => ResizeStep::Down,
        _ => return None,
    };
    Some((step, pressed))
}

impl Widget<Message, iced::Theme, iced::Renderer> for Capture<'_> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State {
            scope: self.scope,
            composing: false,
        })
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(self.content.as_widget())]
    }
    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<State>();
        if state.scope != self.scope || self.suspended {
            *state = State {
                scope: self.scope,
                composing: false,
            };
        }
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
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::InputMethod(iced::advanced::input_method::Event::Opened)
            | Event::InputMethod(iced::advanced::input_method::Event::Preedit(_, _)) => {
                state.composing = true
            }
            Event::InputMethod(iced::advanced::input_method::Event::Closed)
            | Event::Window(iced::window::Event::Unfocused) => state.composing = false,
            _ => {}
        }
        if !self.suspended
            && !state.composing
            && let Some((step, pressed)) = shortcut(event)
        {
            if pressed {
                shell.publish(Message::Control(Control::ResizeStep(step)));
            }
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
