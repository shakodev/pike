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

pub(crate) fn capture<'a>(
    content: impl Into<Element<'a, Message>>,
    suspended: bool,
    active: Option<&'a iced_term::Terminal>,
    on_key: fn(&KeyEvent) -> Option<KeyAction>,
) -> Element<'a, Message> {
    Element::new(Capture {
        content: content.into(),
        suspended,
        active,
        on_key,
        divider: None,
    })
}

pub(crate) fn divider_focus<'a>(
    content: impl Into<Element<'a, Message>>,
    node: &'a iced::widget::pane_grid::Node,
    active: Option<&'a iced_term::Terminal>,
) -> Element<'a, Message> {
    Element::new(Capture {
        content: content.into(),
        suspended: true,
        active,
        on_key: resize_key,
        divider: Some(node),
    })
}

struct Capture<'a> {
    content: Element<'a, Message>,
    suspended: bool,
    active: Option<&'a iced_term::Terminal>,
    on_key: fn(&KeyEvent) -> Option<KeyAction>,
    divider: Option<&'a iced::widget::pane_grid::Node>,
}

#[derive(Default)]
struct State {
    blurred: bool,
    pressed_owner: Option<u64>,
}

/// Single shared interception boundary for future R26 command mapping. None
/// delegates untouched; Consume swallows releases without repeating an action.
pub(crate) enum KeyAction {
    Consume,
    Dispatch(Control),
}

pub(crate) fn resize_key(event: &KeyEvent) -> Option<KeyAction> {
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
    Some(if pressed {
        KeyAction::Dispatch(Control::ResizeStep(step))
    } else {
        KeyAction::Consume
    })
}

impl Widget<Message, iced::Theme, iced::Renderer> for Capture<'_> {
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
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Window(iced::window::Event::Unfocused) => {
                state.blurred = true;
                state.pressed_owner = None;
            }
            Event::Window(iced::window::Event::Focused) => state.blurred = false,
            _ => {}
        }
        // Read at dispatch time: Opened/Preedit can update the terminal's IME
        // owner without an application message or a rebuilt parent view.
        if !self.suspended
            && !state.blurred
            && self.active.is_none_or(|terminal| !terminal.is_composing())
            && let Event::Keyboard(key) = event
            && let Some(action) = (self.on_key)(key)
        {
            if let KeyAction::Dispatch(control) = action {
                let control = match control {
                    Control::ResizeStep(step) => {
                        if let Some(active) = self.active {
                            Control::ResizeKey(active.id, step)
                        } else {
                            return;
                        }
                    }
                    control => control,
                };
                shell.publish(Message::Control(control));
            }
            shell.capture_event();
            return;
        }
        // PaneGrid itself unfocuses child terminals before inspecting gutters.
        // Remember the stable input owner for both a tap and a real drag. Hit
        // testing uses the pinned widget's exact spacing+leeway calculation.
        let preserve = if !state.blurred
            && let Some(node) = self.divider
        {
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                    state.pressed_owner = cursor.position_over(layout.bounds()).and_then(|point| {
                        let point = point - iced::Vector::new(layout.bounds().x, layout.bounds().y);
                        node.split_regions(super::GUTTER, 0.0, layout.bounds().size())
                            .values()
                            .any(|(axis, region, ratio)| {
                                axis.split_line_bounds(*region, *ratio, super::GUTTER + 8.0)
                                    .contains(point)
                            })
                            .then(|| self.active.map(|terminal| terminal.id))
                            .flatten()
                    });
                    state.pressed_owner
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    state.pressed_owner.take()
                }
                _ => None,
            }
        } else {
            None
        };
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
        if let Some(owner) = preserve
            && let Some(active) = self.active.filter(|terminal| terminal.id == owner)
        {
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                &mut iced::advanced::widget::operation::focusable::focus::<()>(
                    active.widget_id().clone(),
                ),
            );
            shell.publish(Message::Control(Control::ResizeFocus(owner)));
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
