//! One authority per actual window, independent of terminal/widget identity.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Bind the same handle to every terminal in a window, including hidden and
/// newly created sessions. Unknown/default focus is conservatively unfocused.
/// The window owner records real Focused/Unfocused events even without a view.
#[derive(Debug, Clone, Default)]
pub struct WindowFocus(Arc<AtomicBool>);

impl WindowFocus {
    pub fn is_focused(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn set_focused(&self, focused: bool) {
        self.0.store(focused, Ordering::Relaxed);
    }
}
