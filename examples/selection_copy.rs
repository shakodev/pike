//! Opt-in synthetic PTY/widget/clipboard probe; never logs copied contents.
#[cfg(windows)]
#[path = "support/native_clipboard.rs"]
mod native_clipboard;
#[path = "compatibility/report.rs"]
#[allow(dead_code)] // Reuse bounded metadata reporting; screenshots are not needed.
mod report;

use iced::advanced::renderer::Headless;
use iced::advanced::subscription::into_recipes;
use iced::advanced::widget::{Tree, operation};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::futures::{StreamExt, stream};
use iced::keyboard::{self, Modifiers};
use iced::{Event, Executor, Rectangle, Size};
use iced_term::{AlacrittyEvent, BackendCommand, Command, SelectionType, Terminal, TerminalView};
use pike::executor::SharedExecutor;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::Duration;

fn check(ok: bool, message: &'static str) -> io::Result<()> {
    if ok {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}

#[derive(Default)]
struct Clipboard {
    text: String,
    writes: usize,
    unavailable: bool,
    error: Option<io::ErrorKind>,
    #[cfg(windows)]
    native: Option<native_clipboard::Clipboard>,
}
impl Clipboard {
    fn seed(&mut self) -> io::Result<()> {
        self.writes = 0;
        self.error = None;
        self.text = "SYNTHETIC_CLIPBOARD_SENTINEL".into();
        #[cfg(windows)]
        if let Some(native) = &self.native {
            native.write(self.text.clone())?;
        }
        Ok(())
    }
    fn observed(&self) -> io::Result<String> {
        #[cfg(windows)]
        if let Some(native) = &self.native {
            return native.read();
        }
        Ok(self.text.clone())
    }
}
impl clipboard::Clipboard for Clipboard {
    fn read(&self, _: clipboard::Kind) -> Option<String> {
        None
    }
    fn write(&mut self, _: clipboard::Kind, text: String) {
        self.writes += 1;
        if self.unavailable {
            self.error = Some(io::ErrorKind::PermissionDenied);
            return;
        }
        #[cfg(windows)]
        if let Some(native) = &self.native {
            if let Err(error) = native.write(text) {
                self.error = Some(error.kind());
            }
            return;
        }
        self.text = text;
    }
}

struct Scene {
    tree: Tree,
}
impl Scene {
    fn new() -> Self {
        Self {
            tree: Tree::empty(),
        }
    }
    fn dispatch(
        &mut self,
        terminal: &Terminal,
        renderer: &iced::Renderer,
        bounds: Rectangle,
        clipboard: &mut Clipboard,
        event: Event,
    ) -> (Vec<iced_term::Event>, bool) {
        let mut view = TerminalView::show(terminal);
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            renderer,
            &layout::Limits::new(bounds.size(), bounds.size()),
        );
        let mut events = Vec::new();
        let mut shell = Shell::new(&mut events);
        view.as_widget_mut().update(
            &mut self.tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            clipboard,
            &mut shell,
            &bounds,
        );
        let captured = shell.is_event_captured();
        (events, captured)
    }
    fn focus(
        &mut self,
        terminal: &mut Terminal,
        renderer: &iced::Renderer,
        bounds: Rectangle,
        clipboard: &mut Clipboard,
    ) -> io::Result<()> {
        let (events, _) = self.dispatch(
            terminal,
            renderer,
            bounds,
            clipboard,
            Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
        );
        for iced_term::Event::BackendCall(id, command) in events {
            check(
                id == terminal.id && matches!(command, BackendCommand::Resize(..)),
                "layout emitted unexpected terminal command",
            )?;
            terminal.handle(Command::ProxyToBackend(command));
        }
        let mut view = TerminalView::show(terminal);
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            renderer,
            &layout::Limits::new(bounds.size(), bounds.size()),
        );
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            renderer,
            &mut operation::focusable::focus::<()>(terminal.widget_id().clone()),
        );
        Ok(())
    }
    fn copy(
        &mut self,
        terminal: &Terminal,
        renderer: &iced::Renderer,
        bounds: Rectangle,
        clipboard: &mut Clipboard,
    ) -> io::Result<()> {
        // Deliberately omit modifier notifications: the key owns its modifiers.
        let event = Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Character("c".into()),
            modified_key: keyboard::Key::Character("C".into()),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::CTRL | Modifiers::SHIFT,
            text: Some("c".into()),
            repeat: false,
        });
        let (events, captured) = self.dispatch(terminal, renderer, bounds, clipboard, event);
        check(captured, "Copy key was not captured")?;
        check(
            events.is_empty(),
            "Copy published terminal bytes or another backend action",
        )
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--fixture") {
        fixture()?;
        return Ok(());
    }
    let directory = std::path::absolute(
        std::env::args()
            .nth(1)
            .ok_or("expected fresh output directory")?,
    )?;
    check(!directory.exists(), "choose a fresh output directory")?;
    fs::create_dir_all(&directory)?;
    let executor = SharedExecutor::new()?;
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or("offscreen renderer unavailable")?;
    fs::write(directory.join("renderer.txt"), renderer.name())?;
    let mut clipboard = Clipboard::default();
    #[cfg(windows)]
    if std::env::args().any(|argument| argument == "--native-clipboard") {
        let native = native_clipboard::Clipboard::new()?;
        check(
            matches!(native.read(), Err(error) if error.kind() == io::ErrorKind::InvalidData),
            "native reader accepted unknown clipboard data",
        )?;
        native.write("OWNED_SYNTHETIC_PREFIX".into())?;
        check(
            native.read()? == "OWNED_SYNTHETIC_PREFIX",
            "padded owned clipboard round trip failed",
        )?;
        let foreign = native_clipboard::Clipboard::new()?;
        foreign.write("FOREIGN_SYNTHETIC_PREFIX".into())?;
        check(
            matches!(native.read(), Err(error) if error.kind() == io::ErrorKind::InvalidData),
            "native reader accepted replaced clipboard data",
        )?;
        check(
            foreign.read()? == "FOREIGN_SYNTHETIC_PREFIX",
            "foreign owner lost its own data",
        )?;
        clipboard.native = Some(native);
    }
    let mut terminals = Vec::with_capacity(2);
    let result = (|| -> io::Result<()> {
        for id in [151501, 151502] {
            let program = std::env::current_exe()?
                .into_os_string()
                .into_string()
                .map_err(|_| io::Error::other("non-Unicode fixture path"))?;
            terminals.push(Terminal::new(
                id,
                iced_term::settings::Settings {
                    backend: iced_term::settings::BackendSettings {
                        program,
                        args: vec!["--fixture".into()],
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )?);
        }
        run(
            &executor,
            &renderer,
            &mut terminals,
            &mut clipboard,
            &directory,
        )
    })();
    let handles: Vec<_> = terminals.iter().map(Terminal::shutdown_handle).collect();
    drop(terminals);
    let mut cleanup = fs::File::create(directory.join("cleanup.csv"))?;
    writeln!(cleanup, "session,reason")?;
    for (index, handle) in handles.into_iter().enumerate() {
        let completion = executor.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), handle.wait()).await
        })?;
        check(
            completion.reason == iced_term::ExitReason::Requested,
            "selection fixture cleanup was not acknowledged",
        )?;
        writeln!(cleanup, "{},Requested", 151501 + index)?;
    }
    result?;
    fs::write(directory.join("completed.txt"), "passed\n")?;
    Ok(())
}

fn fixture() -> io::Result<()> {
    no_echo()?;
    let mut output = io::stdout().lock();
    write!(
        output,
        "\x1b]2;pike:selection:boot:{}\x07",
        std::process::id()
    )?;
    output.flush()?;
    for line in io::stdin().lock().lines() {
        let command = line?;
        let text = match command.as_str() {
            "hard" => "alpha\r\nbeta",
            "soft" => "abcdefghij",
            "padding" => "alpha",
            "unicode" => "A界Be\u{301}Z",
            "wide-wrap" => "1234567界Z",
            "word" => "alpha beta!",
            "block" => "abcd\r\nefgh",
            "spaces" => "a  b",
            "empty" => " ",
            "other" => "OTHER",
            "history" => "zero\r\none\r\ntwo\r\nthree\r\nfour",
            "done" => {
                write!(output, "\x1b]2;pike:selection:done\x07")?;
                output.flush()?;
                continue;
            }
            _ => {
                return Err(io::Error::other(
                    "unexpected bytes reached controlled child",
                ));
            }
        };
        write!(
            output,
            "\x1b[0m\x1b[2J\x1b[3J\x1b[H{text}\x1b]2;pike:selection:{command}\x07"
        )?;
        output.flush()?;
    }
    Ok(())
}

#[cfg(windows)]
fn no_echo() -> io::Result<()> {
    use windows_sys::Win32::System::Console::{
        ENABLE_ECHO_INPUT, GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE, SetConsoleMode,
    };
    let mut mode = 0;
    // SAFETY: Borrow the child's console handle without closing it. `mode` is
    // valid output storage; change only echo in this isolated controlled child.
    unsafe {
        let input = GetStdHandle(STD_INPUT_HANDLE);
        if GetConsoleMode(input, &mut mode) == 0
            || SetConsoleMode(input, mode & !ENABLE_ECHO_INPUT) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
#[cfg(not(windows))]
fn no_echo() -> io::Result<()> {
    check(
        std::process::Command::new("/bin/stty")
            .arg("-echo")
            .status()?
            .success(),
        "could not disable controlled child echo",
    )
}

fn wait(executor: &SharedExecutor, terminal: &mut Terminal, prefix: &str) -> io::Result<String> {
    executor.block_on(async {
        let mut events = into_recipes(terminal.subscription())
            .pop()
            .unwrap()
            .stream(stream::pending().boxed());
        tokio::time::timeout(Duration::from_secs(15), async {
            while let Some(iced_term::Event::BackendCall(id, command)) = events.next().await {
                check(id == terminal.id, "event reached wrong selection fixture")?;
                let title = match &command {
                    BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Title(title))
                        if title.starts_with(prefix) =>
                    {
                        Some(title.clone())
                    }
                    _ => None,
                };
                terminal.handle(Command::ProxyToBackend(command));
                terminal.refresh();
                if let Some(title) = title {
                    return Ok(title);
                }
            }
            Err(io::Error::other("selection child ended before readiness"))
        })
        .await
        .map_err(io::Error::other)?
    })
}
fn send(executor: &SharedExecutor, terminal: &mut Terminal, command: &str) -> io::Result<()> {
    let newline = if cfg!(windows) { "\r" } else { "\n" };
    terminal.handle(Command::ProxyToBackend(BackendCommand::Write(
        format!("{command}{newline}").into_bytes(),
    )));
    wait(executor, terminal, &format!("pike:selection:{command}"))?;
    Ok(())
}
fn resize(terminal: &mut Terminal, columns: usize, lines: usize) -> Rectangle {
    let cell = terminal.renderable_content().unwrap().terminal_size;
    let dimensions = Size::new(
        cell.cell_width * columns as f32 + 0.01,
        cell.cell_height * lines as f32 + 0.01,
    );
    terminal.handle(Command::ProxyToBackend(BackendCommand::Resize(
        Some(dimensions),
        None,
    )));
    terminal.refresh();
    Rectangle::with_size(dimensions)
}
fn select(
    terminal: &mut Terminal,
    ty: SelectionType,
    start: (usize, usize, bool),
    end: (usize, usize, bool),
) {
    let cell = terminal.renderable_content().unwrap().terminal_size;
    let point = |(line, column, right): (usize, usize, bool)| {
        (
            cell.cell_width * (column as f32 + if right { 0.9 } else { 0.1 }),
            cell.cell_height * (line as f32 + 0.5),
        )
    };
    terminal.handle(Command::ProxyToBackend(BackendCommand::SelectStart(
        ty,
        point(start),
    )));
    terminal.handle(Command::ProxyToBackend(BackendCommand::SelectUpdate(
        point(end),
    )));
}

fn run(
    executor: &SharedExecutor,
    renderer: &iced::Renderer,
    terminals: &mut [Terminal],
    clipboard: &mut Clipboard,
    directory: &Path,
) -> io::Result<()> {
    let mut report = report::Report::new(directory)?;
    for terminal in terminals.iter_mut() {
        let title = wait(executor, terminal, "pike:selection:boot:")?;
        let pid: u32 = title
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .map_err(io::Error::other)?;
        report.record("fixture", terminal.id, pid, 0)?;
    }
    let mut scene = Scene::new();
    let mut checks = fs::File::create(directory.join("checks.csv"))?;
    writeln!(checks, "case,writes,captured,bytes,input_unchanged")?;
    use SelectionType::*;
    for (command, columns, ty, start, end, expected) in [
        (
            "hard",
            12,
            Simple,
            (0, 0, false),
            (1, 3, true),
            Some("alpha\nbeta"),
        ),
        (
            "soft",
            8,
            Simple,
            (0, 0, false),
            (1, 1, true),
            Some("abcdefghij"),
        ),
        (
            "padding",
            12,
            Simple,
            (0, 0, false),
            (0, 11, true),
            Some("alpha"),
        ),
        (
            "unicode",
            12,
            Simple,
            (0, 0, false),
            (0, 5, true),
            Some("A界Be\u{301}Z"),
        ),
        (
            "wide-wrap",
            8,
            Simple,
            (0, 0, false),
            (1, 2, true),
            Some("1234567界Z"),
        ),
        (
            "word",
            12,
            Semantic,
            (0, 7, false),
            (0, 7, true),
            Some("beta!"),
        ),
        (
            "hard",
            12,
            Lines,
            (0, 3, false),
            (1, 1, true),
            Some("alpha\nbeta\n"),
        ),
        (
            "block",
            12,
            Block,
            (0, 1, false),
            (1, 2, true),
            Some("bc\nfg"),
        ),
        (
            "spaces",
            12,
            Simple,
            (0, 1, false),
            (0, 2, true),
            Some("  "),
        ),
        ("empty", 12, Simple, (0, 0, false), (0, 11, true), None),
        ("empty", 12, Simple, (0, 0, false), (0, 0, false), None),
    ] {
        let terminal = &mut terminals[0];
        let bounds = resize(terminal, columns, 4);
        send(executor, terminal, command)?;
        select(terminal, ty, start, end);
        scene.focus(terminal, renderer, bounds, clipboard)?;
        clipboard.seed()?;
        let before = terminal.input_status();
        let work = terminal.work_stats();
        let snapshots = terminal.snapshot_stats().snapshots;
        let range = terminal.renderable_content().unwrap().selectable_range;
        scene.copy(terminal, renderer, bounds, clipboard)?;
        check(clipboard.error.is_none(), "native clipboard write failed")?;
        check(
            clipboard.writes == usize::from(expected.is_some()),
            "Copy wrote clipboard more than once or overwrote empty selection",
        )?;
        check(
            clipboard.observed()? == expected.unwrap_or("SYNTHETIC_CLIPBOARD_SENTINEL"),
            "clipboard round-trip differed from exact selection oracle",
        )?;
        check(
            terminal.input_status() == before,
            "Copy changed terminal input counters",
        )?;
        check(
            terminal.work_stats() == work && terminal.snapshot_stats().snapshots == snapshots,
            "Copy performed unrelated model/presentation work",
        )?;
        check(
            terminal.renderable_content().unwrap().selectable_range == range,
            "Copy cleared the selected session range",
        )?;
        writeln!(
            checks,
            "{command},{},true,{},true",
            clipboard.writes,
            expected.map_or(0, str::len)
        )?;
    }
    // Same-size tab-like tree replacements must copy their own session only.
    for (terminal, command, end) in [(0, "unicode", 5), (1, "other", 4)] {
        let terminal = &mut terminals[terminal];
        resize(terminal, 12, 4);
        send(executor, terminal, command)?;
        select(terminal, Simple, (0, 0, false), (0, end, true));
    }
    for (terminal, expected) in [(0, "A界Be\u{301}Z"), (1, "OTHER"), (0, "A界Be\u{301}Z")] {
        let terminal = &mut terminals[terminal];
        let bounds = resize(terminal, 12, 4);
        scene.focus(terminal, renderer, bounds, clipboard)?;
        clipboard.seed()?;
        scene.copy(terminal, renderer, bounds, clipboard)?;
        check(
            clipboard.observed()? == expected && clipboard.writes == 1,
            "view replacement copied another session",
        )?;
    }
    // Failure is an action result, not a mutation of either selected session.
    let bounds = resize(&mut terminals[0], 12, 4);
    scene.focus(&mut terminals[0], renderer, bounds, clipboard)?;
    clipboard.seed()?;
    #[cfg(windows)]
    let blocker = if clipboard.native.is_some() {
        Some(native_clipboard::Blocker::new()?)
    } else {
        None
    };
    clipboard.unavailable = true;
    #[cfg(windows)]
    if clipboard.native.is_some() {
        clipboard.unavailable = false;
    }
    scene.copy(&terminals[0], renderer, bounds, clipboard)?;
    check(
        clipboard.error.is_some() && clipboard.writes == 1,
        "unavailable clipboard action did not fail once",
    )?;
    clipboard.unavailable = false;
    #[cfg(windows)]
    if let Some(blocker) = blocker {
        blocker.finish()?;
    }
    check(
        clipboard.observed()? == "SYNTHETIC_CLIPBOARD_SENTINEL",
        "blocked clipboard write changed prior contents",
    )?;
    check(
        terminals[0].copy_selection(|text| text).as_deref() == Some("A界Be\u{301}Z")
            && terminals[1].copy_selection(|text| text).as_deref() == Some("OTHER"),
        "failed action changed selected sessions",
    )?;
    // Select history, scroll it offscreen, then copy with presentation released.
    resize(&mut terminals[0], 12, 3);
    send(executor, &mut terminals[0], "history")?;
    terminals[0].handle(Command::ProxyToBackend(BackendCommand::Scroll(100)));
    select(&mut terminals[0], Lines, (0, 0, false), (1, 0, true));
    terminals[0].handle(Command::ProxyToBackend(BackendCommand::Scroll(-100)));
    terminals[0].set_visible(false);
    clipboard.seed()?;
    terminals[0].copy_selection(|text| {
        clipboard::Clipboard::write(clipboard, clipboard::Kind::Standard, text)
    });
    check(
        clipboard.observed()? == "zero\none\n" && clipboard.writes == 1,
        "hidden off-viewport history copy was truncated",
    )?;
    terminals[0].set_visible(true);
    // A column change follows pinned Alacritty invalidation; never revive old text.
    let bounds = resize(&mut terminals[0], 6, 3);
    scene.focus(&mut terminals[0], renderer, bounds, clipboard)?;
    clipboard.seed()?;
    scene.copy(&terminals[0], renderer, bounds, clipboard)?;
    check(
        clipboard.writes == 0 && clipboard.observed()? == "SYNTHETIC_CLIPBOARD_SENTINEL",
        "column reflow recovered stale selection text",
    )?;
    for terminal in terminals {
        send(executor, terminal, "done")?;
    }
    #[cfg(windows)]
    if clipboard.native.is_some() {
        fs::write(
            directory.join("windows-clipboard.txt"),
            "passed: real CF_UNICODETEXT round trips over initialized owned prefixes with spare allocation padding, foreign/unknown clipboard rejection, actual owned-thread clipboard lock, unchanged no-selection/failed writes\n",
        )?;
    }
    report.record("passed", 0, 11, "cases")?;
    Ok(())
}
