//! Opt-in public widget/VT/byte fixture. Uses synthetic content only.
#[cfg(windows)]
#[path = "support/cursor_input.rs"]
#[allow(dead_code)]
mod console_input;
#[path = "compatibility/report.rs"]
#[allow(dead_code)]
mod report;
use iced::advanced::renderer::Headless;
use iced::advanced::subscription::into_recipes;
use iced::advanced::widget::{Tree, operation};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::futures::{StreamExt, stream};
use iced::{Event, Executor, Font, Point, Rectangle, Size};
use iced_term::{
    AlacrittyEvent, BackendCommand as B, Command, HistoryScroll, Terminal, TerminalView,
};
use pike::executor::SharedExecutor;
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
    time::Duration,
};
fn check(ok: bool, message: &'static str) -> io::Result<()> {
    if ok {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--fixture") {
        fixture()?;
        return Ok(());
    }
    let out = std::path::absolute(args.get(1).ok_or("fresh output directory required")?)?;
    check(!out.exists(), "fresh output directory required")?;
    fs::create_dir_all(&out)?;
    let ex = SharedExecutor::new()?;
    let renderer = ex
        .block_on(<iced::Renderer as Headless>::new(
            Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or("no event-fixture renderer")?;
    fs::write(out.join("renderer.txt"), renderer.name())?;
    let mut term = Terminal::new(
        170001,
        iced_term::settings::Settings {
            backend: iced_term::settings::BackendSettings {
                program: std::env::current_exe()?.to_string_lossy().into_owned(),
                args: vec!["--fixture".into()],
                ..Default::default()
            },
            ..Default::default()
        },
    )?;
    let mut pid = 0;
    let result = run(&ex, &renderer, &mut term, &out, &mut pid);
    let shutdown = term.shutdown_handle();
    drop(term);
    let done = ex
        .block_on(async { tokio::time::timeout(Duration::from_secs(10), shutdown.wait()).await })?;
    check(
        done.reason == iced_term::ExitReason::Requested,
        "raw fixture cleanup not acknowledged",
    )?;
    fs::write(
        out.join("cleanup.csv"),
        format!("session,reason,pid\n170001,Requested,{pid}\n"),
    )?;
    result?;
    if let Some(vim) = args.get(2) {
        vim_probe(&ex, &renderer, &out, Path::new(vim))?;
    } else {
        fs::write(
            out.join("vim-unavailable.txt"),
            "No explicit local Vim executable supplied.\n",
        )?;
    }
    fs::write(out.join("completed.txt"), "passed\n")?;
    Ok(())
}
fn fixture() -> io::Result<()> {
    #[cfg(windows)]
    console_input::configure(None)?;
    #[cfg(not(windows))]
    check(
        std::process::Command::new("/bin/stty")
            .args(["raw", "-echo"])
            .status()?
            .success(),
        "raw fixture input unavailable",
    )?;
    let mut input = io::stdin().lock();
    let mut out = io::stdout().lock();
    write!(out, "\x1b]2;pike:mouse:boot:{}\x07", std::process::id())?;
    out.flush()?;
    let mut command = Vec::new();
    let mut control = false;
    let mut expected: Option<(String, Vec<u8>, Vec<u8>)> = None;
    let mut stray = false;
    loop {
        let mut byte = [0];
        if input.read(&mut byte)? == 0 {
            return Ok(());
        }
        let value = byte[0];
        if let Some((id, want, got)) = &mut expected {
            got.push(value);
            if got.len() == want.len() {
                write!(
                    out,
                    "\x1b]2;pike:mouse:result:{id}:{}\x07",
                    if got == want { "ok" } else { "bad" }
                )?;
                out.flush()?;
                expected = None;
            }
            continue;
        }
        if control {
            if value != b'\n' {
                check(
                    value.is_ascii() && command.len() < 512,
                    "invalid bounded control command",
                )?;
                command.push(value);
                continue;
            }
            control = false;
            let line = String::from_utf8(std::mem::take(&mut command)).map_err(io::Error::other)?;
            let mut parts = line.splitn(3, ' ');
            let id = parts.next().ok_or_else(|| io::Error::other("missing id"))?;
            let verb = parts
                .next()
                .ok_or_else(|| io::Error::other("missing verb"))?;
            let data = parts.next().unwrap_or("");
            match verb {
                "mode" => {
                    write!(
                        out,
                        "\x1b[?1049l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1007h\x1b[?1l\x1b[0m\x1b[2J\x1b[3J\x1b[H"
                    )?;
                    let mode = match data {
                        "sgr" => "\x1b[?1002h\x1b[?1006h",
                        "motion" => "\x1b[?1003h\x1b[?1006h",
                        "legacy" => "\x1b[?1000h",
                        "utf8" => "\x1b[?1000h\x1b[?1005h",
                        "alt" => "\x1b[?1049h\x1b[?1h",
                        "alt-normal" => "\x1b[?1049h",
                        "history" => "",
                        _ => return Err(io::Error::other("unknown mode")),
                    };
                    write!(out, "{mode}alpha\r\nbeta")?;
                    if data == "history" {
                        for i in 0..20 {
                            write!(out, "\r\nrow {i:02}")?;
                        }
                    }
                }
                "expect" => {
                    check(
                        !stray && !data.is_empty() && data.len() % 2 == 0 && data.len() <= 256,
                        "unexpected fixture bytes or invalid oracle",
                    )?;
                    let want = (0..data.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&data[i..i + 2], 16).map_err(io::Error::other))
                        .collect::<io::Result<Vec<_>>>()?;
                    expected = Some((id.into(), want, Vec::new()));
                }
                "emit" => write!(out, "\r\ncontinued")?,
                "quiet" => {}
                "check" => {
                    write!(
                        out,
                        "\x1b]2;pike:mouse:result:{id}:{}\x07",
                        if stray { "bad" } else { "ok" }
                    )?;
                    out.flush()?;
                    continue;
                }
                _ => return Err(io::Error::other("unknown control command")),
            }
            write!(out, "\x1b]2;pike:mouse:ready:{id}\x07")?;
            out.flush()?;
        } else if value == b'@' {
            control = true;
        } else {
            stray = true;
        }
    }
}
fn wait(ex: &SharedExecutor, term: &mut Terminal, prefix: &str) -> io::Result<String> {
    ex.block_on(async {
        let mut events = into_recipes(term.subscription())
            .pop()
            .unwrap()
            .stream(stream::pending().boxed());
        tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(iced_term::Event::BackendCall(id, c)) = events.next().await {
                check(id == term.id, "wrong fixture session")?;
                let title = match &c {
                    B::ProcessAlacrittyEvent(AlacrittyEvent::Title(t)) if t.starts_with(prefix) => {
                        Some(t.clone())
                    }
                    _ => None,
                };
                term.handle(Command::ProxyToBackend(c));
                term.refresh();
                if let Some(t) = title {
                    return Ok(t);
                }
            }
            Err(io::Error::other("fixture ended before response"))
        })
        .await
        .map_err(io::Error::other)?
    })
}
fn control(
    ex: &SharedExecutor,
    term: &mut Terminal,
    id: &mut usize,
    command: &str,
) -> io::Result<usize> {
    *id += 1;
    let n = *id;
    // Fixture commands are protocol traffic, preserving a scrolled viewport;
    // explicit Write below separately tests real typing returning to bottom.
    term.handle(Command::ProxyToBackend(B::ProcessAlacrittyEvent(
        AlacrittyEvent::PtyWrite(format!("@{n} {command}\n")),
    )));
    if !command.starts_with("check") {
        wait(ex, term, &format!("pike:mouse:ready:{n}"))?;
    }
    Ok(n)
}
fn expect(
    ex: &SharedExecutor,
    term: &mut Terminal,
    id: &mut usize,
    bytes: &[u8],
) -> io::Result<usize> {
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    control(ex, term, id, &format!("expect {hex}"))
}
fn result(ex: &SharedExecutor, term: &mut Terminal, id: usize) -> io::Result<()> {
    let reply = wait(ex, term, &format!("pike:mouse:result:{id}:"))?;
    check(
        reply.ends_with(":ok"),
        "raw-byte fixture disagreed with exact oracle",
    )
}
fn resize(term: &mut Terminal, cols: usize, rows: usize) -> Rectangle {
    let c = term.renderable_content().unwrap().terminal_size;
    let size = Size::new(
        c.cell_width * cols as f32 + 0.01,
        c.cell_height * rows as f32 + 0.01,
    );
    term.handle(Command::ProxyToBackend(B::Resize(Some(size), None)));
    Rectangle::new(Point::new(13.25, 31.5), size)
}
fn point(term: &Terminal, b: Rectangle, col: f32, row: f32) -> Point {
    let c = term.renderable_content().unwrap().terminal_size;
    Point::new(b.x + c.cell_width * col, b.y + c.cell_height * row)
}
struct Scene {
    tree: Tree,
    pending: Vec<iced_term::Event>,
}
impl Scene {
    fn new() -> Self {
        Self {
            tree: Tree::empty(),
            pending: Vec::new(),
        }
    }
    fn focus(&mut self, term: &Terminal, r: &iced::Renderer, b: Rectangle, focused: bool) {
        let mut v = TerminalView::show(term);
        self.tree.diff(v.as_widget());
        let n = v
            .as_widget_mut()
            .layout(&mut self.tree, r, &layout::Limits::new(b.size(), b.size()))
            .move_to(b.position());
        v.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&n),
            r,
            &mut operation::focusable::focus::<()>(if focused {
                term.widget_id().clone()
            } else {
                iced::widget::Id::unique()
            }),
        );
    }
    fn queue(
        &mut self,
        term: &Terminal,
        r: &iced::Renderer,
        b: Rectangle,
        event: Event,
        cursor: mouse::Cursor,
    ) -> bool {
        let mut v = TerminalView::show(term);
        self.tree.diff(v.as_widget());
        let n = v
            .as_widget_mut()
            .layout(&mut self.tree, r, &layout::Limits::new(b.size(), b.size()))
            .move_to(b.position());
        let mut shell = Shell::new(&mut self.pending);
        v.as_widget_mut().update(
            &mut self.tree,
            &event,
            Layout::new(&n),
            cursor,
            r,
            &mut clipboard::Null,
            &mut shell,
            &b,
        );
        shell.is_event_captured()
    }
    fn mouse(
        &mut self,
        term: &Terminal,
        r: &iced::Renderer,
        b: Rectangle,
        e: mouse::Event,
        p: Point,
    ) {
        self.queue(term, r, b, Event::Mouse(e), mouse::Cursor::Available(p));
    }
    fn apply(&mut self, term: &mut Terminal) -> io::Result<()> {
        for iced_term::Event::BackendCall(id, c) in self.pending.drain(..) {
            check(id == term.id, "widget routed wrong session")?;
            check(
                !matches!(
                    term.handle(Command::ProxyToBackend(c)),
                    iced_term::actions::Action::InputRejected(_)
                ),
                "synthetic input rejected",
            )?;
        }
        Ok(())
    }
}
fn run(
    ex: &SharedExecutor,
    r: &iced::Renderer,
    term: &mut Terminal,
    out: &Path,
    pid: &mut u32,
) -> io::Result<()> {
    *pid = wait(ex, term, "pike:mouse:boot:")?
        .rsplit(':')
        .next()
        .unwrap()
        .parse()
        .map_err(io::Error::other)?;
    let window = iced_term::WindowFocus::default();
    window.set_focused(true);
    term.set_window_focus(window);
    term.set_focused(true);
    let mut serial = 0;
    let mut scene = Scene::new();
    let mut b = resize(term, 12, 5);
    scene.focus(term, r, b, true);
    scene.queue(
        term,
        r,
        b,
        Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        )),
        mouse::Cursor::Unavailable,
    );
    scene.apply(term)?;
    let mut log = fs::File::create(out.join("checks.csv"))?;
    writeln!(log, "case,expected_bytes,result")?;
    for (button, base) in [
        (mouse::Button::Left, 0),
        (mouse::Button::Middle, 1),
        (mouse::Button::Right, 2),
    ] {
        control(ex, term, &mut serial, "mode sgr")?;
        let expected = format!(
            "\x1b[<{base};4;3M\x1b[<{};5;4M\x1b[<{base};12;5m",
            base + 32
        );
        let tag = expect(ex, term, &mut serial, expected.as_bytes())?;
        let p = point(term, b, 3.2, 2.2);
        scene.mouse(term, r, b, mouse::Event::ButtonPressed(button), p);
        let q = point(term, b, 4.2, 3.2);
        scene.mouse(term, r, b, mouse::Event::CursorMoved { position: q }, q);
        let z = point(term, b, 100.0, 100.0);
        scene.mouse(term, r, b, mouse::Event::ButtonReleased(button), z);
        scene.apply(term)?;
        result(ex, term, tag)?;
        writeln!(log, "sgr-button-{base},{},passed", expected.len())?;
    }
    for (mode, expected, col, row) in [
        ("legacy", &b"\x1b[M $#\x1b[M#$#"[..], 3.2, 2.2),
        (
            "utf8",
            &b"\x1b[M \xc2\x80\xc2\x80\x1b[M#\xc2\x80\xc2\x80"[..],
            95.2,
            95.2,
        ),
    ] {
        b = resize(
            term,
            if mode == "utf8" { 120 } else { 12 },
            if mode == "utf8" { 120 } else { 5 },
        );
        control(ex, term, &mut serial, &format!("mode {mode}"))?;
        let tag = expect(ex, term, &mut serial, expected)?;
        let p = point(term, b, col, row);
        scene.mouse(
            term,
            r,
            b,
            mouse::Event::ButtonPressed(mouse::Button::Left),
            p,
        );
        scene.mouse(
            term,
            r,
            b,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            p,
        );
        scene.apply(term)?;
        result(ex, term, tag)?;
        writeln!(log, "{mode},{},passed", expected.len())?;
    }
    b = resize(term, 12, 5);
    control(ex, term, &mut serial, "mode sgr")?;
    let expected = b"\x1b[<0;4;3MK\x1b[<0;4;3m";
    let tag = expect(ex, term, &mut serial, expected)?;
    let p = point(term, b, 3.2, 2.2);
    scene.focus(term, r, b, true);
    scene.mouse(
        term,
        r,
        b,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        p,
    );
    scene.queue(
        term,
        r,
        b,
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Character("k".into()),
            modified_key: iced::keyboard::Key::Character("K".into()),
            physical_key: iced::keyboard::key::Physical::Unidentified(
                iced::keyboard::key::NativeCode::Unidentified,
            ),
            location: iced::keyboard::Location::Standard,
            modifiers: iced::keyboard::Modifiers::empty(),
            text: Some("K".into()),
            repeat: false,
        }),
        mouse::Cursor::Unavailable,
    );
    scene.mouse(
        term,
        r,
        b,
        mouse::Event::ButtonReleased(mouse::Button::Left),
        p,
    );
    scene.apply(term)?;
    result(ex, term, tag)?;
    writeln!(log, "batched-mouse-key-order,{},passed", expected.len())?;
    control(ex, term, &mut serial, "mode motion")?;
    let expected = b"\x1b[<35;4;3M\x1b[<64;4;3M\x1b[<65;4;3M";
    let tag = expect(ex, term, &mut serial, expected)?;
    scene.focus(term, r, b, false);
    term.set_focused(false);
    scene.mouse(term, r, b, mouse::Event::CursorMoved { position: p }, p);
    for _ in 0..4 {
        check(
            scene.queue(
                term,
                r,
                b,
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: 0.0, y: 0.25 },
                }),
                mouse::Cursor::Available(p),
            ),
            "fractional wheel not captured",
        )?;
    }
    let cell_height = term.renderable_content().unwrap().terminal_size.cell_height;
    for _ in 0..4 {
        check(
            scene.queue(
                term,
                r,
                b,
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Pixels {
                        x: 0.0,
                        y: -cell_height / 4.0,
                    },
                }),
                mouse::Cursor::Available(p),
            ),
            "fractional pixel wheel not captured",
        )?;
    }
    scene.apply(term)?;
    result(ex, term, tag)?;
    check(
        !term.renderable_content().unwrap().focused,
        "wheel changed keyboard focus",
    )?;
    writeln!(
        log,
        "unfocused-hover-fractional-wheel,{},passed",
        expected.len()
    )?;
    control(ex, term, &mut serial, "mode sgr")?;
    control(ex, term, &mut serial, "quiet")?;
    let before = term.input_status().queued_bytes;
    scene.queue(
        term,
        r,
        b,
        Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
            iced::keyboard::Modifiers::SHIFT,
        )),
        mouse::Cursor::Unavailable,
    );
    let a = point(term, b, 0.1, 0.5);
    let z = point(term, b, 3.9, 1.5);
    scene.mouse(
        term,
        r,
        b,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        a,
    );
    scene.mouse(term, r, b, mouse::Event::CursorMoved { position: z }, z);
    scene.queue(
        term,
        r,
        b,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Unavailable,
    );
    scene.apply(term)?;
    check(
        term.input_status().queued_bytes == before,
        "Shift selection sent TUI bytes",
    )?;
    check(
        term.copy_selection(|s| s).as_deref() == Some("alpha\nbeta"),
        "local selection/copy disagreed",
    )?;
    let tag = control(ex, term, &mut serial, "check")?;
    result(ex, term, tag)?;
    writeln!(log, "shift-local-copy,0,passed")?;
    scene.queue(
        term,
        r,
        b,
        Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
            iced::keyboard::Modifiers::empty(),
        )),
        mouse::Cursor::Unavailable,
    );
    scene.apply(term)?;
    for event in [
        Event::Window(iced::window::Event::Unfocused),
        Event::Mouse(mouse::Event::CursorLeft),
    ] {
        scene.queue(
            term,
            r,
            b,
            Event::Window(iced::window::Event::Focused),
            mouse::Cursor::Unavailable,
        );
        scene.apply(term)?;
        control(ex, term, &mut serial, "mode sgr")?;
        let expected = b"\x1b[<0;4;3M\x1b[<0;4;3m";
        let tag = expect(ex, term, &mut serial, expected)?;
        scene.mouse(
            term,
            r,
            b,
            mouse::Event::ButtonPressed(mouse::Button::Left),
            p,
        );
        scene.queue(term, r, b, event, mouse::Cursor::Unavailable);
        scene.apply(term)?;
        result(ex, term, tag)?;
        writeln!(log, "blur-or-windowleave,{},passed", expected.len())?;
    }
    for (mode, expected) in [
        ("alt", &b"\x1bOA\x1bOA\x1bOB"[..]),
        ("alt-normal", &b"\x1b[A\x1b[A\x1b[B"[..]),
    ] {
        control(ex, term, &mut serial, &format!("mode {mode}"))?;
        let tag = expect(ex, term, &mut serial, expected)?;
        for y in [2.0, -1.0] {
            scene.mouse(
                term,
                r,
                b,
                mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: 0.0, y },
                },
                p,
            );
        }
        scene.apply(term)?;
        result(ex, term, tag)?;
        writeln!(log, "{mode},{},passed", expected.len())?;
    }
    control(ex, term, &mut serial, "mode history")?;
    term.handle(Command::ProxyToBackend(B::ScrollHistory(
        HistoryScroll::Top,
    )));
    let before: Vec<_> = term
        .renderable_content()
        .unwrap()
        .viewport
        .display_iter()
        .take(12)
        .map(|x| x.c)
        .collect();
    let offset = term.renderable_content().unwrap().viewport.display_offset();
    control(ex, term, &mut serial, "emit")?;
    let after: Vec<_> = term
        .renderable_content()
        .unwrap()
        .viewport
        .display_iter()
        .take(12)
        .map(|x| x.c)
        .collect();
    check(
        before == after
            && term.renderable_content().unwrap().viewport.display_offset() == offset + 1,
        "output moved scrolled viewport",
    )?;
    term.handle(Command::ProxyToBackend(B::ScrollHistory(
        HistoryScroll::PageDown,
    )));
    term.handle(Command::ProxyToBackend(B::ScrollHistory(
        HistoryScroll::PageUp,
    )));
    term.handle(Command::ProxyToBackend(B::ScrollHistory(
        HistoryScroll::Top,
    )));
    check(
        term.renderable_content().unwrap().viewport.display_offset()
            == term.snapshot_stats().history_lines,
        "history top bound incorrect",
    )?;
    let tag = expect(ex, term, &mut serial, b"K")?;
    term.handle(Command::ProxyToBackend(B::Write(vec![b'K'])));
    result(ex, term, tag)?;
    check(
        term.renderable_content().unwrap().viewport.display_offset() == 0,
        "typing did not return bottom",
    )?;
    writeln!(log, "history-output-typing,1,passed")?;
    let tag = control(ex, term, &mut serial, "check")?;
    result(ex, term, tag)?;
    Ok(())
}
fn vim_probe(
    ex: &SharedExecutor,
    r: &iced::Renderer,
    out: &Path,
    program: &Path,
) -> io::Result<()> {
    let script = out.join("vimrc");
    let result_file = out.join("vim-result.txt");
    let pid_file = out.join("vim.pid");
    fs::write(
        &script,
        format!(
            "set nocompatible nomodeline mouse=a ttymouse=sgr\nset noswapfile nobackup nowritebackup\ncall setline(1, ['alpha', 'beta', 'gamma'])\ncall writefile([string(getpid())], '{}')\nset title titlestring=pike-mouse-vim-ready\n",
            pid_file.to_string_lossy().replace('\'', "''")
        ),
    )?;
    let mut term = Terminal::new(
        170002,
        iced_term::settings::Settings {
            backend: iced_term::settings::BackendSettings {
                program: program.to_string_lossy().into_owned(),
                args: vec![
                    "-N".into(),
                    "-n".into(),
                    "-i".into(),
                    "NONE".into(),
                    "--noplugin".into(),
                    "-u".into(),
                    script.to_string_lossy().into_owned(),
                ],
                ..Default::default()
            },
            ..Default::default()
        },
    )?;
    let result = (|| -> io::Result<()> {
        let b = resize(&mut term, 40, 10);
        wait(ex, &mut term, "pike-mouse-vim-ready")?;
        term.refresh();
        check(
            term.renderable_content()
                .unwrap()
                .terminal_mode
                .intersects(iced_term::TermMode::MOUSE_DRAG | iced_term::TermMode::MOUSE_MOTION),
            "Vim did not enable mouse tracking",
        )?;
        let window = iced_term::WindowFocus::default();
        window.set_focused(true);
        term.set_window_focus(window);
        let mut scene = Scene::new();
        scene.focus(&term, r, b, true);
        let p = point(&term, b, 0.2, 1.5);
        scene.mouse(
            &term,
            r,
            b,
            mouse::Event::ButtonPressed(mouse::Button::Left),
            p,
        );
        scene.mouse(
            &term,
            r,
            b,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            p,
        );
        scene.apply(&mut term)?;
        term.handle(Command::ProxyToBackend(B::Write(
            format!(
                "A!\x1b:execute 'write! '.fnameescape('{}')\r:qa!\r",
                result_file.to_string_lossy().replace('\'', "''")
            )
            .into_bytes(),
        )));
        Ok(())
    })();
    let handle = term.shutdown_handle();
    if result.is_err() {
        term.request_close();
    }
    let natural =
        ex.block_on(async { tokio::time::timeout(Duration::from_secs(10), handle.wait()).await });
    if natural.is_err() {
        term.request_close();
    }
    drop(term);
    let timed_out = natural.is_err();
    let done = match natural {
        Ok(done) => done,
        Err(_) => ex
            .block_on(async { tokio::time::timeout(Duration::from_secs(10), handle.wait()).await })
            .map_err(io::Error::other)?,
    };
    let pid = fs::read_to_string(&pid_file).unwrap_or_default();
    fs::write(
        out.join("vim-cleanup.csv"),
        format!(
            "session,reason,pid\n170002,{:?},{}\n",
            done.reason,
            pid.trim()
        ),
    )?;
    result?;
    check(!timed_out, "Vim timed out before requested cleanup")?;
    check(
        done.reason == iced_term::ExitReason::ChildExit && done.exit_code == Some(0),
        "Vim did not exit cleanly",
    )?;
    check(
        fs::read_to_string(result_file)? == "alpha\nbeta!\ngamma\n",
        "Vim mouse click targeted wrong row",
    )?;
    fs::write(
        out.join("vim-check.txt"),
        "passed: actual local Vim mouse-selected row2 before append/write/natural exit\n",
    )?;
    Ok(())
}
