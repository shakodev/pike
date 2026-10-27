//! Opt-in widget -> bounded writer -> raw PTY child conformance probe.
#[path = "support/keyboard_cases.rs"]
mod cases;

use iced::advanced::renderer::Headless;
use iced::advanced::subscription::into_recipes;
use iced::advanced::widget::{Tree, operation};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::futures::{StreamExt, stream};
use iced::keyboard::{Event as KeyEvent, Modifiers};
use iced::{Event, Executor, Rectangle, Size};
use iced_term::bindings::{Binding, BindingAction, InputKind};
use iced_term::{AlacrittyEvent, BackendCommand, Command, TermMode, Terminal, TerminalView};
use pike::executor::SharedExecutor;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn check(ok: bool, message: &'static str) -> io::Result<()> {
    if ok {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}

fn main() -> io::Result<()> {
    let output = std::path::absolute(
        std::env::args()
            .nth(1)
            .ok_or_else(|| io::Error::other("expected fresh output directory"))?,
    )?;
    check(!output.exists(), "choose a fresh output directory")?;
    fs::create_dir_all(&output)?;
    let executor = SharedExecutor::new()?;
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or_else(|| io::Error::other("offscreen renderer unavailable"))?;
    let fixture = std::env::current_exe()?
        .with_file_name(format!("input_fixture{}", std::env::consts::EXE_SUFFIX));
    check(fixture.is_file(), "build the input_fixture example first")?;
    let mut report = fs::File::create(output.join("summary.csv"))?;
    writeln!(
        report,
        "application_cursor,newline,cases,bytes,hash,max_dispatch_us,fixture_pid,cleanup"
    )?;
    for (application, newline) in [(false, false), (true, false), (false, true), (true, true)] {
        let cases = cases::cases(application, newline);
        let mut expected: Vec<_> = cases
            .iter()
            .flat_map(|case| case.expected.iter().copied())
            .collect();
        expected.push(b'!'); // FIFO sentinel after all positive/negative cases.
        let (directory_name, fixture_mode) = match (application, newline) {
            (false, false) => ("normal", "keyboard-normal"),
            (true, false) => ("application", "keyboard-app"),
            (false, true) => ("newline", "keyboard-newline"),
            (true, true) => ("application-newline", "keyboard-app-newline"),
        };
        let directory = output.join(directory_name);
        let settings = iced_term::settings::BackendSettings {
            program: path_string(&fixture)?,
            args: vec![
                "echo".into(),
                expected.len().to_string(),
                path_string(&directory)?,
                fixture_mode.into(),
            ],
            ..Default::default()
        };
        let ((pid, maximum), completion) = with_terminal(
            &executor,
            14001 + u64::from(application) + 2 * u64::from(newline),
            settings,
            |terminal| {
                wait(&executor, terminal, |terminal, command| {
                    matches!(command, BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Title(title)) if title == "pike:keys-ready")
                        && terminal.renderable_content().is_some()
                })?;
                let mode = terminal.renderable_content().unwrap().terminal_mode;
                check(
                    mode.contains(TermMode::APP_CURSOR) == application,
                    "cursor mode did not reach the emulator",
                )?;
                check(
                    mode.contains(TermMode::LINE_FEED_NEW_LINE) == newline,
                    "newline mode did not reach the emulator",
                )?;
                check(
                    !mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL),
                    "unsupported keyboard protocol was enabled",
                )?;
                let mut scene = Scene::new();
                scene.focus(terminal, &renderer)?;
                terminal.handle(Command::AddBindings(vec![
                    binding(InputKind::Char("x".into()), BindingAction::Ignore),
                    binding(
                        InputKind::KeyCode(iced::keyboard::key::Named::F21),
                        BindingAction::Copy,
                    ),
                    binding(
                        InputKind::KeyCode(iced::keyboard::key::Named::F22),
                        BindingAction::Paste,
                    ),
                ]));
                let before = terminal.work_stats();
                let snapshots = terminal.snapshot_stats().snapshots;
                let mut maximum = Duration::ZERO;
                for case in &cases {
                    // Deliberately stale state must not override a key's modifiers.
                    let (events, _) = scene.dispatch(
                        terminal,
                        &renderer,
                        Event::Keyboard(KeyEvent::ModifiersChanged(Modifiers::SHIFT)),
                    );
                    check(
                        events.is_empty(),
                        "modifier notification emitted terminal bytes",
                    )?;
                    let start = Instant::now();
                    let (events, captured) =
                        scene.dispatch(terminal, &renderer, Event::Keyboard(case.event.clone()));
                    maximum = maximum.max(start.elapsed());
                    check(captured == case.captured, "key event capture mismatch")?;
                    if case.expected.is_empty() {
                        check(
                            events.is_empty(),
                            "consumed/released key fell through to bytes",
                        )?;
                    } else {
                        check(
                            matches!(events.as_slice(), [iced_term::Event::BackendCall(id, BackendCommand::Write(bytes))]
                        if *id == terminal.id && bytes == &case.expected),
                            "keyboard byte oracle mismatch",
                        )?;
                    }
                    route(terminal, events)?;
                }
                // The encoder borrows committed text until byte admission;
                // an oversized event must be rejected as a whole, not copied
                // into another terminal input allocation or partially sent.
                let oversized = "K".repeat(iced_term::INPUT_BYTE_LIMIT + 1);
                let (events, captured) = scene.dispatch(
                    terminal,
                    &renderer,
                    Event::Keyboard(cases::character("k", Modifiers::empty(), Some(&oversized))),
                );
                check(
                    captured
                        && matches!(events.as_slice(),
                    [iced_term::Event::BackendCall(_, BackendCommand::InputRejected(iced_term::InputError::Full { requested, .. }))]
                    if *requested == oversized.len()),
                    "oversized key text bypassed byte admission",
                )?;
                check(
                    terminal.work_stats() == before
                        && terminal.snapshot_stats().snapshots == snapshots,
                    "keyboard input performed font/resize/snapshot work",
                )?;
                send(terminal, b"!".to_vec())?;
                wait_file(&directory.join("done"))?;
                let expected_hash = hash(&expected);
                check(
                    fs::read_to_string(directory.join("result"))?
                        == format!("{},{expected_hash}\n", expected.len()),
                    "raw child received different or duplicated bytes",
                )?;
                let pid = fs::read_to_string(directory.join("ready"))?
                    .parse::<u32>()
                    .map_err(io::Error::other)?;
                Ok((pid, maximum))
            },
        )?;
        check(
            completion.reason == iced_term::ExitReason::Requested,
            "raw fixture cleanup was not acknowledged",
        )?;
        writeln!(
            report,
            "{application},{newline},{},{},{},{:.3},{pid},Requested",
            cases.len(),
            expected.len(),
            hash(&expected),
            maximum.as_secs_f64() * 1e6
        )?;
    }
    let editor = std::env::args().nth(2).map(PathBuf::from).or_else(|| {
        cfg!(unix)
            .then(|| PathBuf::from("/usr/bin/vim"))
            .filter(|path| path.is_file())
    });
    if let Some(editor) = editor {
        verify_editor(&executor, &renderer, &editor, &output)?;
        fs::write(
            output.join("editor.txt"),
            "passed: synthetic insert/move/save/exit\n",
        )?;
    } else {
        fs::write(
            output.join("editor.txt"),
            "not run: pass a Vim executable as the second argument\n",
        )?;
    }
    fs::write(output.join("renderer.txt"), renderer.name())?;
    fs::write(output.join("completed.txt"), "passed\n")
}

fn binding(target: InputKind, action: BindingAction) -> (Binding<InputKind>, BindingAction) {
    (
        Binding {
            target,
            modifiers: Modifiers::empty(),
            terminal_mode_include: TermMode::empty(),
            terminal_mode_exclude: TermMode::empty(),
        },
        action,
    )
}

fn path_string(path: &Path) -> io::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| io::Error::other("non-Unicode fixture path"))
}

fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn with_terminal<T>(
    executor: &SharedExecutor,
    id: u64,
    backend: iced_term::settings::BackendSettings,
    run: impl FnOnce(&mut Terminal) -> io::Result<T>,
) -> io::Result<(T, iced_term::BackendCompletion)> {
    let mut terminal = Terminal::new(
        id,
        iced_term::settings::Settings {
            backend,
            ..Default::default()
        },
    )?;
    let result = run(&mut terminal);
    let handle = terminal.shutdown_handle();
    drop(terminal);
    let completion = executor
        .block_on(async { tokio::time::timeout(Duration::from_secs(10), handle.wait()).await })
        .map_err(io::Error::other)?;
    result.map(|value| (value, completion))
}

fn wait(
    executor: &SharedExecutor,
    terminal: &mut Terminal,
    predicate: impl Fn(&Terminal, &BackendCommand) -> bool,
) -> io::Result<()> {
    executor.block_on(async {
        let mut events = into_recipes(terminal.subscription())
            .pop()
            .unwrap()
            .stream(stream::pending().boxed());
        tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(iced_term::Event::BackendCall(id, command)) = events.next().await {
                check(id == terminal.id, "event reached the wrong fixture")?;
                terminal.handle(Command::ProxyToBackend(command.clone()));
                terminal.refresh();
                if predicate(terminal, &command) {
                    return Ok(());
                }
            }
            Err(io::Error::other("fixture ended before readiness"))
        })
        .await
        .map_err(io::Error::other)?
    })
}

fn wait_file(path: &Path) -> io::Result<()> {
    let start = Instant::now();
    // Opt-in fixture observation only; no polling is added to the application.
    while !path.is_file() {
        check(
            start.elapsed() < Duration::from_secs(10),
            "raw fixture deadline",
        )?;
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

fn send(terminal: &mut Terminal, bytes: Vec<u8>) -> io::Result<()> {
    let action = terminal.handle(Command::ProxyToBackend(BackendCommand::Write(bytes)));
    check(
        !matches!(action, iced_term::actions::Action::InputRejected(_)),
        "synthetic input was rejected",
    )
}

fn route(terminal: &mut Terminal, events: Vec<iced_term::Event>) -> io::Result<()> {
    for iced_term::Event::BackendCall(id, command) in events {
        check(
            id == terminal.id,
            "keyboard command reached the wrong fixture",
        )?;
        let action = terminal.handle(Command::ProxyToBackend(command));
        check(
            !matches!(action, iced_term::actions::Action::InputRejected(_)),
            "synthetic input was rejected",
        )?;
    }
    Ok(())
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
    fn focus(&mut self, terminal: &mut Terminal, renderer: &iced::Renderer) -> io::Result<()> {
        let (events, _) = self.dispatch(
            terminal,
            renderer,
            Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
        );
        route(terminal, events)?;
        let mut view = TerminalView::show(terminal);
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            renderer,
            &layout::Limits::new(Size::new(640.0, 400.0), Size::new(640.0, 400.0)),
        );
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            renderer,
            &mut operation::focusable::focus::<()>(terminal.widget_id().clone()),
        );
        Ok(())
    }
    fn dispatch(
        &mut self,
        terminal: &Terminal,
        renderer: &iced::Renderer,
        event: Event,
    ) -> (Vec<iced_term::Event>, bool) {
        let mut view = TerminalView::show(terminal);
        self.tree.diff(view.as_widget());
        let size = Size::new(640.0, 400.0);
        let node =
            view.as_widget_mut()
                .layout(&mut self.tree, renderer, &layout::Limits::new(size, size));
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
            &Rectangle::with_size(size),
        );
        let captured = shell.is_event_captured();
        (events, captured)
    }
}

fn verify_editor(
    executor: &SharedExecutor,
    renderer: &iced::Renderer,
    editor: &Path,
    output: &Path,
) -> io::Result<()> {
    use iced::keyboard::key::Named;
    let file = output.join("synthetic-editor.txt");
    let mut settings = iced_term::settings::BackendSettings {
        program: path_string(editor)?,
        args: vec![
            "-u".into(),
            "NONE".into(),
            "-i".into(),
            "NONE".into(),
            "-n".into(),
            "-N".into(),
            "--noplugin".into(),
            "--cmd".into(),
            "set t_RV= t_u7= t_RB= t_RF=".into(),
            path_string(&file)?,
        ],
        ..Default::default()
    };
    settings.env.insert("TERM".into(), "xterm-256color".into());
    let (_, completion) = with_terminal(executor, 14003, settings, |terminal| {
        wait(executor, terminal, |terminal, _| {
            terminal
                .renderable_content()
                .is_some_and(|content| content.terminal_mode.contains(TermMode::ALT_SCREEN))
        })?;
        let mut scene = Scene::new();
        scene.focus(terminal, renderer)?;
        for key in ["i", "a", "b"] {
            let (events, captured) = scene.dispatch(
                terminal,
                renderer,
                Event::Keyboard(cases::character(key, Modifiers::empty(), Some(key))),
            );
            check(captured, "editor key was not handled")?;
            route(terminal, events)?;
        }
        wait(executor, terminal, |terminal, _| {
            terminal.renderable_content().is_some_and(|content| {
                content
                    .viewport
                    .display_iter()
                    .take(2)
                    .map(|cell| cell.c)
                    .eq("ab".chars())
            })
        })?;
        for event in [
            cases::named(Named::ArrowLeft, Modifiers::empty()),
            cases::character("x", Modifiers::empty(), Some("x")),
            cases::named(Named::Escape, Modifiers::empty()),
            cases::character(":", Modifiers::empty(), Some(":")),
            cases::character("w", Modifiers::empty(), Some("w")),
            cases::character("q", Modifiers::empty(), Some("q")),
            cases::named(Named::Enter, Modifiers::empty()),
        ] {
            let (events, captured) = scene.dispatch(terminal, renderer, Event::Keyboard(event));
            check(captured, "editor key was not handled")?;
            route(terminal, events)?;
        }
        executor
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(10), terminal.shutdown_handle().wait())
                    .await
            })
            .map_err(io::Error::other)?;
        check(
            fs::read(&file)? == b"axb\n",
            "full-screen editor produced unexpected synthetic text",
        )
    })?;
    check(
        completion.exit_code == Some(0),
        "full-screen editor did not exit successfully",
    )
}
