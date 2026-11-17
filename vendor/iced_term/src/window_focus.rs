//! One authority per actual window, independent of terminal/widget identity.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    focused: AtomicBool,
    input_epoch: Mutex<Arc<()>>,
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

/// Opaque immutable native-focus receipt. Returning to the same focused window
/// cannot revive a receipt invalidated by an intervening native blur.
#[derive(Clone)]
pub struct WindowInputLease(Arc<()>);

impl WindowFocus {
    pub fn input_lease(&self) -> Option<WindowInputLease> {
        let epoch =
            self.0.input_epoch.lock().unwrap_or_else(|e| e.into_inner());
        self.is_focused().then(|| WindowInputLease(epoch.clone()))
    }
    pub fn accepts_input(&self, lease: &WindowInputLease) -> bool {
        let epoch =
            self.0.input_epoch.lock().unwrap_or_else(|e| e.into_inner());
        self.is_focused() && Arc::ptr_eq(&epoch, &lease.0)
    }

    pub(crate) fn same_window(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn is_focused(&self) -> bool {
        self.0.focused.load(Ordering::Relaxed)
    }
    pub fn set_focused(&self, focused: bool) {
        let mut epoch =
            self.0.input_epoch.lock().unwrap_or_else(|e| e.into_inner());
        let previous = self.0.focused.swap(focused, Ordering::Relaxed);
        if previous != focused {
            *epoch = Arc::new(());
        }
        if previous && !focused {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_blur_focus_roundtrip_cannot_revive_delayed_input_receipts() {
        let window = WindowFocus::default();
        assert!(window.input_lease().is_none());
        window.set_focused(true);
        let first = window.input_lease().unwrap();
        assert!(window.accepts_input(&first));
        window.set_focused(true);
        assert!(
            window.accepts_input(&first),
            "redundant native focus invalidated the current owner"
        );
        window.set_focused(false);
        assert!(!window.accepts_input(&first));
        window.set_focused(true);
        assert!(
            !window.accepts_input(&first),
            "native roundtrip revived a stale receipt before App messages"
        );
        let current = window.input_lease().unwrap();
        assert!(window.accepts_input(&current));
        let other = WindowFocus::default();
        other.set_focused(true);
        assert!(!other.accepts_input(&current));
    }
}
