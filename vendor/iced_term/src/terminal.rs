use crate::actions::Action;
use crate::backend;
use crate::bindings::{Binding, BindingAction, BindingsLayout, InputKind};
use crate::event_bridge::{self, Delivery, Receiver};
use crate::font::TermFont;
use crate::settings::{FontSettings, Settings, ThemeSettings};
use crate::theme::{ColorPalette, Theme};
use iced::futures::stream::BoxStream;
use iced::futures::{stream, StreamExt};
use iced::widget::canvas::Cache;
use iced::Subscription;
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
    pub(crate) theme: Theme,
    pub(crate) cache: Cache,
    pub(crate) bindings: BindingsLayout,
    pub(crate) backend: backend::Backend,
    backend_event_rx: Arc<Receiver>,
}

impl Terminal {
    pub fn new(id: u64, settings: Settings) -> Result<Self> {
        let (backend_event_tx, backend_event_rx) = event_bridge::channel();
        let theme = Theme::new(settings.theme);
        let font = TermFont::new(settings.font);

        Ok(Self {
            id,
            widget_id: iced::widget::Id::unique(),
            font,
            theme,
            bindings: BindingsLayout::default(),
            cache: Cache::default(),
            backend: backend::Backend::new(
                id,
                backend_event_tx,
                settings.backend,
            )?,
            backend_event_rx,
        })
    }

    pub fn widget_id(&self) -> &iced::widget::Id {
        &self.widget_id
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

        match cmd {
            Command::ChangeTheme(color_pallete) => {
                self.theme = Theme::new(ThemeSettings::new(color_pallete));
            },
            Command::ChangeFont(font_settings) => {
                self.font = TermFont::new(font_settings);
            },
            Command::AddBindings(bindings) => {
                self.bindings.add_bindings(bindings);
            },
            Command::ProxyToBackend(cmd) => {
                action = self.backend.handle(cmd);
            },
        };

        self.sync_and_redraw();
        action
    }

    fn sync_and_redraw(&mut self) {
        self.sync_font();
        self.backend.sync();
        self.redraw();
    }

    fn sync_font(&mut self) {
        self.font.sync();
        self.backend
            .handle(backend::Command::Resize(None, Some(self.font.measure)));
    }

    fn redraw(&mut self) {
        self.cache.clear();
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
