use crate::actions::Action;
use crate::backend;
use crate::bindings::{Binding, BindingAction, BindingsLayout, InputKind};
use crate::event_bridge::{self, Delivery, Receiver};
use crate::font::TermFont;
use crate::settings::{BackendSettings, FontSettings, Settings, ThemeSettings};
use crate::theme::{ColorPalette, Theme};
use iced::futures::stream::BoxStream;
use iced::futures::{stream, StreamExt};
use iced::widget::canvas::Cache;
use iced::Subscription;
use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::io::Result;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Event {
    BackendCall(u64, backend::Command),
}

#[derive(Debug, Clone)]
pub enum Command {
    ChangeTheme(Box<ColorPalette>),
    ChangeFont(FontSettings),
    AddBindings(Vec<(Binding<InputKind>, BindingAction)>),
    ProxyToBackend(backend::Command),
}

pub struct Terminal {
    pub id: u64,
    widget_id: iced::widget::Id,
    pub(crate) font: TermFont,
    pub(crate) cache: Cache,
    pub(crate) bindings: BindingsLayout,
    pub(crate) backend: backend::Backend,
    backend_event_rx: Arc<Receiver>,
    cache_invalidations: Cell<u64>,
}

/// Per-session metadata counters; no sampling task or terminal text is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkStats {
    pub font_measurements: u64,
    /// Calls to the native notifier, not acknowledgements from the child.
    pub pty_resizes: u64,
    pub grid_resizes: u64,
    /// Explicit clears; Iced also regenerates geometry when layer size changes.
    pub cache_invalidations: u64,
}

/// PTY/session state only. Prepare on a blocking worker, then attach on the UI
/// thread. Dropping an unattached value requests nonblocking backend shutdown.
/// No font measurement, widget ID or canvas cache is created during preparation.
pub struct PreparedTerminal {
    id: u64,
    backend: backend::Backend,
    backend_event_rx: Arc<Receiver>,
}

impl PreparedTerminal {
    pub fn new(id: u64, settings: BackendSettings) -> Result<Self> {
        let (sender, receiver) = event_bridge::channel();
        Ok(Self {
            id,
            backend: backend::Backend::new(id, sender, settings)?,
            backend_event_rx: receiver,
        })
    }

    pub fn shutdown_handle(&self) -> crate::ShutdownHandle {
        self.backend.shutdown_handle()
    }

    /// Call on the UI thread; font/cache construction is deliberately deferred.
    pub fn attach(self, font: FontSettings, theme: ThemeSettings) -> Terminal {
        self.attach_with_visibility(font, theme, true)
    }

    /// UI-only attachment for a background session: no initial viewport copy.
    pub fn attach_hidden(
        self,
        font: FontSettings,
        theme: ThemeSettings,
    ) -> Terminal {
        self.attach_with_visibility(font, theme, false)
    }

    fn attach_with_visibility(
        mut self,
        font: FontSettings,
        theme: ThemeSettings,
        visible: bool,
    ) -> Terminal {
        let font = TermFont::new(font);
        self.backend.theme = Theme::new(theme);
        self.backend.initialize_font(font.measure);
        self.backend.set_visible(visible);
        self.backend.take_render_dirty();
        Terminal {
            id: self.id,
            widget_id: iced::widget::Id::unique(),
            font,
            bindings: BindingsLayout::default(),
            cache: Cache::default(),
            backend: self.backend,
            backend_event_rx: self.backend_event_rx,
            cache_invalidations: Cell::new(0),
        }
    }
}

impl Terminal {
    /// Synchronous compatibility constructor. Applications should prepare on a
    /// bounded blocking worker and attach on their UI thread instead.
    pub fn new(id: u64, settings: Settings) -> Result<Self> {
        Ok(PreparedTerminal::new(id, settings.backend)?
            .attach(settings.font, settings.theme))
    }

    pub fn widget_id(&self) -> &iced::widget::Id {
        &self.widget_id
    }

    /// Copy this session's authoritative selection on explicit user request.
    ///
    /// No selection or empty extracted text returns `None` without invoking the
    /// writer. The writer runs after releasing the model lock and owns the one
    /// extracted String; its result (including any clipboard error) is returned
    /// unchanged. Selection, viewport, input and other sessions are untouched.
    /// Menu callers can return an Iced clipboard Task; fallible hosts can return
    /// a Result. No copied contents are logged or retained by the adapter.
    pub fn copy_selection<T>(
        &self,
        write: impl FnOnce(String) -> T,
    ) -> Option<T> {
        self.backend
            .selectable_content()
            .filter(|text| !text.is_empty())
            .map(write)
    }

    /// Completion survives subscription cancellation and Terminal destruction.
    pub fn shutdown_handle(&self) -> crate::ShutdownHandle {
        self.backend.shutdown_handle()
    }

    pub fn request_close(&self) -> bool {
        self.backend.shutdown_handle().request_close()
    }

    /// Refresh the retained read-only model after backend completion, even when
    /// no event subscription was active. Returns None while hidden: activate
    /// first to obtain the final viewport. No terminal contents are logged.
    pub fn final_content(&mut self) -> Option<&crate::RenderableContent> {
        self.backend.shutdown_handle().completion()?;
        if !self.is_visible() {
            return None;
        }
        self.backend.sync();
        self.backend.take_render_dirty();
        self.redraw();
        Some(self.backend.renderable_content())
    }

    /// Hidden sessions still require their event subscription and handle calls
    /// for protocol/lifecycle delivery. Hiding releases their cached viewport;
    /// activation refreshes it once, including after the backend has exited.
    /// Returns a scoped rejection if the required focus-out report could not
    /// be admitted; that backend also requests close instead of losing a reply.
    pub fn set_visible(&mut self, visible: bool) -> Action {
        if self.backend.is_visible() == visible {
            return Action::Ignore;
        }
        let action = if !visible {
            self.backend.handle(backend::Command::FocusChanged(false))
        } else {
            Action::Ignore
        };
        self.backend.set_visible(visible);
        self.backend.take_render_dirty();
        self.redraw();
        action
    }

    pub fn is_visible(&self) -> bool {
        self.backend.is_visible()
    }

    /// Explicit presentation refresh. Hidden calls do no snapshot work.
    pub fn refresh(&mut self) {
        if self.is_visible() {
            self.backend.sync();
            self.backend.take_render_dirty();
            self.redraw();
        }
    }

    pub fn renderable_content(&self) -> Option<&crate::RenderableContent> {
        self.is_visible().then(|| self.backend.renderable_content())
    }

    pub fn snapshot_stats(&self) -> crate::SnapshotStats {
        self.backend.snapshot_stats()
    }

    pub fn work_stats(&self) -> WorkStats {
        let (pty_resizes, grid_resizes) = self.backend.resize_counts();
        WorkStats {
            font_measurements: self.font.measurements(),
            pty_resizes,
            grid_resizes,
            cache_invalidations: self.cache_invalidations.get(),
        }
    }

    pub fn input_status(&self) -> crate::InputStatus {
        self.backend.input_status()
    }

    /// Actual status when known; explicit cancellation/signals may have none.
    pub fn exit_code(&self) -> Option<i32> {
        self.backend.exit_code()
    }

    /// Pane focus, gated by visibility and the actual owner's WindowFocus.
    /// Workspace owners must route activation/removal, including hidden tabs.
    pub fn set_focused(&mut self, focused: bool) -> Action {
        self.handle(Command::ProxyToBackend(backend::Command::FocusChanged(
            focused
                && self.is_visible()
                && self.backend.window_focus.is_focused(),
        )))
    }

    /// Bind the actual window owner's authority before showing/activating a
    /// terminal. Share this handle with hidden sessions and later attachments.
    pub fn set_window_focus(&mut self, focus: crate::WindowFocus) -> Action {
        self.backend.window_focus = focus;
        if !self.backend.window_focus.is_focused() {
            self.set_focused(false)
        } else {
            Action::Ignore
        }
    }

    /// Advisory whole-input check. Admission repeats it atomically when handled.
    pub fn preflight_input(
        &self,
        bytes: usize,
    ) -> std::result::Result<(), crate::InputError> {
        self.backend.preflight_input(bytes)
    }

    pub fn subscription(&self) -> Subscription<Event> {
        let data = TerminalSubscriptionData {
            id: self.id,
            event_receiver: self.backend_event_rx.clone(),
        };

        Subscription::run_with(data, terminal_subscription_stream)
    }

    pub fn handle(&mut self, cmd: Command) -> Action {
        let mut action = Action::default();
        let mut repaint = false;

        match cmd {
            Command::ChangeTheme(color_pallete) => {
                self.backend.theme =
                    Theme::new(ThemeSettings::new(color_pallete));
                repaint = true;
            },
            Command::ChangeFont(font_settings) => {
                if self.font.update(font_settings) {
                    self.backend.handle(backend::Command::Resize(
                        None,
                        Some(self.font.measure),
                    ));
                    repaint = true;
                }
            },
            Command::AddBindings(bindings) => {
                self.bindings.add_bindings(bindings);
            },
            Command::ProxyToBackend(cmd) => {
                action = self.backend.handle(cmd);
            },
        };

        if !matches!(action, Action::InputRejected(_)) {
            self.backend.sync_if_dirty();
            let changed = self.backend.take_render_dirty();
            if self.is_visible() && (changed || repaint) {
                self.redraw();
            }
        }
        action
    }

    pub(crate) fn redraw(&self) {
        self.cache.clear();
        self.cache_invalidations
            .set(self.cache_invalidations.get().saturating_add(1));
    }
}

#[derive(Clone)]
struct TerminalSubscriptionData {
    id: u64,
    event_receiver: Arc<Receiver>,
}

impl Hash for TerminalSubscriptionData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

fn terminal_subscription_stream(
    data: &TerminalSubscriptionData,
) -> BoxStream<'static, Event> {
    stream::unfold(data.clone(), |data| async move {
        let command = match data.event_receiver.recv().await? {
            Delivery::Terminal(event) => {
                backend::Command::ProcessAlacrittyEvent(event)
            },
            Delivery::Bridge(event) => backend::Command::EventBridge(event),
        };
        Some((Event::BackendCall(data.id, command), data))
    })
    .fuse()
    .boxed()
}

#[cfg(test)]
mod work_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AlacrittyEvent, EventBridgeEvent};
    use iced::futures::FutureExt;

    #[test]
    fn subscription_cancellation_preserves_events_and_session_routing() {
        let (sender, receiver) = event_bridge::channel();
        let data = TerminalSubscriptionData {
            id: 42,
            event_receiver: receiver,
        };
        let mut stream = terminal_subscription_stream(&data);
        assert!(stream.next().now_or_never().is_none());
        drop(stream);
        sender.send(AlacrittyEvent::PtyWrite("reply".into()));
        let mut stream = terminal_subscription_stream(&data);
        assert!(
            matches!(stream.next().now_or_never(), Some(Some(Event::BackendCall(42, backend::Command::ProcessAlacrittyEvent(AlacrittyEvent::PtyWrite(s))))) if s == "reply")
        );
        drop(stream);
        // Closing the final output consumer makes later producer sends harmless.
        drop(data);
        sender.send(AlacrittyEvent::Wakeup);
    }

    #[test]
    fn subscription_delivers_exit_and_completion_then_is_fused() {
        let (sender, receiver) = event_bridge::channel();
        let data = TerminalSubscriptionData {
            id: 7,
            event_receiver: receiver,
        };
        let mut stream = terminal_subscription_stream(&data);
        sender.send(AlacrittyEvent::Exit);
        drop(sender);
        assert!(matches!(
            stream.next().now_or_never(),
            Some(Some(Event::BackendCall(
                7,
                backend::Command::ProcessAlacrittyEvent(AlacrittyEvent::Exit)
            )))
        ));
        assert!(matches!(
            stream.next().now_or_never(),
            Some(Some(Event::BackendCall(
                7,
                backend::Command::EventBridge(EventBridgeEvent::Completed)
            )))
        ));
        for _ in 0..10 {
            assert!(stream.next().now_or_never().unwrap().is_none());
        }
    }
}
