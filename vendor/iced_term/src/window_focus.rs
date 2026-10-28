//! One authority per actual window, independent of terminal/widget identity.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    focused: AtomicBool,
    ime: Mutex<crate::ime::Window>,
}

/// Share with every terminal in the actual window, including hidden sessions.
/// Unknown/default focus is conservatively unfocused. IME ownership shares this
/// authority so native untagged composition events cannot follow a stale pane.
#[derive(Clone, Default)]
pub struct WindowFocus(Arc<State>);

impl std::fmt::Debug for WindowFocus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowFocus")
            .field("focused", &self.is_focused())
            .finish()
    }
}

impl WindowFocus {
    pub(crate) fn same_window(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn is_focused(&self) -> bool {
        self.0.focused.load(Ordering::Relaxed)
    }
    pub fn set_focused(&self, focused: bool) {
        if self.0.focused.swap(focused, Ordering::Relaxed) && !focused {
            self.0.ime.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
    }
    pub(crate) fn claim_ime(&self, session: &Arc<crate::ime::Session>) {
        self.0
            .ime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .claim(session);
    }
    pub(crate) fn release_ime(&self, session: &Arc<crate::ime::Session>) {
        self.0
            .ime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .release(session);
    }
    pub(crate) fn reset_ime_on_redraw(&self) -> bool {
        self.0
            .ime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .disable_once()
    }
}
