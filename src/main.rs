#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use iced::widget::{button, column, container, row, text};
use iced::{Element, Fill, Subscription, Task};
use iced_term::actions::Action;
use iced_term::{Command, Event, Terminal, TerminalView};
use pike::launch::{CancelLaunch, LaunchError, LaunchPool, PreparedSession, RequestId, SessionId};

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .executor::<pike::executor::SharedExecutor>()
        .subscription(App::subscription)
        .title("Pike")
        .window_size((960.0, 640.0))
        .exit_on_close_request(false)
        .run()
}

#[derive(Default)]
struct App {
    launches: LaunchPool,
    target: Target,
    exiting: bool,
    notice: Option<String>,
    window_focus: iced_term::WindowFocus,
}

#[derive(Default)]
enum Target {
    #[default]
    Closed,
    Starting {
        request: RequestId,
        session: SessionId,
        cancel: CancelLaunch,
    },
    Failed(LaunchError),
    Running(Box<Terminal>),
}

enum Message {
    Control(Control),
    Launched(
        RequestId,
        SessionId,
        Result<Box<PreparedSession>, LaunchError>,
    ),
    Terminal(Event),
    WindowFocused(bool),
    CloseWindow,
    ShutdownComplete,
}

#[derive(Clone, Copy)]
enum Control {
    Open,
    Close,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let mut app = Self::default();
        let task = app.open();
        (app, task)
    }

    fn open(&mut self) -> Task<Message> {
        if self.exiting || matches!(self.target, Target::Starting { .. } | Target::Running(_)) {
            return Task::none();
        }
        match self.launches.start(Default::default()) {
            Ok(launch) => {
                let request = launch.request;
                self.notice = None;
                let session = launch.session;
                self.target = Target::Starting {
                    request,
                    session,
                    cancel: launch.cancellation(),
                };
                Task::perform(launch.receive(), move |result| {
                    Message::Launched(request, session, result)
                })
            }
            Err(error) => {
                self.target = Target::Failed(error);
                Task::none()
            }
        }
    }

    fn close_target(&mut self) {
        if let Target::Starting { cancel, .. } = &self.target {
            cancel.cancel();
        }
        // Dropping Running requests close; pool admission stays held until the
        // PTY worker acknowledges disposal. No UI thread joins or blocking I/O.
        self.target = Target::Closed;
        self.notice = None;
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Control(Control::Open) => return self.open(),
            Message::Control(Control::Close) => self.close_target(),
            Message::Launched(request, session, result) => {
                if self.exiting
                    || !matches!(&self.target,
                    Target::Starting { request: current, session: current_session, .. }
                    if *current == request && *current_session == session)
                {
                    // A closed/retried target cannot receive an old completion.
                    drop(result);
                    return Task::none();
                }
                match result.and_then(|ready| ready.attach(Default::default(), Default::default()))
                {
                    Ok(mut terminal) => {
                        terminal.set_window_focus(self.window_focus.clone());
                        let focus = TerminalView::focus(terminal.widget_id().clone());
                        self.target = Target::Running(Box::new(terminal));
                        return focus;
                    }
                    Err(error) => self.target = Target::Failed(error),
                }
            }
            Message::Terminal(Event::BackendCall(id, command)) => {
                if let Target::Running(terminal) = &mut self.target
                    && terminal.id == id
                {
                    match terminal.handle(Command::ProxyToBackend(command)) {
                        Action::InputRejected(error) => {
                            self.notice = Some(format!("Input was not sent: {error}"));
                        }
                        Action::EventBridgeError(_) => {
                            self.notice = Some(
                                "Terminal output stopped. Close and reopen the terminal.".into(),
                            );
                        }
                        Action::BackendCompleted(completion)
                            if matches!(
                                completion.reason,
                                iced_term::ExitReason::IoError(_) | iced_term::ExitReason::Panicked
                            ) =>
                        {
                            self.notice = Some(
                                "The terminal stopped unexpectedly. Close and reopen it.".into(),
                            );
                        }
                        _ => {}
                    }
                }
            }
            Message::WindowFocused(focused) => {
                self.window_focus.set_focused(focused);
                if !focused && let Target::Running(terminal) = &mut self.target {
                    terminal.set_focused(false);
                }
            }
            Message::CloseWindow if !self.exiting => {
                self.exiting = true;
                self.close_target();
                return Task::perform(self.launches.shutdown(), |_| Message::ShutdownComplete);
            }
            Message::CloseWindow => {}
            Message::ShutdownComplete => return iced::exit(),
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let terminal = match &self.target {
            Target::Running(terminal) => terminal.subscription().map(Message::Terminal),
            _ => Subscription::none(),
        };
        Subscription::batch([
            iced::window::close_requests().map(|_| Message::CloseWindow),
            iced::window::events().filter_map(|(_, event)| match event {
                iced::window::Event::Focused => Some(Message::WindowFocused(true)),
                iced::window::Event::Unfocused => Some(Message::WindowFocused(false)),
                _ => None,
            }),
            terminal,
        ])
    }

    fn view(&self) -> Element<'_, Message> {
        let status: Element<'_, Message> = match &self.target {
            Target::Closed => text(if self.exiting {
                "Closing terminals…"
            } else {
                "Terminal closed"
            })
            .into(),
            Target::Starting { .. } => text("Starting terminal…").into(),
            Target::Failed(error) => text(error.to_string()).into(),
            Target::Running(terminal) => TerminalView::show(terminal).map(Message::Terminal),
        };
        let controls: Element<'_, Control> = row![
            text("Pike").size(22),
            button("Open / Retry").on_press_maybe(
                (!self.exiting && matches!(self.target, Target::Closed | Target::Failed(_)))
                    .then_some(Control::Open)
            ),
            button("Close terminal").on_press_maybe(
                (!self.exiting
                    && matches!(self.target, Target::Starting { .. } | Target::Running(_)))
                .then_some(Control::Close)
            ),
        ]
        .spacing(12)
        .into();
        let mut content = column![controls.map(Message::Control), status].spacing(12);
        if let Some(notice) = &self.notice {
            content = content.push(text(notice));
        }
        container(content)
            .padding(12)
            .width(Fill)
            .height(Fill)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Executor;
    use pike::executor::SharedExecutor;
    use pike::launch::LaunchRequest;
    use std::time::Duration;

    fn starting(app: &mut App, executor: &SharedExecutor) -> LaunchRequest {
        #[cfg(windows)]
        let (program, args) = ("cmd.exe", vec!["/D", "/Q", "/C", "set /p PIKE_WAIT="]);
        #[cfg(not(windows))]
        let (program, args) = ("/bin/sh", vec!["-c", "read pike_wait"]);
        let launch = executor
            .enter(|| {
                app.launches.start(iced_term::settings::BackendSettings {
                    program: program.into(),
                    args: args.into_iter().map(str::to_owned).collect(),
                    #[cfg(windows)]
                    escape_args: false,
                    ..Default::default()
                })
            })
            .unwrap();
        app.target = Target::Starting {
            request: launch.request,
            session: launch.session,
            cancel: launch.cancellation(),
        };
        launch
    }

    #[test]
    fn late_success_and_failure_cannot_replace_a_retried_target() {
        let executor = SharedExecutor::new().unwrap();
        let mut app = App::default();
        let first = starting(&mut app, &executor);
        let ids = (first.request, first.session);
        let ready = executor.block_on(first.receive()).unwrap();
        app.close_target();
        let retry = starting(&mut app, &executor);
        let current = (retry.request, retry.session);
        assert_ne!(ids, current);
        drop(app.update(Message::Launched(ids.0, ids.1, Ok(ready))));
        drop(app.update(Message::Launched(
            ids.0,
            ids.1,
            Err(LaunchError::WorkerFailed),
        )));
        assert!(
            matches!(app.target, Target::Starting { request, session, .. }
            if (request, session) == current)
        );
        drop(retry);
        app.close_target();
        executor.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), app.launches.shutdown())
                .await
                .unwrap();
        });
    }

    #[test]
    fn terminal_errors_are_visible_and_preserve_the_terminal_model() {
        let executor = SharedExecutor::new().unwrap();
        let mut app = App::default();
        let launch = starting(&mut app, &executor);
        let ids = (launch.request, launch.session);
        let ready = executor.block_on(launch.receive()).unwrap();
        drop(app.update(Message::Launched(ids.0, ids.1, Ok(ready))));
        let id = ids.1.get();
        drop(app.update(Message::Terminal(Event::BackendCall(
            id,
            iced_term::BackendCommand::InputRejected(iced_term::InputError::Full {
                requested: 1024,
                available: 0,
            }),
        ))));
        assert!(
            app.notice
                .as_deref()
                .unwrap()
                .starts_with("Input was not sent:")
        );
        drop(app.update(Message::Terminal(Event::BackendCall(
            id,
            iced_term::BackendCommand::EventBridge(iced_term::EventBridgeEvent::Error(
                iced_term::EventBridgeError::ControlOverflow,
            )),
        ))));
        assert!(
            app.notice
                .as_deref()
                .unwrap()
                .starts_with("Terminal output stopped.")
        );
        assert!(matches!(app.target, Target::Running(_)));
        executor.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), app.launches.shutdown())
                .await
                .unwrap();
        });
    }
}
