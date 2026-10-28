//! Synthetic IME events through the public widget and existing PTY backend.
use iced::advanced::InputMethod;
use iced::advanced::renderer::Headless;
use iced::advanced::subscription::into_recipes;
use iced::advanced::widget::{Tree, operation};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::futures::{StreamExt, stream};
use iced::keyboard::{self, Modifiers};
use iced::{Executor, Point, Rectangle, Size};
use iced_term::{BackendCommand, Command, Event, Terminal, TerminalView};
use pike::executor::SharedExecutor;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Duration;

fn check(ok: bool, message: &'static str) -> io::Result<()> {
    if ok {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}
fn admitted(terminal: &Terminal) -> usize {
    let s = terminal.input_status();
    s.outstanding_bytes + s.written_bytes + s.cancelled_bytes
}
fn ime(event: iced::advanced::input_method::Event) -> iced::Event {
    iced::Event::InputMethod(event)
}
fn redraw() -> iced::Event {
    iced::Event::Window(iced::window::Event::RedrawRequested(
        std::time::Instant::now(),
    ))
}
fn key(key: keyboard::Key, modifiers: Modifiers, text: Option<&str>) -> iced::Event {
    iced::Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
    })
}

struct Scene {
    tree: Tree,
    bounds: Rectangle,
}
impl Scene {
    fn focus(&mut self, terminal: &Terminal, renderer: &iced::Renderer) {
        let mut view = TerminalView::show(terminal);
        self.tree.diff(view.as_widget());
        let node = view
            .as_widget_mut()
            .layout(
                &mut self.tree,
                renderer,
                &layout::Limits::new(self.bounds.size(), self.bounds.size()),
            )
            .move_to(self.bounds.position());
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            renderer,
            &mut operation::focusable::focus::<()>(terminal.widget_id().clone()),
        );
    }
    fn dispatch(
        &mut self,
        terminal: &Terminal,
        renderer: &iced::Renderer,
        event: iced::Event,
    ) -> (Vec<Event>, InputMethod, bool, bool) {
        let mut view = TerminalView::show(terminal);
        self.tree.diff(view.as_widget());
        let node = view
            .as_widget_mut()
            .layout(
                &mut self.tree,
                renderer,
                &layout::Limits::new(self.bounds.size(), self.bounds.size()),
            )
            .move_to(self.bounds.position());
        let mut events = Vec::new();
        let mut shell = Shell::new(&mut events);
        view.as_widget_mut().update(
            &mut self.tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut clipboard::Null,
            &mut shell,
            &self.bounds,
        );
        let strategy = shell.input_method().clone();
        let captured = shell.is_event_captured();
        let repaint = !matches!(shell.redraw_request(), iced::window::RedrawRequest::Wait);
        (events, strategy, captured, repaint)
    }
    fn route(
        &mut self,
        terminal: &mut Terminal,
        renderer: &iced::Renderer,
        event: iced::Event,
    ) -> io::Result<InputMethod> {
        let (events, strategy, _, _) = self.dispatch(terminal, renderer, event);
        route(terminal, events)?;
        Ok(strategy)
    }
}

fn route(terminal: &mut Terminal, events: Vec<Event>) -> io::Result<()> {
    for Event::BackendCall(id, command) in events {
        check(id == terminal.id, "wrong session origin")?;
        let replay = matches!(command, BackendCommand::ImeCommit(_));
        let action = terminal.handle(Command::ProxyToBackend(command.clone()));
        check(
            action == iced_term::actions::Action::Ignore,
            "unexpected rejected synthetic input",
        )?;
        if replay {
            check(
                terminal.handle(Command::ProxyToBackend(command))
                    == iced_term::actions::Action::Ignore,
                "commit replay failed",
            )?;
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--fixture") {
        return fixture(args[2].parse()?, Path::new(&args[3])).map_err(Into::into);
    }
    let output = std::path::absolute(args.get(1).ok_or("expected fresh output path")?)?;
    check(!output.exists(), "choose a fresh output directory")?;
    fs::create_dir_all(&output)?;
    let executor = SharedExecutor::new()?;
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or("offscreen renderer unavailable")?;
    let window = iced_term::WindowFocus::default();
    window.set_focused(true);
    let mut terminals = Vec::new();
    for index in 0..2 {
        let mut terminal = Terminal::new(
            190101 + index,
            iced_term::settings::Settings {
                backend: iced_term::settings::BackendSettings {
                    program: std::env::current_exe()?
                        .to_str()
                        .ok_or("non-Unicode fixture path")?
                        .into(),
                    args: vec![
                        "--fixture".into(),
                        index.to_string(),
                        output.to_str().ok_or("non-Unicode output path")?.into(),
                    ],
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        terminal.set_window_focus(window.clone());
        terminals.push(terminal);
    }
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        for terminal in &mut terminals {
            ready(&executor, terminal, "pike:ime:ready")?;
        }
        verify(&executor, &renderer, &mut terminals, &window, &output)?;
        for terminal in &mut terminals {
            check(
                terminal.handle(Command::ProxyToBackend(BackendCommand::Write(
                    b"!".to_vec(),
                ))) == iced_term::actions::Action::Ignore,
                "finish marker rejected",
            )?;
        }
        Ok(())
    })();
    let handles: Vec<_> = terminals.iter().map(Terminal::shutdown_handle).collect();
    if result.is_err() {
        for terminal in &terminals {
            terminal.request_close();
        }
    }
    let mut record = fs::File::create(output.join("cleanup.txt"))?;
    for (index, handle) in handles.into_iter().enumerate() {
        let completion = executor.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), handle.wait()).await
        })?;
        writeln!(record, "session={}, {completion:?}", 190101 + index)?;
        if result.is_ok() {
            check(
                completion.exit_code == Some(0) && completion.output_drained,
                "fixture failed or did not drain",
            )?;
        }
    }
    result?;
    fs::write(output.join("completed.txt"), "passed\n")?;
    Ok(())
}

fn ready(executor: &SharedExecutor, terminal: &mut Terminal, title: &str) -> io::Result<()> {
    executor.block_on(async {
        let recipe=into_recipes(terminal.subscription()).pop().ok_or_else(||io::Error::other("missing subscription"))?;
        let mut events=recipe.stream(stream::pending().boxed());
        tokio::time::timeout(Duration::from_secs(5),async {
            while let Some(Event::BackendCall(id,command))=events.next().await {
                check(id==terminal.id,"wrong readiness origin")?;
                if matches!(terminal.handle(Command::ProxyToBackend(command)),iced_term::actions::Action::ChangeTitle(ref s) if s==title) {
                    terminal.refresh();return Ok(());
                }
            }
            Err(io::Error::other("fixture ended before ready"))
        }).await.map_err(io::Error::other)?
    })
}

fn verify(
    executor: &SharedExecutor,
    renderer: &iced::Renderer,
    terminals: &mut [Terminal],
    window: &iced_term::WindowFocus,
    output: &Path,
) -> io::Result<()> {
    use iced::advanced::input_method::Event as Ime;
    let [a, b] = terminals else {
        return Err(io::Error::other("two fixture sessions required"));
    };
    let mut scene = Scene {
        tree: Tree::empty(),
        bounds: Rectangle::new(Point::new(10.5, 20.25), Size::new(480.0, 320.0)),
    };
    scene.focus(a, renderer);
    scene.route(a, renderer, redraw())?;
    let first = scene.route(a, renderer, redraw())?;
    let InputMethod::Enabled { cursor: first, .. } = first else {
        return Err(io::Error::other("initial IME disabled"));
    };
    scene.route(a, renderer, ime(Ime::Opened))?;
    scene.route(a, renderer, ime(Ime::Preedit("ni".into(), Some(0..2))))?;
    check(admitted(a) == 0, "preedit wrote PTY input")?;
    for key in [
        keyboard::Key::Character("n".into()),
        keyboard::Key::Named(keyboard::key::Named::Enter),
        keyboard::Key::Named(keyboard::key::Named::ArrowLeft),
    ] {
        let (events, _, captured, _) = scene.dispatch(
            a,
            renderer,
            self::key(key, Modifiers::empty(), Some("duplicate")),
        );
        check(captured, "composing key escaped to shortcut routing")?;
        route(a, events)?;
    }
    check(admitted(a) == 0, "composing key fallback wrote input")?;
    scene.bounds.x += 30.0;
    scene.bounds.y += 40.0;
    let InputMethod::Enabled { cursor: moved, .. } = scene.route(a, renderer, redraw())? else {
        return Err(io::Error::other("moved IME disabled"));
    };
    check(
        moved.x == first.x + 30.0 && moved.y == first.y + 40.0,
        "candidate did not follow pane origin",
    )?;
    scene.bounds.width = 240.0;
    scene.bounds.height = 160.0;
    scene.route(a, renderer, redraw())?;
    let InputMethod::Enabled {
        cursor: resized, ..
    } = scene.route(a, renderer, redraw())?
    else {
        return Err(io::Error::other("resized IME disabled"));
    };
    for scale in [1.0, 1.25, 1.5, 2.0] {
        scene.route(
            a,
            renderer,
            iced::Event::Window(iced::window::Event::Rescaled(scale)),
        )?;
        let InputMethod::Enabled { cursor, .. } = scene.route(a, renderer, redraw())? else {
            return Err(io::Error::other("DPI IME disabled"));
        };
        check(
            cursor == resized,
            "logical IME anchor applied monitor scale twice",
        )?;
    }
    scene.route(a, renderer, ime(Ime::Preedit(String::new(), None)))?;
    let (commit, _, _, _) = scene.dispatch(a, renderer, ime(Ime::Commit("你好".into())));
    scene.route(a, renderer, ime(Ime::Closed))?;
    route(a, commit)?;
    scene.route(
        a,
        renderer,
        key(keyboard::Key::Unidentified, Modifiers::empty(), None),
    )?;
    scene.route(
        a,
        renderer,
        key(keyboard::Key::Unidentified, Modifiers::empty(), Some("")),
    )?;
    scene.route(
        a,
        renderer,
        key(
            keyboard::Key::Character("e".into()),
            Modifiers::empty(),
            Some("é"),
        ),
    )?;
    scene.route(
        a,
        renderer,
        key(
            keyboard::Key::Named(keyboard::key::Named::Space),
            Modifiers::empty(),
            Some("´"),
        ),
    )?;
    scene.route(
        a,
        renderer,
        key(
            keyboard::Key::Character("q".into()),
            Modifiers::CTRL | Modifiers::ALT,
            Some("@"),
        ),
    )?;
    scene.route(
        a,
        renderer,
        key(keyboard::Key::Character("c".into()), Modifiers::CTRL, None),
    )?;
    scene.route(
        a,
        renderer,
        key(
            keyboard::Key::Character("c".into()),
            Modifiers::CTRL | Modifiers::SHIFT,
            Some("c"),
        ),
    )?;
    check(
        admitted(a) == "你好é´@\x03".len(),
        "commit/dead-key/AltGr/copy byte count differs",
    )?;
    ready(executor, a, "pike:ime:moved")?;
    let InputMethod::Enabled {
        cursor: updated, ..
    } = scene.route(a, renderer, redraw())?
    else {
        return Err(io::Error::other("moved model cursor disabled IME"));
    };
    let cell = a.renderable_content().unwrap().terminal_size;
    check(
        updated.x == scene.bounds.x + 6.0 * cell.cell_width
            && updated.y == scene.bounds.y + 3.0 * cell.cell_height,
        "candidate did not follow current model cursor",
    )?;
    scene.route(a, renderer, ime(Ime::Opened))?;
    scene.route(a, renderer, ime(Ime::Preedit("cancelled".into(), None)))?;
    let (stale, _, _, _) = scene.dispatch(a, renderer, ime(Ime::Commit("NEVER".into())));
    a.set_focused(false);
    a.set_visible(false);
    scene.focus(b, renderer);
    check(
        !scene.route(b, renderer, redraw())?.is_enabled(),
        "new owner skipped native disable boundary",
    )?;
    check(
        scene.route(b, renderer, redraw())?.is_enabled(),
        "new owner was not re-enabled",
    )?;
    scene.route(b, renderer, ime(Ime::Commit("LATE_A".into())))?;
    route(a, stale)?;
    check(admitted(b) == 0, "old composition entered new session")?;
    scene.route(b, renderer, ime(Ime::Opened))?;
    scene.route(b, renderer, ime(Ime::Preedit("blur".into(), None)))?;
    window.set_focused(false);
    scene.route(
        b,
        renderer,
        iced::Event::Window(iced::window::Event::Unfocused),
    )?;
    window.set_focused(true);
    scene.route(
        b,
        renderer,
        iced::Event::Window(iced::window::Event::Focused),
    )?;
    scene.route(b, renderer, redraw())?;
    scene.route(b, renderer, redraw())?;
    scene.route(b, renderer, ime(Ime::Commit("LATE_BLUR".into())))?;
    b.set_input_method_enabled(false);
    check(
        !scene.route(b, renderer, redraw())?.is_enabled(),
        "modal retained IME",
    )?;
    b.set_input_method_enabled(true);
    scene.route(b, renderer, redraw())?;
    scene.route(b, renderer, redraw())?;
    scene.route(b, renderer, ime(Ime::Opened))?;
    scene.route(b, renderer, ime(Ime::Preedit("jie".into(), Some(0..3))))?;
    scene.route(b, renderer, ime(Ime::Preedit(String::new(), None)))?;
    scene.route(b, renderer, ime(Ime::Commit("界".into())))?;
    scene.route(b, renderer, ime(Ime::Closed))?;
    check(
        admitted(b) == "界".len(),
        "new owner composition bytes differ",
    )?;
    let (events, _, _, repaint) = scene.dispatch(b, renderer, redraw());
    route(b, events)?;
    check(!repaint, "settled IME scheduled an idle redraw")?;
    fs::write(
        output.join("observations.txt"),
        format!(
            "candidate_initial={first:?}\ncandidate_moved={moved:?}\ncandidate_resized={resized:?}\ncandidate_model_updated={updated:?}\nA_bytes={}\nB_bytes={}\nsynthetic_preedit/commit/replay/cancel/owner/blur/modal/shortcut/dead-key/AltGr passed\n",
            admitted(a),
            admitted(b)
        ),
    )?;
    Ok(())
}

fn fixture(index: usize, output: &Path) -> io::Result<()> {
    raw_console()?;
    fs::write(
        output.join(format!("{index}.pid")),
        std::process::id().to_string(),
    )?;
    write!(io::stdout().lock(), "\x1b[3;5H\x1b]2;pike:ime:ready\x07")?;
    io::stdout().flush()?;
    let expected = if index == 0 {
        "你好é´@\x03!"
    } else {
        "界!"
    };
    let expected = &expected.as_bytes()[..expected.len() - 1];
    let mut bytes = vec![0; expected.len()];
    io::stdin().read_exact(&mut bytes)?;
    check(bytes == expected, "synthetic IME bytes differed")?;
    write!(io::stdout().lock(), "\x1b[4;7H\x1b]2;pike:ime:moved\x07")?;
    io::stdout().flush()?;
    let mut finish = [0];
    io::stdin().read_exact(&mut finish)?;
    check(finish == *b"!", "unexpected trailing IME input")
}

#[cfg(windows)]
fn raw_console() -> io::Result<()> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_INPUT_HANDLE, SetConsoleCP, SetConsoleMode,
    };
    // SAFETY: Borrow only this synthetic child's console; never close the handle.
    unsafe {
        let h = GetStdHandle(STD_INPUT_HANDLE);
        if h.is_null()
            || h == INVALID_HANDLE_VALUE
            || SetConsoleMode(h, 0) == 0
            || SetConsoleCP(65001) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
#[cfg(not(windows))]
fn raw_console() -> io::Result<()> {
    check(
        std::process::Command::new("stty")
            .args(["raw", "-echo"])
            .status()?
            .success(),
        "raw fixture setup failed",
    )
}
