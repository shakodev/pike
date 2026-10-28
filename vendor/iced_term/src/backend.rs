use crate::actions::Action;
use crate::event_bridge::{EventBridgeEvent, Sender};
use crate::geometry::{same_pty_size, Geometry, TerminalSize};
use crate::settings::{BackendSettings, HistoryLimit};
use crate::snapshot::{RenderableContent, SnapshotStats};
use crate::theme::Theme;
use alacritty_terminal::event::{Event, EventListener, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Notifier};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Direction, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::{
    self, test::TermSize, viewport_to_point, Term, TermMode,
};
use alacritty_terminal::tty;
use iced::keyboard::Modifiers;
use iced_core::Size;
use std::cmp::min;
use std::io::Result;
use std::ops::Index;
use std::sync::Arc;

const URL_REGEX: &str = r#"(ipfs:|ipns:|magnet:|mailto:|gemini://|gopher://|https://|http://|news:|file://|git://|ssh:|ftp://)[^\u{0000}-\u{001F}\u{007F}-\u{009F}<>"\s{-}\^⟨⟩`]+"#;

#[derive(Debug, Clone)]
pub enum Command {
    Write(Vec<u8>),
    /// Bounded, owner-bound pending request. Accept only when handled in order.
    Paste(crate::PasteRequest),
    PasteRejected(crate::PasteError),
    ImeCommit(crate::ImeCommit),
    ImeRejected(crate::ImeError),
    Scroll(i32),
    Resize(Option<Size<f32>>, Option<Size<f32>>),
    SetHistoryLimit(HistoryLimit),
    /// The owning application supplies effective pane AND window focus.
    /// Hidden sessions must be unfocused; duplicate states never emit reports.
    FocusChanged(bool),
    SelectStart(SelectionType, (f32, f32)),
    SelectUpdate((f32, f32)),
    ProcessLink(LinkAction, Point),
    MouseReport(MouseButton, Modifiers, Point, bool),
    ProcessAlacrittyEvent(Event),
    EventBridge(EventBridgeEvent),
    InputRejected(crate::InputError),
    /// Cancel pending input and request shutdown without waiting for PTY cleanup.
    RequestShutdown,
}

#[derive(Debug, Clone)]
pub enum MouseMode {
    Sgr,
    Normal(bool),
}

impl From<TermMode> for MouseMode {
    fn from(term_mode: TermMode) -> Self {
        if term_mode.contains(TermMode::SGR_MOUSE) {
            MouseMode::Sgr
        } else if term_mode.contains(TermMode::UTF8_MOUSE) {
            MouseMode::Normal(true)
        } else {
            MouseMode::Normal(false)
        }
    }
}

#[derive(Debug, Clone)]
pub enum MouseButton {
    LeftButton = 0,
    MiddleButton = 1,
    RightButton = 2,
    LeftMove = 32,
    MiddleMove = 33,
    RightMove = 34,
    NoneMove = 35,
    ScrollUp = 64,
    ScrollDown = 65,
    Other = 99,
}

#[derive(Debug, Clone)]
pub enum LinkAction {
    Clear,
    Hover,
    Open,
}

pub struct Backend {
    id: u64,
    pending_paste: Arc<crate::paste::PasteSlot>,
    pub(crate) ime: Arc<crate::ime::Session>,
    pub(crate) theme: Theme,
    pub(crate) window_focus: crate::WindowFocus,
    term: Arc<FairMutex<Term<EventProxy>>>,
    size: TerminalSize,
    geometry: Geometry,
    last_pty_size: WindowSize,
    pty_resizes: u64,
    grid_resizes: u64,
    snapshot_dirty: bool,
    render_dirty: bool,
    notifier: Notifier,
    last_content: RenderableContent,
    pub(crate) url_regex: RegexSearch,
    event_bridge_failed: bool,
    shutdown: crate::ShutdownHandle,
    history_limit: HistoryLimit,
    deferred_resize: bool,
    visible: bool,
    snapshots: u64,
    cells_copied: u64,
    exit_code: Option<i32>,
}

impl Backend {
    pub fn new(
        id: u64,
        pty_event_proxy_sender: Sender,
        settings: BackendSettings,
    ) -> Result<Self> {
        #[cfg(windows)]
        let shell = if settings.program.is_empty() {
            if !settings.args.is_empty() {
                return Err(
                    alacritty_terminal::tty::LaunchError::InvalidProgram.into(),
                );
            }
            None
        } else {
            Some(tty::Shell::new(settings.program, settings.args))
        };
        #[cfg(not(windows))]
        let shell = Some(tty::Shell::new(settings.program, settings.args));
        let pty_config = tty::Options {
            shell,
            working_directory: settings.working_directory,
            env: settings.env,
            drain_on_exit: true,
            #[cfg(windows)]
            escape_args: settings.escape_args,
        };

        let history_limit = settings.history;
        let config = term::Config {
            scrolling_history: history_limit.lines(),
            osc52: term::Osc52::Disabled,
            ..Default::default()
        };
        let terminal_size = TerminalSize::default();
        let pty = tty::new(&pty_config, terminal_size.into(), id)?;

        let event_proxy = EventProxy(pty_event_proxy_sender.complete_on_drop());

        let term = Term::new(config, &terminal_size, event_proxy.clone());

        let term = Arc::new(FairMutex::new(term));

        let completion_proxy = event_proxy.clone();
        completion_proxy.0.expect_backend();
        let pty_event_loop =
            EventLoop::new(term.clone(), event_proxy, pty, true, false)?;

        let notifier = Notifier(pty_event_loop.channel());

        let pending_paste = Arc::new(crate::paste::PasteSlot::default());
        let ime = Arc::new(crate::ime::Session::default());
        let (completed, completion) = tokio::sync::watch::channel(None);
        let shutdown = crate::ShutdownHandle::new(
            notifier.0.clone(),
            completion_proxy.0.clone(),
            completion,
            pending_paste.clone(),
            ime.clone(),
        );
        let stopped_proxy = completion_proxy.clone();
        let completed_paste = pending_paste.clone();
        let completed_ime = ime.clone();
        pty_event_loop.spawn_disposing(
            move || stopped_proxy.0.complete_source(),
            move |result| {
                completed_paste.cancel();
                completed_ime.cancel();
                completed.send_replace(Some(result));
                completion_proxy.0.backend_completed(result);
            },
        );

        Ok(Self {
            id,
            pending_paste,
            ime,
            theme: Theme::default(),
            window_focus: crate::WindowFocus::default(),
            term: term.clone(),
            size: terminal_size,
            geometry: Geometry::default(),
            last_pty_size: terminal_size.into(),
            pty_resizes: 0,
            grid_resizes: 0,
            snapshot_dirty: true,
            render_dirty: false,
            notifier,
            last_content: RenderableContent::default(),
            url_regex: RegexSearch::new(URL_REGEX).expect("invalid url regexp"),
            event_bridge_failed: false,
            shutdown,
            history_limit,
            deferred_resize: false,
            visible: false,
            snapshots: 0,
            cells_copied: 0,
            exit_code: None,
        })
    }

    pub fn handle(&mut self, cmd: Command) -> Action {
        if matches!(cmd, Command::RequestShutdown) {
            self.request_shutdown();
            return Action::Ignore;
        }
        if let Command::ImeRejected(error) = cmd {
            return Action::ImeRejected(error);
        }
        if let Command::InputRejected(error) = cmd {
            return Action::InputRejected(error);
        }
        if let Command::PasteRejected(error) = cmd {
            return Action::PasteRejected(error);
        }
        if let Command::EventBridge(event) = cmd {
            return match event {
                EventBridgeEvent::Error(error) => {
                    self.event_bridge_failed = true;
                    self.request_shutdown();
                    Action::EventBridgeError(error)
                },
                EventBridgeEvent::Completed => Action::Ignore,
                EventBridgeEvent::BackendCompleted(completion) => {
                    self.exit_code = completion.exit_code.or(self.exit_code);
                    self.snapshot_dirty = true;
                    Action::BackendCompleted(completion)
                },
            };
        }
        if self.event_bridge_failed
            && !matches!(
                cmd,
                Command::ProcessAlacrittyEvent(
                    Event::Exit | Event::ChildExit(_)
                ) | Command::Resize(_, _)
            )
        {
            return if matches!(cmd, Command::Paste(_)) {
                self.pending_paste.cancel();
                Action::PasteRejected(crate::InputError::Closed.into())
            } else if matches!(cmd, Command::ImeCommit(_)) {
                self.ime.cancel();
                Action::ImeRejected(crate::ImeError::Input(
                    crate::InputError::Closed,
                ))
            } else if matches!(cmd, Command::Write(_)) {
                Action::InputRejected(crate::InputError::Closed)
            } else {
                Action::Ignore
            };
        }
        let mut action = Action::default();
        let term = self.term.clone();
        let mut term = term.lock();
        match cmd {
            Command::RequestShutdown => {
                unreachable!("handled before terminal lock")
            },
            Command::ImeRejected(_) => {
                unreachable!("handled before terminal lock")
            },
            Command::InputRejected(_) => {
                unreachable!("handled before terminal lock")
            },
            Command::PasteRejected(_) => {
                unreachable!("handled before terminal lock")
            },
            Command::EventBridge(_) => {
                unreachable!("handled before terminal lock")
            },
            Command::ProcessAlacrittyEvent(event) => {
                match event {
                    Event::Exit => {
                        // Exit describes the model, not resource completion.
                        action = Action::Shutdown {
                            exit_code: self.exit_code,
                        };
                        self.snapshot_dirty = true;
                    },
                    Event::Title(title) => {
                        action = Action::ChangeTitle(
                            alacritty_terminal::event::sanitize_title(&title),
                        );
                    },
                    Event::ResetTitle => action = Action::ResetTitle,
                    Event::Bell => action = Action::Bell,
                    Event::ChildExit(code) => {
                        self.exit_code = Some(code);
                        action = Action::ChildExit(code);
                    },
                    Event::PtyWrite(pty) => {
                        action = self.protocol_reply(pty.as_bytes());
                    },
                    Event::ColorRequest(index, formatter) => {
                        if let Some(fallback) = self.theme.query_color(index) {
                            action = self
                                .protocol_reply(formatter(fallback).as_bytes());
                        }
                    },
                    Event::TextAreaSizeRequest(formatter) => {
                        action = self.protocol_reply(
                            formatter(self.text_area_size()).as_bytes(),
                        );
                    },
                    Event::ClipboardStore(..) | Event::ClipboardLoad(..) => {
                        // OSC52 is disabled at the parser and bridge; public
                        // injected commands cannot access the clipboard either.
                    },
                    Event::MouseCursorDirty => {
                        self.last_content.terminal_mode = *term.mode();
                        action = Action::MouseCursorDirty;
                    },
                    Event::CursorBlinkingChange => {
                        self.last_content.terminal_mode = *term.mode();
                        let shape = term.renderable_content().cursor.shape;
                        if self.last_content.cursor_shape != shape {
                            self.last_content.cursor_shape = shape;
                            self.render_dirty |= self.visible;
                        }
                        action = Action::CursorBlinkingChanged(
                            term.cursor_style().blinking,
                        );
                    },
                    Event::Wakeup => {
                        self.snapshot_dirty = true;
                    },
                };
            },
            Command::Write(input) => {
                action = self.write(&input);
                if action == Action::Ignore && term.grid().display_offset() != 0
                {
                    term.scroll_display(Scroll::Bottom);
                    self.snapshot_dirty = true;
                }
            },
            Command::Paste(request) => {
                match request.take(self.id) {
                    Ok(Some(text)) => {
                        let bracketed =
                            term.mode().contains(TermMode::BRACKETED_PASTE);
                        action = match crate::paste::prepare(
                            &text,
                            bracketed,
                            |len| self.preflight_input(len),
                        ) {
                            Ok(bytes) => {
                                match self.notifier.0.send_input(&bytes) {
                                    Ok(()) => {
                                        if !bytes.is_empty()
                                            && term.grid().display_offset() != 0
                                        {
                                            term.scroll_display(Scroll::Bottom);
                                            self.snapshot_dirty = true;
                                        }
                                        Action::Ignore
                                    },
                                    Err(error) => {
                                        Action::PasteRejected(error.into())
                                    },
                                }
                            },
                            Err(error) => Action::PasteRejected(error),
                        };
                    },
                    Ok(None) => {}, // replayed or cancelled request
                    Err(error) => action = Action::PasteRejected(error),
                }
            },
            Command::ImeCommit(commit) => match commit.take(&self.ime) {
                Ok(Some(text)) => {
                    action = match self.notifier.0.send_input(text.as_bytes()) {
                        Ok(()) => {
                            if term.grid().display_offset() != 0 {
                                term.scroll_display(Scroll::Bottom);
                                self.snapshot_dirty = true;
                            }
                            Action::Ignore
                        },
                        Err(error) => {
                            Action::ImeRejected(crate::ImeError::Input(error))
                        },
                    };
                },
                Ok(None) => {},
                Err(error) => action = Action::ImeRejected(error),
            },
            Command::Scroll(delta) => {
                action = self.scroll(&mut term, delta);
            },
            Command::Resize(layout_size, font_measure) => {
                self.resize(&mut term, layout_size, font_measure);
            },
            Command::SetHistoryLimit(limit) => {
                if self.history_limit != limit {
                    term.set_options(term::Config {
                        scrolling_history: limit.lines(),
                        osc52: term::Osc52::Disabled,
                        ..Default::default()
                    });
                    self.history_limit = limit;
                    self.last_content.hovered_hyperlink = None;
                    self.snapshot_dirty = true;
                }
            },
            Command::FocusChanged(focused) => {
                if !focused {
                    self.window_focus.release_ime(&self.ime);
                }
                let focused =
                    focused && self.visible && self.window_focus.is_focused();
                if term.is_focused != focused {
                    term.is_focused = focused;
                    self.last_content.focused = focused;
                    self.render_dirty |= self.visible;
                    if term.mode().contains(TermMode::FOCUS_IN_OUT) {
                        action = self.protocol_reply(if focused {
                            b"\x1b[I"
                        } else {
                            b"\x1b[O"
                        });
                    }
                }
            },
            Command::SelectStart(selection_type, (x, y)) => {
                self.start_selection(&mut term, selection_type, x, y);
                self.snapshot_dirty = true;
            },
            Command::SelectUpdate((x, y)) => {
                self.update_selection(&mut term, x, y);
                self.snapshot_dirty = true;
            },
            Command::ProcessLink(link_action, point) => {
                let before = self.last_content.hovered_hyperlink.clone();
                self.process_link_action(&term, link_action, point);
                self.render_dirty |=
                    before != self.last_content.hovered_hyperlink;
            },
            Command::MouseReport(button, modifiers, point, pressed) => {
                action = self.process_mouse_report(
                    button,
                    modifiers,
                    point,
                    pressed,
                    *term.mode(),
                );
            },
        };

        action
    }

    fn process_link_action(
        &mut self,
        terminal: &Term<EventProxy>,
        link_action: LinkAction,
        point: Point,
    ) {
        match link_action {
            LinkAction::Hover => {
                self.last_content.hovered_hyperlink = self.regex_match_at(
                    terminal,
                    point,
                    &mut self.url_regex.clone(),
                );
            },
            LinkAction::Clear => {
                self.last_content.hovered_hyperlink = None;
            },
            LinkAction::Open => {
                self.open_link(terminal);
            },
        };
    }

    fn open_link(&self, terminal: &Term<EventProxy>) {
        if let Some(range) = &self.last_content.hovered_hyperlink {
            let start = range.start();
            let end = range.end();

            if start.line < terminal.topmost_line()
                || end.line > terminal.bottommost_line()
                || start.column > terminal.last_column()
                || end.column > terminal.last_column()
            {
                return;
            }
            let mut url = String::from(terminal.grid().index(*start).c);
            for indexed in terminal.grid().iter_from(*start) {
                url.push(indexed.c);
                if indexed.point == *end {
                    break;
                }
            }

            open::that(url).unwrap_or_else(|_| {
                panic!("link opening is failed");
            })
        }
    }

    fn process_mouse_report(
        &self,
        button: MouseButton,
        modifiers: Modifiers,
        point: Point,
        pressed: bool,
        mode: TermMode,
    ) -> Action {
        let mut mods = 0;
        if modifiers.contains(Modifiers::SHIFT) {
            mods += 4;
        }
        if modifiers.contains(Modifiers::ALT) {
            mods += 8;
        }
        if modifiers.contains(Modifiers::COMMAND) {
            mods += 16;
        }

        match MouseMode::from(mode) {
            MouseMode::Sgr => {
                self.sgr_mouse_report(point, button as u8 + mods, pressed)
            },
            MouseMode::Normal(is_utf8) => {
                if pressed {
                    self.normal_mouse_report(
                        point,
                        button as u8 + mods,
                        is_utf8,
                    )
                } else {
                    self.normal_mouse_report(point, 3 + mods, is_utf8)
                }
            },
        }
    }

    fn sgr_mouse_report(
        &self,
        point: Point,
        button: u8,
        pressed: bool,
    ) -> Action {
        let c = if pressed { 'M' } else { 'm' };

        let msg = format!(
            "\x1b[<{};{};{}{}",
            button,
            point.column + 1,
            point.line + 1,
            c
        );

        self.write(msg.as_bytes())
    }

    fn normal_mouse_report(
        &self,
        point: Point,
        button: u8,
        is_utf8: bool,
    ) -> Action {
        let Point { line, column } = point;
        let max_point = if is_utf8 { 2015 } else { 223 };

        if line >= max_point || column >= max_point {
            return Action::Ignore;
        }

        let mut msg = vec![b'\x1b', b'[', b'M', 32 + button];

        let mouse_pos_encode = |pos: usize| -> Vec<u8> {
            let pos = 32 + 1 + pos;
            let first = 0xC0 + pos / 64;
            let second = 0x80 + (pos & 63);
            vec![first as u8, second as u8]
        };

        if is_utf8 && column >= Column(95) {
            msg.append(&mut mouse_pos_encode(column.0));
        } else {
            msg.push(32 + 1 + column.0 as u8);
        }

        if is_utf8 && line >= 95 {
            msg.append(&mut mouse_pos_encode(line.0 as usize));
        } else {
            msg.push(32 + 1 + line.0 as u8);
        }

        self.write(&msg)
    }

    fn start_selection(
        &mut self,
        terminal: &mut Term<EventProxy>,
        selection_type: SelectionType,
        x: f32,
        y: f32,
    ) {
        let location = Self::selection_point(
            x,
            y,
            &self.size,
            terminal.grid().display_offset(),
        );
        terminal.selection = Some(Selection::new(
            selection_type,
            location,
            self.selection_side(x),
        ));
    }

    fn update_selection(
        &mut self,
        terminal: &mut Term<EventProxy>,
        x: f32,
        y: f32,
    ) {
        let display_offset = terminal.grid().display_offset();
        if let Some(ref mut selection) = terminal.selection {
            let location =
                Self::selection_point(x, y, &self.size, display_offset);
            selection.update(location, self.selection_side(x));
        }
    }

    pub fn selection_point(
        x: f32,
        y: f32,
        terminal_size: &TerminalSize,
        display_offset: usize,
    ) -> Point {
        let col = (x / terminal_size.cell_width).floor().max(0.0) as usize;
        let col = min(Column(col), Column(terminal_size.num_cols as usize - 1));

        let line = (y / terminal_size.cell_height).floor().max(0.0) as usize;
        let line = min(line, terminal_size.num_lines as usize - 1);

        viewport_to_point(display_offset, Point::new(line, col))
    }

    fn selection_side(&self, x: f32) -> Side {
        let cell_x = x.rem_euclid(self.size.cell_width);
        let half_cell_width = self.size.cell_width / 2.0;

        if cell_x > half_cell_width {
            Side::Right
        } else {
            Side::Left
        }
    }

    fn resize(
        &mut self,
        terminal: &mut Term<EventProxy>,
        layout_size: Option<Size<f32>>,
        font_measure: Option<Size<f32>>,
    ) {
        if layout_size.is_none()
            && font_measure.is_none()
            && !self.deferred_resize
        {
            return;
        }
        let Some(next) = self.geometry.update(layout_size, font_measure) else {
            return;
        };
        let closed = self.notifier.0.input_status().closed;
        // Preserve parsing geometry until the EOF drain has completed.
        if closed && self.shutdown.completion().is_none() {
            self.deferred_resize = true;
            return;
        }
        self.deferred_resize = false;
        let native = WindowSize::from(next);
        if !closed && !same_pty_size(self.last_pty_size, native) {
            self.notifier.on_resize(native);
            self.last_pty_size = native;
            self.pty_resizes = self.pty_resizes.saturating_add(1);
        }
        if self.size.num_cols != next.num_cols
            || self.size.num_lines != next.num_lines
        {
            terminal.resize(TermSize::new(
                next.num_cols as usize,
                next.num_lines as usize,
            ));
            self.grid_resizes = self.grid_resizes.saturating_add(1);
        }
        if self.size != next {
            self.size = next;
            self.snapshot_dirty = true;
        }
    }

    pub(crate) fn preflight_input(
        &self,
        bytes: usize,
    ) -> std::result::Result<(), crate::InputError> {
        self.notifier.0.preflight_input(bytes)
    }

    pub(crate) fn preflight_paste(
        &self,
        text: &str,
    ) -> std::result::Result<(), crate::PasteError> {
        let bracketed =
            self.term.lock().mode().contains(TermMode::BRACKETED_PASTE);
        let len = crate::paste::wire_len(text, bracketed)?;
        self.preflight_input(len).map_err(Into::into)
    }

    /// Bound pending UI payloads separately; this is not writer admission.
    pub(crate) fn queue_paste(&self, text: Option<String>) -> Command {
        let result = (|| -> std::result::Result<crate::PasteRequest, crate::PasteError> {
            let text = text.ok_or(crate::PasteError::ClipboardUnavailable)?;
            if self.event_bridge_failed { return Err(crate::InputError::Closed.into()); }
            self.preflight_paste(&text)?;
            self.pending_paste.stage(self.id, text)
        })();
        match result {
            Ok(request) => Command::Paste(request),
            Err(error) => Command::PasteRejected(error),
        }
    }

    fn request_shutdown(&self) {
        self.shutdown.request_close();
    }

    pub(crate) fn shutdown_handle(&self) -> crate::ShutdownHandle {
        self.shutdown.clone()
    }

    pub(crate) fn input_status(&self) -> crate::InputStatus {
        self.notifier.0.input_status()
    }

    fn write(&self, input: &[u8]) -> Action {
        match self.notifier.0.send_input(input) {
            Ok(()) => Action::Ignore,
            Err(error) => Action::InputRejected(error),
        }
    }

    fn protocol_reply(&self, input: &[u8]) -> Action {
        let action = self.write(input);
        if matches!(action, Action::InputRejected(_)) {
            // A lost ordered reply cannot leave a supposedly healthy session.
            // Close only this backend through the existing cancellation path.
            self.request_shutdown();
        }
        action
    }

    pub(crate) fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    pub(crate) fn is_focused(&self) -> bool {
        self.term.lock().is_focused
    }

    fn text_area_size(&self) -> alacritty_terminal::event::TextAreaSize {
        // CSI 14t reports virtual grid pixels in the same logical units as
        // cells, independently of monitor DPI. Round the aggregate once;
        // native per-cell integer metadata would distort fractional cells.
        let pixels = |cells: u16, cell: f32| {
            (f64::from(cells) * f64::from(cell))
                .round()
                .clamp(1.0, f64::from(u32::MAX)) as u32
        };
        alacritty_terminal::event::TextAreaSize {
            width: pixels(self.size.num_cols, self.size.cell_width),
            height: pixels(self.size.num_lines, self.size.cell_height),
        }
    }

    fn scroll(
        &mut self,
        terminal: &mut Term<EventProxy>,
        delta_value: i32,
    ) -> Action {
        if delta_value != 0 {
            let scroll = Scroll::Delta(delta_value);
            if terminal
                .mode()
                .contains(TermMode::ALTERNATE_SCROLL | TermMode::ALT_SCREEN)
            {
                let line_cmd = if delta_value > 0 { b'A' } else { b'B' };
                let repeats = delta_value.unsigned_abs() as usize;
                let bytes = repeats.saturating_mul(3);
                if let Err(error) = self.preflight_input(bytes) {
                    return Action::InputRejected(error);
                }
                let mut content = Vec::with_capacity(bytes);

                for _ in 0..repeats {
                    content.push(0x1b);
                    content.push(b'O');
                    content.push(line_cmd);
                }

                return self.write(&content);
            } else {
                let before = terminal.grid().display_offset();
                terminal.grid_mut().scroll_display(scroll);
                self.snapshot_dirty |=
                    before != terminal.grid().display_offset();
            }
        }
        Action::Ignore
    }

    pub fn selectable_content(&self) -> Option<String> {
        self.term.lock().selection_to_string()
    }

    pub fn sync(&mut self) {
        if !self.visible {
            return;
        }
        let term = self.term.clone();
        let mut term = term.lock();
        if self.deferred_resize && self.shutdown.completion().is_some() {
            self.resize(&mut term, None, None);
        }
        self.internal_sync(&mut term);
    }

    pub(crate) fn sync_if_dirty(&mut self) {
        if self.snapshot_dirty
            || (self.deferred_resize && self.shutdown.completion().is_some())
        {
            self.sync();
        }
    }

    pub(crate) fn take_render_dirty(&mut self) -> bool {
        std::mem::take(&mut self.render_dirty)
    }

    pub(crate) fn resize_counts(&self) -> (u64, u64) {
        (self.pty_resizes, self.grid_resizes)
    }

    fn internal_sync(&mut self, terminal: &mut Term<EventProxy>) {
        self.last_content.capture(terminal, self.size);
        self.snapshot_dirty = false;
        self.render_dirty = true;
        self.snapshots = self.snapshots.saturating_add(1);
        self.cells_copied = self
            .cells_copied
            .saturating_add(self.last_content.viewport.cells().len() as u64);
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    /// Seed UI cell metrics while retaining the backend's initial row/column
    /// count. There is no real pane layout yet, and a hidden child can already
    /// have exited before UI attachment. Do not reflow or notify the PTY here.
    pub(crate) fn initialize_font(&mut self, measure: Size<f32>) {
        self.size.cell_width = measure.width;
        self.size.cell_height = measure.height;
        self.geometry.initialize(measure, self.size);
    }

    pub(crate) fn set_visible(&mut self, visible: bool) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        if visible {
            self.sync();
        } else {
            self.last_content = RenderableContent::default();
        }
    }

    pub(crate) fn snapshot_stats(&self) -> SnapshotStats {
        let terminal = self.term.lock();
        SnapshotStats {
            snapshots: self.snapshots,
            cells_copied: self.cells_copied,
            viewport_cells: self.last_content.viewport.cells().len(),
            history_lines: terminal.history_size(),
            history_limit: self.history_limit,
            columns: terminal.columns(),
            screen_lines: terminal.screen_lines(),
        }
    }

    pub fn renderable_content(&self) -> &RenderableContent {
        &self.last_content
    }

    /// Based on alacritty/src/display/hint.rs > regex_match_at
    /// Retrieve the match, if the specified point is inside the content matching the regex.
    fn regex_match_at(
        &self,
        terminal: &Term<EventProxy>,
        point: Point,
        regex: &mut RegexSearch,
    ) -> Option<Match> {
        let x = visible_regex_match_iter(terminal, regex)
            .find(|rm| rm.contains(&point));
        x
    }
}

/// Copied from alacritty/src/display/hint.rs:
/// Iterate over all visible regex matches.
fn visible_regex_match_iter<'a>(
    term: &'a Term<EventProxy>,
    regex: &'a mut RegexSearch,
) -> impl Iterator<Item = Match> + 'a {
    let viewport_start = Line(-(term.grid().display_offset() as i32));
    let viewport_end = viewport_start + term.bottommost_line();
    let mut start =
        term.line_search_left(Point::new(viewport_start, Column(0)));
    let mut end = term.line_search_right(Point::new(viewport_end, Column(0)));
    start.line = start.line.max(viewport_start - 100);
    end.line = end.line.min(viewport_end + 100);

    RegexIter::new(start, end, Direction::Right, term, regex)
        .skip_while(move |rm| rm.end().line < viewport_start)
        .take_while(move |rm| rm.start().line <= viewport_end)
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.request_shutdown();
    }
}

#[derive(Clone)]
pub struct EventProxy(pub(crate) Sender);

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        self.0.send(event);
    }
}

#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod selection_tests;

#[cfg(test)]
mod protocol_tests;

#[cfg(all(test, windows))]
mod event_bridge_tests {
    use super::*;
    use crate::event_bridge::{self, Delivery, EventBridgeError};
    use std::time::Duration;

    #[test]
    fn windows_conpty_exit_reaches_a_stalled_consumer_after_flood_and_resize() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let (sender, receiver) = event_bridge::channel();
        let mut backend = Backend::new(
            5001,
            sender,
            BackendSettings {
                program: "cmd.exe".into(),
                args: vec!["/D".into(), "/Q".into()],
                ..BackendSettings::default()
            },
        )
        .unwrap();
        // Real ConPTY output while the UI side performs no receives. Resize and
        // close must acquire the same terminal mutex the parser uses.
        backend.handle(Command::Write(
            b"for /L %i in (1,1,2000) do @echo synthetic\r".to_vec(),
        ));
        for width in [640.0, 800.0, 960.0] {
            backend.handle(Command::Resize(
                Some(Size::new(width, 480.0)),
                Some(Size::new(8.0, 16.0)),
            ));
        }
        backend.handle(Command::Write(b"exit /b 37\r".to_vec()));
        let mut exit = false;
        let mut child_exit = false;
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                while let Some(delivery) = receiver.recv().await {
                    match delivery {
                        Delivery::Terminal(Event::Exit) => exit = true,
                        Delivery::Terminal(Event::ChildExit(37)) => {
                            child_exit = true
                        },
                        Delivery::Bridge(EventBridgeEvent::Error(error)) => {
                            panic!("unexpected scoped error: {error:?}")
                        },
                        _ => {},
                    }
                }
            })
            .await
            .expect("ConPTY event source must terminate");
        });
        assert!(exit && child_exit);
    }

    #[test]
    fn windows_overload_closes_only_its_backend_and_keeps_exit_deliverable() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let (sender, receiver) = event_bridge::channel();
        let mut backend = Backend::new(
            5002,
            sender.clone(),
            BackendSettings {
                program: "cmd.exe".into(),
                args: vec!["/D".into(), "/Q".into()],
                ..BackendSettings::default()
            },
        )
        .unwrap();
        let (healthy_sender, healthy_receiver) = event_bridge::channel();
        let mut healthy = Backend::new(
            5003,
            healthy_sender,
            BackendSettings {
                program: "cmd.exe".into(),
                args: vec!["/D".into(), "/Q".into()],
                ..BackendSettings::default()
            },
        )
        .unwrap();
        for _ in 0..1000 {
            sender.send(Event::PtyWrite("synthetic".into()));
        }
        // Route only the scoped error, as an app would. Avoid forwarding test
        // replies to the shell; input byte admission is separate issue #6.
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                while let Some(delivery) = receiver.recv().await {
                    if let Delivery::Bridge(EventBridgeEvent::Error(error)) =
                        delivery
                    {
                        assert_eq!(error, EventBridgeError::ControlOverflow);
                        assert_eq!(
                            backend.handle(Command::EventBridge(
                                EventBridgeEvent::Error(error)
                            )),
                            Action::EventBridgeError(error)
                        );
                    }
                }
            })
            .await
            .expect("overloaded backend must stop producing events");
        });
        assert!(backend.event_bridge_failed);
        // A second real session remains usable after the first was stopped.
        healthy
            .handle(Command::Write(b"echo synthetic\rexit /b 38\r".to_vec()));
        let mut healthy_exit = false;
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                while let Some(delivery) = healthy_receiver.recv().await {
                    match delivery {
                        Delivery::Terminal(Event::ChildExit(38)) => {
                            healthy_exit = true
                        },
                        Delivery::Bridge(EventBridgeEvent::Error(error)) => {
                            panic!("healthy session: {error:?}")
                        },
                        _ => {},
                    }
                }
            })
            .await
            .expect("other session must remain responsive");
        });
        assert!(healthy_exit);
        assert_eq!(
            backend.handle(Command::ProcessAlacrittyEvent(Event::Exit)),
            Action::Shutdown {
                exit_code: backend.exit_code()
            }
        );
    }
}
