//! Pointer ownership is decided on press; a local selection never emits TUI bytes.
use super::*;
use crate::HistoryScroll;

// Bound work for malformed/extreme wheel events, not retained history size.
const MAX_STEPS: f64 = 256.0;

#[derive(Debug, Clone, Default)]
pub(super) struct Wheel {
    pub(super) x: f64,
    pub(super) y: f64,
    context: Option<u32>,
}
impl Wheel {
    fn take(
        &mut self,
        delta: ScrollDelta,
        cell: Size<f32>,
        mode: TermMode,
        shift: bool,
    ) -> (i32, i32) {
        let context = (mode
            & (TermMode::MOUSE_MODE
                | TermMode::SGR_MOUSE
                | TermMode::UTF8_MOUSE
                | TermMode::ALT_SCREEN
                | TermMode::ALTERNATE_SCROLL))
            .bits()
            | u32::from(shift) << 31;
        if self.context != Some(context) {
            *self = Self {
                context: Some(context),
                ..Self::default()
            };
        }
        let (x, y) = match delta {
            ScrollDelta::Lines { x, y } => (f64::from(x), f64::from(y)),
            ScrollDelta::Pixels { x, y } => (
                f64::from(x) / f64::from(cell.width),
                f64::from(y) / f64::from(cell.height),
            ),
        };
        fn step(remainder: &mut f64, delta: f64) -> i32 {
            if !delta.is_finite() {
                return 0;
            }
            *remainder += delta.clamp(-MAX_STEPS, MAX_STEPS);
            let whole = remainder.trunc().clamp(-MAX_STEPS, MAX_STEPS);
            *remainder -= whole;
            whole as i32
        }
        (step(&mut self.x, x), step(&mut self.y, y))
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct State {
    held: u8,
    active: Option<MouseButton>,
    last_motion: Option<(MouseButton, Modifiers, TerminalGridPoint)>,
    local: Point,
    pub(super) screen: TerminalGridPoint,
    pub(super) wheel: Wheel,
}
fn mapped(button: mouse::Button) -> Option<(u8, MouseButton)> {
    match button {
        mouse::Button::Left => Some((1, MouseButton::LeftButton)),
        mouse::Button::Middle => Some((2, MouseButton::MiddleButton)),
        mouse::Button::Right => Some((4, MouseButton::RightButton)),
        _ => None,
    }
}
fn motion(button: MouseButton) -> MouseButton {
    match button {
        MouseButton::MiddleButton => MouseButton::MiddleMove,
        MouseButton::RightButton => MouseButton::RightMove,
        _ => MouseButton::LeftMove,
    }
}
impl State {
    fn active(&self) -> Option<MouseButton> {
        self.active.or_else(|| {
            [
                (1, MouseButton::LeftButton),
                (2, MouseButton::MiddleButton),
                (4, MouseButton::RightButton),
            ]
            .into_iter()
            .find_map(|(bit, b)| (self.held & bit != 0).then_some(b))
        })
    }
}
impl<'a> TerminalView<'a> {
    pub(super) fn cancel_pointer(
        state: &mut TerminalViewState,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        for (bit, button) in [
            (1, MouseButton::LeftButton),
            (2, MouseButton::MiddleButton),
            (4, MouseButton::RightButton),
        ] {
            if state.pointer.held & bit != 0 {
                commands.push(Command::MouseReport(
                    button,
                    state.keyboard_modifiers,
                    state.pointer.screen,
                    false,
                ));
            }
        }
        state.reset_pointer();
        commands
    }
    pub(super) fn has_pointer_capture(state: &TerminalViewState) -> bool {
        state.is_dragged || state.pointer.held != 0
    }
    pub(super) fn fallback_cursor(
        state: &TerminalViewState,
        origin: Point,
    ) -> Point {
        Point::new(
            origin.x + state.pointer.local.x,
            origin.y + state.pointer.local.y,
        )
    }
    pub(super) fn handle_mouse_event(
        &self,
        state: &mut TerminalViewState,
        origin: Point,
        position: Point,
        event: &mouse::Event,
    ) -> Vec<Command> {
        if !position.x.is_finite() || !position.y.is_finite() {
            return Vec::new();
        }
        let content = self.term.backend.renderable_content();
        let size = content.terminal_size;
        let mode = content.terminal_mode;
        let inside = state.bounds.is_some_and(|b| b.contains(position));
        // Captured drag-out/release coordinates stay on this pane's final cell.
        let x = (position.x - origin.x)
            .clamp(0.0, (f32::from(size.num_cols) - 0.001) * size.cell_width);
        let y = (position.y - origin.y)
            .clamp(0.0, (f32::from(size.num_lines) - 0.001) * size.cell_height);
        let screen = Backend::selection_point(x, y, &size, 0);
        state.mouse_position_on_grid = Backend::selection_point(
            x,
            y,
            &size,
            content.viewport.display_offset(),
        );
        state.pointer.screen = screen;
        state.pointer.local = Point::new(x, y);
        let shift = state.keyboard_modifiers.shift();
        let tracking = mode.intersects(TermMode::MOUSE_MODE) && !shift;
        let mut commands = Vec::new();
        match event {
            mouse::Event::ButtonPressed(button) => {
                let Some((bit, button_code)) = mapped(*button) else {
                    return commands;
                };
                if tracking && !state.is_dragged {
                    if state.pointer.held & bit == 0 {
                        commands.push(Command::MouseReport(
                            button_code,
                            state.keyboard_modifiers,
                            screen,
                            true,
                        ));
                        state.pointer.held |= bit;
                        state.pointer.active = Some(button_code);
                        state.pointer.last_motion = Some((
                            motion(button_code),
                            state.keyboard_modifiers,
                            screen,
                        ));
                    }
                    state.last_click = None;
                } else if *button == mouse::Button::Left
                    && state.pointer.held == 0
                {
                    let click = Click::new(
                        position,
                        mouse::Button::Left,
                        state.last_click,
                    );
                    let ty = match click.kind() {
                        mouse::click::Kind::Single => SelectionType::Simple,
                        mouse::click::Kind::Double => SelectionType::Semantic,
                        mouse::click::Kind::Triple => SelectionType::Lines,
                    };
                    state.last_click = Some(click);
                    state.is_dragged = true;
                    commands.push(Command::SelectStart(ty, (x, y)));
                }
            },
            mouse::Event::CursorMoved { .. } => {
                if state.is_dragged {
                    commands.push(Command::SelectUpdate((x, y)));
                } else if tracking {
                    let button = if let Some(button) = state.pointer.active() {
                        mode.intersects(
                            TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION,
                        )
                        .then_some(motion(button))
                    } else {
                        mode.contains(TermMode::MOUSE_MOTION)
                            .then_some(MouseButton::NoneMove)
                    };
                    if let Some(button) = button {
                        let report = (button, state.keyboard_modifiers, screen);
                        if state.pointer.last_motion != Some(report) {
                            commands.push(Command::MouseReport(
                                button,
                                state.keyboard_modifiers,
                                screen,
                                true,
                            ));
                            state.pointer.last_motion = Some(report);
                        }
                    }
                }
                if !mode.intersects(TermMode::MOUSE_MODE)
                    && state.keyboard_modifiers == Modifiers::COMMAND
                {
                    commands.push(Command::ProcessLink(
                        LinkAction::Hover,
                        state.mouse_position_on_grid,
                    ));
                }
            },
            mouse::Event::ButtonReleased(button) => {
                if let Some((bit, button_code)) = mapped(*button) {
                    if state.pointer.held & bit != 0 {
                        commands.push(Command::MouseReport(
                            button_code,
                            state.keyboard_modifiers,
                            screen,
                            false,
                        ));
                        state.pointer.held &= !bit;
                        if state.pointer.active == Some(button_code) {
                            state.pointer.active = None;
                        }
                        state.pointer.last_motion = None;
                    } else if *button == mouse::Button::Left && state.is_dragged
                    {
                        commands.push(Command::SelectUpdate((x, y)));
                        state.is_dragged = false;
                        if inside
                            && self.term.bindings.get_action(
                                InputKind::Mouse(mouse::Button::Left),
                                state.keyboard_modifiers,
                                mode,
                            ) == BindingAction::LinkOpen
                        {
                            commands.push(Command::ProcessLink(
                                LinkAction::Open,
                                state.mouse_position_on_grid,
                            ));
                        }
                    }
                }
            },
            mouse::Event::WheelScrolled { delta } => {
                let (horizontal, vertical) = state.pointer.wheel.take(
                    *delta,
                    Size::new(size.cell_width, size.cell_height),
                    mode,
                    shift,
                );
                if tracking {
                    for (steps, positive, negative) in [
                        (
                            vertical,
                            MouseButton::ScrollUp,
                            MouseButton::ScrollDown,
                        ),
                        (
                            horizontal,
                            MouseButton::ScrollLeft,
                            MouseButton::ScrollRight,
                        ),
                    ] {
                        let button =
                            if steps > 0 { positive } else { negative };
                        for _ in 0..steps.unsigned_abs() {
                            commands.push(Command::MouseReport(
                                button,
                                state.keyboard_modifiers,
                                screen,
                                true,
                            ));
                        }
                    }
                } else if vertical != 0 {
                    commands.push(if shift {
                        Command::ScrollHistory(HistoryScroll::Lines(vertical))
                    } else {
                        Command::Scroll(vertical)
                    });
                }
            },
            _ => {},
        }
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_line_and_logical_pixel_deltas_agree_in_both_directions() {
        let cell = Size::new(8.0, 20.0);
        for sign in [1.0, -1.0] {
            let mut lines = Wheel::default();
            let mut pixels = Wheel::default();
            for i in 0..4 {
                let a = lines.take(
                    ScrollDelta::Lines {
                        x: 0.0,
                        y: sign * 0.25,
                    },
                    cell,
                    TermMode::empty(),
                    false,
                );
                let b = pixels.take(
                    ScrollDelta::Pixels {
                        x: 0.0,
                        y: sign * 5.0,
                    },
                    cell,
                    TermMode::empty(),
                    false,
                );
                assert_eq!(a, b);
                assert_eq!(a, (0, if i == 3 { sign as i32 } else { 0 }));
            }
        }
    }
    #[test]
    fn route_changes_reset_fraction_and_extreme_values_have_bounded_work() {
        let mut wheel = Wheel::default();
        let cell = Size::new(8.0, 20.0);
        assert_eq!(
            wheel.take(
                ScrollDelta::Lines { x: 0.0, y: 0.75 },
                cell,
                TermMode::empty(),
                false
            ),
            (0, 0)
        );
        assert_eq!(
            wheel.take(
                ScrollDelta::Lines { x: 0.0, y: 0.5 },
                cell,
                TermMode::MOUSE_DRAG,
                true
            ),
            (0, 0)
        );
        assert_eq!(
            wheel.take(
                ScrollDelta::Lines {
                    x: f32::MAX,
                    y: f32::NEG_INFINITY
                },
                cell,
                TermMode::MOUSE_DRAG,
                true
            ),
            (256, 0)
        );
    }
}
