//! Screen-coordinate mouse packets; history coordinates never enter this encoder.
use super::{Backend, MouseButton};
use crate::actions::Action;
use alacritty_terminal::index::Point;
use alacritty_terminal::term::TermMode;
use iced::keyboard::Modifiers;

#[derive(Default)]
pub(super) struct ReportedMouse {
    held: u8,
    point: Point,
    modifiers: Modifiers,
}

fn index(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::LeftButton | MouseButton::LeftMove => Some(0),
        MouseButton::MiddleButton | MouseButton::MiddleMove => Some(1),
        MouseButton::RightButton | MouseButton::RightMove => Some(2),
        _ => None,
    }
}

pub(super) fn encode(
    button: MouseButton,
    modifiers: Modifiers,
    mut point: Point,
    pressed: bool,
    mode: TermMode,
) -> Option<Vec<u8>> {
    if !mode.intersects(TermMode::MOUSE_MODE) || point.line.0 < 0 {
        return None;
    }
    let code = button as u8;
    match button {
        MouseButton::Other => return None,
        MouseButton::NoneMove if !mode.contains(TermMode::MOUSE_MOTION) => {
            return None
        },
        MouseButton::LeftMove
        | MouseButton::MiddleMove
        | MouseButton::RightMove
            if !mode
                .intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION) =>
        {
            return None
        },
        _ => {},
    }
    if !pressed
        && !matches!(
            button,
            MouseButton::LeftButton
                | MouseButton::MiddleButton
                | MouseButton::RightButton
        )
    {
        return None;
    }
    let mods = u8::from(modifiers.shift()) * 4
        + u8::from(modifiers.alt()) * 8
        + u8::from(modifiers.control()) * 16;
    if mode.contains(TermMode::SGR_MOUSE) {
        let suffix = if pressed { 'M' } else { 'm' };
        return Some(
            format!(
                "\x1b[<{};{};{}{}",
                code + mods,
                point.column.0.checked_add(1)?,
                i64::from(point.line.0) + 1,
                suffix
            )
            .into_bytes(),
        );
    }
    let utf8 = mode.contains(TermMode::UTF8_MOUSE);
    let limit = if utf8 { 2015 } else { 223 };
    if pressed {
        if point.line.0 >= limit || point.column.0 >= limit as usize {
            return None;
        }
    } else {
        // An admitted press needs a representable release even after a drag
        // beyond legacy bounds or an SGR -> legacy encoding change. Clamp only
        // releases; unsupported presses/motion/wheel must remain unreported.
        point.line.0 = point.line.0.min(limit - 1);
        point.column.0 = point.column.0.min(limit as usize - 1);
    }
    let button = if pressed { code } else { 3 };
    let mut bytes = vec![b'\x1b', b'[', b'M', 32 + button + mods];
    for value in [point.column.0, point.line.0 as usize] {
        let value = 33 + value as u32;
        if utf8 {
            // Limits above guarantee a valid one/two-byte scalar, never a surrogate.
            let mut buffer = [0; 4];
            bytes.extend_from_slice(
                char::from_u32(value)
                    .unwrap()
                    .encode_utf8(&mut buffer)
                    .as_bytes(),
            );
        } else {
            bytes.push(value as u8);
        }
    }
    Some(bytes)
}

impl Backend {
    pub(super) fn process_mouse_report(
        &mut self,
        button: MouseButton,
        modifiers: Modifiers,
        mut point: Point,
        pressed: bool,
        mode: TermMode,
    ) -> Action {
        if !mode.intersects(TermMode::MOUSE_MODE) {
            self.mouse.held = 0;
            return Action::Ignore;
        }
        if point.line.0 < 0 {
            return Action::Ignore;
        }
        point.line.0 = point.line.0.min(i32::from(self.size.num_lines) - 1);
        point.column.0 = point.column.0.min(self.size.num_cols as usize - 1);
        let bit = index(button).map(|i| 1 << i).unwrap_or(0);
        if (!pressed
            || matches!(
                button,
                MouseButton::LeftMove
                    | MouseButton::MiddleMove
                    | MouseButton::RightMove
            ))
            && self.mouse.held & bit == 0
        {
            return Action::Ignore;
        }
        let Some(packet) = encode(button, modifiers, point, pressed, mode)
        else {
            return Action::Ignore;
        };
        // A release cannot be silently lost while leaving a live peer's button
        // held. Use the existing ordered-reply failure/close policy for it.
        let action = if pressed {
            self.write(&packet)
        } else {
            self.protocol_reply(&packet)
        };
        if action == Action::Ignore {
            if pressed
                && matches!(
                    button,
                    MouseButton::LeftButton
                        | MouseButton::MiddleButton
                        | MouseButton::RightButton
                )
            {
                self.mouse.held |= bit;
            } else if !pressed {
                self.mouse.held &= !bit;
            }
            self.mouse.point = point;
            self.mouse.modifiers = modifiers;
        }
        action
    }

    pub(super) fn release_mouse(&mut self, mode: TermMode) -> Action {
        let mut packets = Vec::new();
        if mode.intersects(TermMode::MOUSE_MODE) {
            for (i, button) in [
                MouseButton::LeftButton,
                MouseButton::MiddleButton,
                MouseButton::RightButton,
            ]
            .into_iter()
            .enumerate()
            {
                if self.mouse.held & (1 << i) != 0 {
                    if let Some(packet) = encode(
                        button,
                        self.mouse.modifiers,
                        self.mouse.point,
                        false,
                        mode,
                    ) {
                        packets.extend(packet);
                    }
                }
            }
        }
        self.mouse.held = 0;
        if packets.is_empty() {
            Action::Ignore
        } else {
            self.protocol_reply(&packets)
        }
    }
}

#[cfg(test)]
impl Backend {
    pub(crate) fn fixture_vt(&mut self, input: &[u8]) {
        {
            let mut term = self.term.lock();
            let mut parser: alacritty_terminal::vte::ansi::Processor =
                Default::default();
            parser.advance(&mut *term, input);
        }
        self.snapshot_dirty = true;
        self.sync_if_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::index::{Column, Line};
    fn point(x: usize, y: i32) -> Point {
        Point::new(Line(y), Column(x))
    }
    #[test]
    fn exact_sgr_buttons_modifiers_motion_release_and_wheel() {
        let mode = TermMode::MOUSE_MOTION | TermMode::SGR_MOUSE;
        for (button, pressed, modifiers, expected) in [
            (
                MouseButton::LeftButton,
                true,
                Modifiers::empty(),
                "\x1b[<0;4;3M",
            ),
            (
                MouseButton::MiddleButton,
                true,
                Modifiers::SHIFT | Modifiers::ALT | Modifiers::CTRL,
                "\x1b[<29;4;3M",
            ),
            (
                MouseButton::RightButton,
                false,
                Modifiers::CTRL,
                "\x1b[<18;4;3m",
            ),
            (
                MouseButton::LeftMove,
                true,
                Modifiers::empty(),
                "\x1b[<32;4;3M",
            ),
            (
                MouseButton::MiddleMove,
                true,
                Modifiers::empty(),
                "\x1b[<33;4;3M",
            ),
            (
                MouseButton::RightMove,
                true,
                Modifiers::empty(),
                "\x1b[<34;4;3M",
            ),
            (
                MouseButton::NoneMove,
                true,
                Modifiers::empty(),
                "\x1b[<35;4;3M",
            ),
            (
                MouseButton::ScrollUp,
                true,
                Modifiers::empty(),
                "\x1b[<64;4;3M",
            ),
            (
                MouseButton::ScrollDown,
                true,
                Modifiers::empty(),
                "\x1b[<65;4;3M",
            ),
            (
                MouseButton::ScrollLeft,
                true,
                Modifiers::empty(),
                "\x1b[<66;4;3M",
            ),
            (
                MouseButton::ScrollRight,
                true,
                Modifiers::empty(),
                "\x1b[<67;4;3M",
            ),
        ] {
            assert_eq!(
                encode(button, modifiers, point(3, 2), pressed, mode).unwrap(),
                expected.as_bytes()
            );
        }
    }
    #[test]
    fn legacy_utf8_boundaries_and_negative_history_coordinates() {
        let mode = TermMode::MOUSE_REPORT_CLICK;
        assert_eq!(
            encode(
                MouseButton::LeftButton,
                Modifiers::empty(),
                point(3, 2),
                true,
                mode
            )
            .unwrap(),
            b"\x1b[M $#"
        );
        assert_eq!(
            encode(
                MouseButton::RightButton,
                Modifiers::ALT,
                point(3, 2),
                false,
                mode
            )
            .unwrap(),
            b"\x1b[M+$#"
        );
        assert!(encode(
            MouseButton::LeftButton,
            Modifiers::empty(),
            point(223, 0),
            true,
            mode
        )
        .is_none());
        assert!(encode(
            MouseButton::LeftButton,
            Modifiers::empty(),
            point(0, -1),
            true,
            mode | TermMode::SGR_MOUSE
        )
        .is_none());
        assert_eq!(
            encode(
                MouseButton::LeftButton,
                Modifiers::empty(),
                point(95, 95),
                true,
                mode | TermMode::UTF8_MOUSE
            )
            .unwrap(),
            b"\x1b[M \xc2\x80\xc2\x80"
        );
        assert!(encode(
            MouseButton::LeftButton,
            Modifiers::empty(),
            point(2015, 0),
            true,
            mode | TermMode::UTF8_MOUSE
        )
        .is_none());
    }
    #[test]
    fn encoding_flags_alone_do_not_enable_tracking_or_drag() {
        assert!(encode(
            MouseButton::LeftMove,
            Modifiers::empty(),
            point(0, 0),
            false,
            TermMode::MOUSE_MOTION | TermMode::SGR_MOUSE
        )
        .is_none());
        for mode in
            [TermMode::empty(), TermMode::SGR_MOUSE, TermMode::UTF8_MOUSE]
        {
            assert!(encode(
                MouseButton::LeftButton,
                Modifiers::empty(),
                point(0, 0),
                true,
                mode
            )
            .is_none());
        }
        assert!(encode(
            MouseButton::LeftMove,
            Modifiers::empty(),
            point(0, 0),
            true,
            TermMode::MOUSE_REPORT_CLICK
        )
        .is_none());
        assert!(encode(
            MouseButton::NoneMove,
            Modifiers::empty(),
            point(0, 0),
            true,
            TermMode::MOUSE_DRAG
        )
        .is_none());
    }

    #[test]
    fn mandatory_release_clamps_both_legacy_limits_without_changing_presses() {
        for (flags, expected) in [
            (TermMode::empty(), &b"\x1b[M#\xff\xff"[..]),
            (TermMode::UTF8_MOUSE, &b"\x1b[M#\xdf\xbf\xdf\xbf"[..]),
        ] {
            let mode = TermMode::MOUSE_MOTION | flags;
            let outside = point(usize::MAX, i32::MAX);
            for button in [
                MouseButton::LeftButton,
                MouseButton::LeftMove,
                MouseButton::NoneMove,
                MouseButton::ScrollDown,
            ] {
                assert!(encode(
                    button,
                    Modifiers::empty(),
                    outside,
                    true,
                    mode
                )
                .is_none());
            }
            assert_eq!(
                encode(
                    MouseButton::LeftButton,
                    Modifiers::empty(),
                    outside,
                    false,
                    mode
                )
                .unwrap(),
                expected
            );
        }
    }
}
