//! A single pre-child interceptor, deliberately limited to resize. R26 can
//! extend this boundary. Event subscriptions run too late to prevent shell input.
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::keyboard::{Event as KeyEvent, Key, Modifiers, key::Named};
use iced::{Element, Event, Length, Rectangle, Size, Vector};
use std::{cell::Cell, rc::Rc};

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
        pointer_blocked: None,
    })
}

/// Share admission across PaneGrid and its pane contents. Element::map creates
/// a fresh Shell, so Shell capture alone cannot guard mapped terminal widgets.
#[derive(Default)]
pub(crate) struct Divider(Rc<Cell<bool>>);

impl Divider {
    pub(crate) fn content<'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        Element::new(Capture {
            content: content.into(),
            suspended: true,
            active: None,
            on_key: resize_key,
            divider: None,
            pointer_blocked: Some(self.0.clone()),
        })
    }

    pub(crate) fn grid<'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
        node: &'a iced::widget::pane_grid::Node,
        active: Option<&'a iced_term::Terminal>,
    ) -> Element<'a, Message> {
        divider_focus(content, node, active, self.0.clone())
    }
}

fn divider_focus<'a>(
    content: impl Into<Element<'a, Message>>,
    node: &'a iced::widget::pane_grid::Node,
    active: Option<&'a iced_term::Terminal>,
    pointer_blocked: Rc<Cell<bool>>,
) -> Element<'a, Message> {
    Element::new(Capture {
        content: content.into(),
        suspended: true,
        active,
        on_key: resize_key,
        divider: Some(node),
        pointer_blocked: Some(pointer_blocked),
    })
}

struct Capture<'a> {
    content: Element<'a, Message>,
    suspended: bool,
    active: Option<&'a iced_term::Terminal>,
    on_key: fn(&KeyEvent) -> Option<KeyAction>,
    divider: Option<&'a iced::widget::pane_grid::Node>,
    pointer_blocked: Option<Rc<Cell<bool>>>,
}

#[derive(Default)]
struct State {
    blurred: bool,
    pressed_owner: Option<u64>,
    cancel_resize: bool,
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
        let state = tree.state.downcast_mut::<State>();
        // Latch owner loss even if another view diff restores the old owner
        // before the next event. Preserve all child terminal widget state.
        if self.divider.is_some()
            && state.pressed_owner.is_some()
            && state.pressed_owner != self.active.map(|terminal| terminal.id)
        {
            state.cancel_resize = true;
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
        if self.divider.is_none()
            && self
                .pointer_blocked
                .as_ref()
                .is_some_and(|blocked| blocked.get())
            && matches!(event, Event::Mouse(_))
        {
            shell.capture_event();
            return;
        }
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Window(iced::window::Event::Unfocused) => {
                state.blurred = true;
                state.cancel_resize |= state.pressed_owner.is_some();
            }
            Event::Window(iced::window::Event::Focused) => state.blurred = false,
            _ => {}
        }
        if self.divider.is_some()
            && state.pressed_owner.is_some()
            && state.pressed_owner
                != self
                    .active
                    .filter(|terminal| !terminal.is_composing())
                    .map(|terminal| terminal.id)
        {
            state.cancel_resize = true;
        }
        if state.cancel_resize
            && let Some(blocked) = &self.pointer_blocked
        {
            state.cancel_resize = false;
            state.pressed_owner = None;
            // The pinned PaneGrid only clears Action::Resizing on release,
            // not Unfocused. Deliver cancellation before any later movement;
            // a separate Shell keeps it from consuming the current real click.
            blocked.set(true);
            let mut messages = Vec::new();
            let mut cancellation = Shell::new(&mut messages);
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
            blocked.set(false);
            shell.request_redraw_at(cancellation.redraw_request());
            for message in messages {
                shell.publish(message);
            }
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
        let pressed = state.pressed_owner.is_some();
        let preserve = if let Some(node) = self.divider {
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                    let divider_hit = cursor.position_over(layout.bounds()).is_some_and(|point| {
                        let point = point - iced::Vector::new(layout.bounds().x, layout.bounds().y);
                        node.split_regions(super::GUTTER, 0.0, layout.bounds().size())
                            .values()
                            .any(|(axis, region, ratio)| {
                                axis.split_line_bounds(*region, *ratio, super::GUTTER + 8.0)
                                    .contains(point)
                            })
                    });
                    if divider_hit {
                        let owner = self.active.filter(|terminal| !terminal.is_composing());
                        if state.blurred || owner.is_none() {
                            shell.capture_event();
                            return;
                        }
                        state.pressed_owner = owner.map(|terminal| terminal.id);
                    }
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
        // PaneGrid forwards to terminals before testing its resize leeway.
        // Reserve the whole gesture first, including moves/releases inside a
        // terminal, so it cannot start selection or emit mouse-report bytes.
        let blocked = self
            .pointer_blocked
            .as_ref()
            .filter(|_| self.divider.is_some());
        if let Some(blocked) = blocked {
            blocked.set(pressed || preserve.is_some());
        }
        if (pressed || preserve.is_some()) && matches!(event, Event::Mouse(_)) {
            shell.capture_event();
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
        if let Some(blocked) = blocked {
            blocked.set(false);
        }
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
