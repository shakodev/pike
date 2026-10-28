//! Native candidate-area caching must include DPI as well as logical geometry.
use crate::core::{Rectangle, input_method::Purpose};
use winit::dpi::{LogicalPosition, LogicalSize};

#[derive(Default)]
pub(super) struct State {
    enabled: Option<(Rectangle, Purpose, f64)>,
}

// A private seam around the actual setters; tests exercise the same admission
// branch without creating an OS window or claiming native IME verification.
pub(super) trait Window {
    fn scale_factor(&self) -> f64;
    fn allow(&self, allowed: bool);
    fn cursor_area(&self, cursor: Rectangle);
    fn purpose(&self, purpose: Purpose);
}

impl Window for winit::window::Window {
    fn scale_factor(&self) -> f64 {
        winit::window::Window::scale_factor(self)
    }

    fn allow(&self, allowed: bool) {
        self.set_ime_allowed(allowed);
    }

    fn cursor_area(&self, cursor: Rectangle) {
        self.set_ime_cursor_area(
            LogicalPosition::new(cursor.x, cursor.y),
            LogicalSize::new(cursor.width, cursor.height),
        );
    }

    fn purpose(&self, purpose: Purpose) {
        self.set_ime_purpose(crate::conversion::ime_purpose(purpose));
    }
}

impl State {
    pub(super) fn enable(
        &mut self,
        window: &impl Window,
        cursor: Rectangle,
        purpose: Purpose,
    ) {
        if self.enabled.is_none() {
            window.allow(true);
        }

        // Winit converts logical coordinates inside set_ime_cursor_area. An
        // unchanged logical cursor needs that setter again after monitor DPI
        // changes, without disabling IME or disturbing in-progress preedit.
        let requested = (cursor, purpose, window.scale_factor());
        if self.enabled != Some(requested) {
            window.cursor_area(cursor);
            window.purpose(purpose);
            self.enabled = Some(requested);
        }
    }

    pub(super) fn disable(&mut self, window: &impl Window) {
        if self.enabled.take().is_some() {
            window.allow(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use winit::dpi::{PhysicalPosition, PhysicalSize};

    #[derive(Default)]
    struct Native {
        scale: Cell<f64>,
        allowed: RefCell<Vec<bool>>,
        areas: RefCell<Vec<(PhysicalPosition<i32>, PhysicalSize<u32>)>>,
        purposes: RefCell<Vec<Purpose>>,
    }

    impl Window for Native {
        fn scale_factor(&self) -> f64 {
            self.scale.get()
        }
        fn allow(&self, allowed: bool) {
            self.allowed.borrow_mut().push(allowed);
        }
        fn cursor_area(&self, cursor: Rectangle) {
            self.areas.borrow_mut().push((
                LogicalPosition::new(cursor.x, cursor.y)
                    .to_physical(self.scale.get()),
                LogicalSize::new(cursor.width, cursor.height)
                    .to_physical(self.scale.get()),
            ));
        }
        fn purpose(&self, purpose: Purpose) {
            self.purposes.borrow_mut().push(purpose);
        }
    }

    #[test]
    fn stationary_cursor_dpi_refresh_calls_native_area_without_disabling() {
        let native = Native::default();
        let mut state = State::default();
        let cursor = Rectangle {
            x: 40.0,
            y: 80.0,
            width: 8.0,
            height: 20.0,
        };
        for scale in [1.0, 1.25, 1.5, 2.0, 2.0] {
            native.scale.set(scale);
            state.enable(&native, cursor, Purpose::Terminal);
        }
        assert_eq!(*native.allowed.borrow(), [true]);
        assert_eq!(
            *native.areas.borrow(),
            [
                ((40, 80).into(), (8, 20).into()),
                ((50, 100).into(), (10, 25).into()),
                ((60, 120).into(), (12, 30).into()),
                ((80, 160).into(), (16, 40).into()),
            ]
        );
        assert_eq!(*native.purposes.borrow(), [Purpose::Terminal; 4]);
    }

    #[test]
    fn geometry_purpose_and_disable_keep_original_setter_semantics() {
        let native = Native::default();
        native.scale.set(1.25);
        let mut state = State::default();
        let cursor = Rectangle {
            x: 40.0,
            y: 80.0,
            width: 8.0,
            height: 20.0,
        };
        state.disable(&native);
        state.enable(&native, cursor, Purpose::Terminal);
        state.enable(&native, cursor, Purpose::Terminal);
        let moved = Rectangle { x: 80.0, ..cursor };
        state.enable(&native, moved, Purpose::Terminal);
        state.enable(&native, moved, Purpose::Normal);
        assert_eq!(native.areas.borrow().len(), 3);
        state.disable(&native);
        state.disable(&native);
        state.enable(&native, moved, Purpose::Terminal);
        assert_eq!(*native.allowed.borrow(), [true, false, true]);
        assert_eq!(native.areas.borrow().len(), 4);
    }
}
