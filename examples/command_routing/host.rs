//! Fixture-only driver of the real App, runtime tasks and full widget tree.
use iced::advanced::clipboard::{Clipboard, Kind};
use iced::advanced::widget::operation::{self, Operation, Outcome};
use iced::futures::{FutureExt, StreamExt, channel::mpsc, future};
use iced::{Executor, Point, Rectangle, Size, Task, event, mouse};
use iced_runtime::futures::subscription::{self, Tracker};
use iced_runtime::user_interface::{Cache, UserInterface};
use iced_runtime::{Action, task};
use pike::app::{App, Message};
use pike::executor::SharedExecutor;
use pike::identity::SessionId;
use std::io;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct FixtureClipboard(pub Option<String>);
impl Clipboard for FixtureClipboard {
    fn read(&self, _: Kind) -> Option<String> {
        self.0.clone()
    }
    fn write(&mut self, _: Kind, value: String) {
        self.0 = Some(value);
    }
}

pub struct Host {
    pub app: App,
    pub renderer: iced::Renderer,
    pub clipboard: FixtureClipboard,
    pub controls: usize,
    pub evidence: Vec<String>,
    executor: SharedExecutor,
    cache: Cache,
    subscriptions: Tracker,
    messages: mpsc::UnboundedReceiver<Message>,
    message_sender: mpsc::UnboundedSender<Message>,
    actions: mpsc::UnboundedReceiver<Action<Message>>,
    action_sender: mpsc::UnboundedSender<Action<Message>>,
    window: iced::window::Id,
}

impl Host {
    pub fn new(
        app: App,
        initial: Task<Message>,
        renderer: iced::Renderer,
        executor: SharedExecutor,
    ) -> Self {
        let (message_sender, messages) = mpsc::unbounded();
        let (action_sender, actions) = mpsc::unbounded();
        let mut host = Self {
            app,
            renderer,
            clipboard: FixtureClipboard::default(),
            controls: 0,
            evidence: Vec::new(),
            executor,
            cache: Cache::new(),
            subscriptions: Tracker::new(),
            messages,
            message_sender,
            actions,
            action_sender,
            window: iced::window::Id::unique(),
        };
        host.enqueue(initial);
        host.subscribe();
        host
    }

    fn enqueue(&self, task: Task<Message>) {
        if let Some(mut stream) = task::into_stream(task) {
            let sender = self.action_sender.clone();
            self.executor.spawn(async move {
                while let Some(action) = stream.next().await {
                    if sender.unbounded_send(action).is_err() {
                        break;
                    }
                }
            });
        }
    }

    /// Execute a deliberately delayed real task after later selections have
    /// completed, so focus-ordering regressions do not depend on a scheduler.
    pub fn finish(&mut self, task: Task<Message>) -> io::Result<()> {
        if let Some(mut stream) = task::into_stream(task) {
            loop {
                let action = self
                    .executor
                    .block_on(async {
                        tokio::time::timeout(Duration::from_secs(10), stream.next()).await
                    })
                    .map_err(io::Error::other)?;
                let Some(action) = action else {
                    break;
                };
                self.action(action)?;
            }
        }
        Ok(())
    }

    fn subscribe(&mut self) {
        for future in self.subscriptions.update(
            subscription::into_recipes(self.app.subscription()).into_iter(),
            self.message_sender.clone(),
        ) {
            self.executor.spawn(future);
        }
    }

    pub fn update(&mut self, message: Message) {
        self.controls += usize::from(matches!(&message, Message::Control(_)));
        let task = self.executor.enter(|| self.app.update(message));
        self.enqueue(task);
        self.subscribe();
    }

    pub fn event(&mut self, event: iced::Event, cursor: mouse::Cursor) -> event::Status {
        let mut ui = UserInterface::build(
            self.app.view(),
            Size::new(1200.0, 800.0),
            std::mem::replace(&mut self.cache, Cache::new()),
            &mut self.renderer,
        );
        let mut messages = Vec::new();
        let (_, statuses) = ui.update(
            std::slice::from_ref(&event),
            cursor,
            &mut self.renderer,
            &mut self.clipboard,
            &mut messages,
        );
        self.cache = ui.into_cache();
        let status = statuses[0];
        // Match the real runtime: subscriptions see the final widget status.
        self.subscriptions
            .broadcast(subscription::Event::Interaction {
                window: self.window,
                event,
                status,
            });
        for message in messages {
            self.update(message);
        }
        status
    }

    pub fn redraw(&mut self) {
        self.event(
            iced::Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
            mouse::Cursor::Unavailable,
        );
    }

    fn operate(&mut self, operation: &mut dyn Operation) {
        let mut ui = UserInterface::build(
            self.app.view(),
            Size::new(1200.0, 800.0),
            std::mem::replace(&mut self.cache, Cache::new()),
            &mut self.renderer,
        );
        ui.operate(&self.renderer, operation);
        self.cache = ui.into_cache();
    }

    fn action(&mut self, action: Action<Message>) -> io::Result<()> {
        match action {
            Action::Output(message) => self.update(message),
            Action::Widget(mut operation) => loop {
                self.operate(operation.as_mut());
                match operation.finish() {
                    Outcome::Chain(next) => operation = next,
                    _ => break,
                }
            },
            Action::Clipboard(iced_runtime::clipboard::Action::Read { target, channel }) => {
                let _ = channel.send(self.clipboard.read(target));
            }
            Action::Clipboard(iced_runtime::clipboard::Action::Write { target, contents }) => {
                self.clipboard.write(target, contents);
            }
            Action::Exit => {}
            _ => {
                return Err(io::Error::other(
                    "unexpected runtime action in finite fixture",
                ));
            }
        }
        Ok(())
    }

    pub fn drain(&mut self) -> io::Result<()> {
        for _ in 0..4096 {
            if let Some(Some(action)) = self.actions.next().now_or_never() {
                self.action(action)?;
            } else if let Some(Some(message)) = self.messages.next().now_or_never() {
                self.update(message);
            } else {
                return Ok(());
            }
        }
        Err(io::Error::other("fixture event budget exceeded"))
    }

    pub fn until(&mut self, mut predicate: impl FnMut(&mut Self) -> bool) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.drain()?;
            if predicate(self) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io::Error::other("actual-host fixture deadline exceeded"));
            }
            let received = self.executor.block_on(async {
                tokio::time::timeout(
                    Duration::from_millis(10),
                    future::select(self.actions.next(), self.messages.next()),
                )
                .await
            });
            match received {
                Ok(future::Either::Left((Some(action), _))) => self.action(action)?,
                Ok(future::Either::Right((Some(message), _))) => self.update(message),
                _ => {}
            }
        }
    }

    pub fn focused(&mut self, session: SessionId) -> bool {
        let id = self.app.terminal(session).unwrap().widget_id().clone();
        let mut query = operation::focusable::is_focused(id);
        self.operate(&mut operation::black_box(&mut query));
        matches!(query.finish(), Outcome::Some(true))
    }

    pub fn bounds(&mut self, session: SessionId) -> Option<Rectangle> {
        struct Bounds(iced::widget::Id, Option<Rectangle>);
        impl Operation for Bounds {
            fn traverse(&mut self, traverse: &mut dyn FnMut(&mut dyn Operation)) {
                traverse(self);
            }
            fn focusable(
                &mut self,
                id: Option<&iced::widget::Id>,
                bounds: Rectangle,
                _: &mut dyn operation::Focusable,
            ) {
                if id == Some(&self.0) {
                    self.1 = Some(bounds);
                }
            }
        }
        let id = self.app.terminal(session)?.widget_id().clone();
        let mut query = Bounds(id, None);
        self.operate(&mut query);
        query.1
    }

    pub fn click(&mut self, point: Point) {
        let cursor = mouse::Cursor::Available(point);
        for event in [
            mouse::Event::CursorMoved { position: point },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            self.event(iced::Event::Mouse(event), cursor);
        }
    }

    pub fn cleanup(&mut self) -> io::Result<Vec<String>> {
        let mut handles = Vec::new();
        for (id, _) in self.app.model().sessions().iter() {
            if let Some(terminal) = self.app.terminal(id) {
                terminal.request_close();
                handles.push((id, terminal.shutdown_handle()));
            }
        }
        let mut receipts = Vec::new();
        for (id, handle) in handles {
            let completion = self
                .executor
                .block_on(async {
                    tokio::time::timeout(Duration::from_secs(10), handle.wait()).await
                })
                .map_err(io::Error::other)?;
            receipts.push(format!("session={} {completion:?}", id.get()));
        }
        Ok(receipts)
    }
}
