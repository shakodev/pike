//! Synthetic VT -> authoritative cells -> actual renderer pixel/copy probe.
#[path = "compatibility/report.rs"]
mod report;

use iced::advanced::renderer::{Headless, Renderer as _, Style};
use iced::advanced::subscription::into_recipes;
use iced::advanced::widget::{Tree, operation};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::futures::{StreamExt, stream};
use iced::keyboard::{self, Modifiers};
use iced::{Color, Event, Executor, Font, Point, Rectangle, Size, Theme};
use iced_term::{AlacrittyEvent, BackendCommand, Command, Terminal, TerminalView};
use pike::executor::SharedExecutor;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::Duration;

const SCREEN: Size<f32> = Size::new(640.0, 400.0);

fn check(ok: bool, message: &'static str) -> io::Result<()> {
    if ok {
        Ok(())
    } else {
        Err(io::Error::other(message))
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
    let mut renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            Font::MONOSPACE,
            iced::Pixels(20.0),
            None,
        ))
        .ok_or("offscreen renderer unavailable")?;
    fs::write(directory.join("renderer.txt"), renderer.name())?;
    let program = std::env::current_exe()?
        .into_os_string()
        .into_string()
        .map_err(|_| "non-Unicode fixture path")?;
    let fonts = if cfg!(windows) {
        ["Consolas", "Courier New"]
    } else {
        ["Menlo", "Courier New"]
    };
    let mut events = report::Report::new(&directory)?;
    let mut cleanup = fs::File::create(directory.join("cleanup.csv"))?;
    writeln!(cleanup, "session,reason,pid")?;
    for (index, family) in fonts.iter().enumerate() {
        let output = directory.join(format!("font-{index}"));
        fs::create_dir(&output)?;
        fs::write(output.join("font.txt"), family)?;
        let present = iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .raw()
            .db()
            .faces()
            .any(|face| face.families.iter().any(|(name, _)| name == family));
        check(present, "requested fixture font is not installed")?;
        let mut terminal = Terminal::new(
            18001 + index as u64,
            iced_term::settings::Settings {
                font: iced_term::settings::FontSettings {
                    size: 20.0,
                    scale_factor: 1.4,
                    font_type: Font::with_name(family),
                },
                backend: iced_term::settings::BackendSettings {
                    program: program.clone(),
                    args: vec!["--fixture".into()],
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        let mut pid = 0;
        let result = run(&executor, &mut renderer, &mut terminal, &output, &mut pid);
        let handle = terminal.shutdown_handle();
        drop(terminal);
        let completion = executor.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), handle.wait()).await
        })?;
        check(
            completion.reason == iced_term::ExitReason::Requested,
            "Unicode child cleanup was not acknowledged",
        )?;
        writeln!(cleanup, "{},Requested,{pid}", 18001 + index)?;
        cleanup.flush()?;
        events.record("fixture", 18001 + index as u64, pid, "Requested")?;
        result?;
    }
    events.record("passed", 0, fonts.len(), "fonts")?;
    fs::write(directory.join("completed.txt"), "passed\n")?;
    Ok(())
}

fn fixture() -> io::Result<()> {
    no_echo()?;
    let mut output = io::stdout().lock();
    write!(
        output,
        "\x1b]2;pike:unicode:boot:{}\x07",
        std::process::id()
    )?;
    output.flush()?;
    for line in io::stdin().lock().lines() {
        let command = line?;
        match command.as_str() {
            "grid" => {
                write!(output, "\x1b[0m\x1b[2J\x1b[H\x1b[?25l")?;
                for (row, text) in [
                    (1, "A界Be\u{301}Z"),
                    (2, "A  BeZ"),
                    (3, "A  BéZ"),
                    (4, "界漢┌─┬─┐"),
                    (5, "A\x1b[44m界\x1b[0mB"),
                    (6, "X\u{338}Y"),
                    (7, "XY"),
                    (8, "👨\u{200d}👩"),
                ] {
                    write!(output, "\x1b[{row};1H{text}")?;
                }
                write!(
                    output,
                    "\x1b[9;1He{}\x1b[5;2H\x1b[?25h",
                    "\u{301}".repeat(40)
                )?;
            }
            "spacer" => write!(output, "\x1b[5;3H")?,
            "wrap" => write!(output, "\x1b[0m\x1b[2J\x1b[H\x1b[?25l1234567界Z")?,
            "narrow" => write!(output, "\x1b[0m\x1b[2J\x1b[H\x1b[?25l12345界Z")?,
            _ => return Err(io::Error::other("unknown synthetic command")),
        }
        write!(output, "\x1b]2;pike:unicode:{command}\x07")?;
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
    // SAFETY: Borrow the child's inherited console handle, do not close it.
    // `mode` is writable local storage. Only this synthetic child's echo is
    // disabled, preserving line input for its fixed command vocabulary.
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
        "could not disable synthetic command echo",
    )
}

fn wait(executor: &SharedExecutor, terminal: &mut Terminal, title: &str) -> io::Result<String> {
    executor.block_on(async {
        let mut events = into_recipes(terminal.subscription())
            .pop()
            .unwrap()
            .stream(stream::pending().boxed());
        tokio::time::timeout(Duration::from_secs(15), async {
            while let Some(iced_term::Event::BackendCall(_, command)) = events.next().await {
                let ready = match &command {
                    BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Title(value))
                        if value.starts_with(title) =>
                    {
                        Some(value.clone())
                    }
                    _ => None,
                };
                terminal.handle(Command::ProxyToBackend(command));
                terminal.refresh();
                if let Some(ready) = ready {
                    return Ok(ready);
                }
            }
            Err(io::Error::other("Unicode child ended before readiness"))
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
    wait(executor, terminal, &format!("pike:unicode:{command}"))?;
    Ok(())
}

fn size(terminal: &mut Terminal, columns: usize, lines: usize) -> Rectangle {
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
    Rectangle::new(Point::new(16.0, 24.0), dimensions)
}

fn render(
    terminal: &Terminal,
    renderer: &mut iced::Renderer,
    tree: &mut Tree,
    bounds: Rectangle,
    viewport: Rectangle,
    scale: f32,
) -> iced::window::Screenshot {
    renderer.reset(Rectangle::with_size(SCREEN));
    let mut view = TerminalView::show(terminal);
    tree.diff(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(
            tree,
            renderer,
            &layout::Limits::new(bounds.size(), bounds.size()),
        )
        .move_to(bounds.position());
    view.as_widget().draw(
        tree,
        renderer,
        &Theme::Dark,
        &Style::default(),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &viewport,
    );
    let dimensions = Size::new(
        (SCREEN.width * scale) as u32,
        (SCREEN.height * scale) as u32,
    );
    iced::window::Screenshot::new(
        Headless::screenshot(renderer, dimensions, scale, Color::WHITE),
        dimensions,
        scale,
    )
}

fn crop(
    shot: &iced::window::Screenshot,
    bounds: Rectangle,
    cell: Size<f32>,
    column: usize,
    row: usize,
    width: usize,
) -> io::Result<Vec<u8>> {
    let scale = shot.scale_factor;
    let region = Rectangle {
        x: ((bounds.x + column as f32 * cell.width) * scale).ceil() as u32 + 1,
        y: ((bounds.y + row as f32 * cell.height) * scale).ceil() as u32 + 1,
        width: (width as f32 * cell.width * scale).floor() as u32 - 2,
        height: (cell.height * scale).floor() as u32 - 2,
    };
    Ok(shot.crop(region).map_err(io::Error::other)?.rgba.to_vec())
}

fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.abs_diff(*b) <= 2)
}

fn sample(shot: &iced::window::Screenshot, point: Point) -> &[u8] {
    let x = (point.x * shot.scale_factor) as usize;
    let y = (point.y * shot.scale_factor) as usize;
    let offset = (y * shot.size.width as usize + x) * 4;
    &shot.rgba[offset..offset + 4]
}

fn pane(shot: &iced::window::Screenshot, bounds: Rectangle) -> io::Result<Vec<u8>> {
    let scale = shot.scale_factor;
    Ok(shot
        .crop(Rectangle {
            x: (bounds.x * scale) as u32 + 1,
            y: (bounds.y * scale) as u32 + 1,
            width: (bounds.width * scale) as u32 - 2,
            height: (bounds.height * scale) as u32 - 2,
        })
        .map_err(io::Error::other)?
        .rgba
        .to_vec())
}

fn copy(
    terminal: &Terminal,
    renderer: &iced::Renderer,
    tree: &mut Tree,
    bounds: Rectangle,
) -> io::Result<String> {
    #[derive(Default)]
    struct Clipboard(String);
    impl clipboard::Clipboard for Clipboard {
        fn read(&self, _: clipboard::Kind) -> Option<String> {
            None
        }
        fn write(&mut self, _: clipboard::Kind, text: String) {
            self.0 = text;
        }
    }
    let mut clipboard = Clipboard::default();
    let mut view = TerminalView::show(terminal);
    let node = view
        .as_widget_mut()
        .layout(
            tree,
            renderer,
            &layout::Limits::new(bounds.size(), bounds.size()),
        )
        .move_to(bounds.position());
    view.as_widget_mut().operate(
        tree,
        Layout::new(&node),
        renderer,
        &mut operation::focusable::focus::<()>(terminal.widget_id().clone()),
    );
    let modifiers = if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL | Modifiers::SHIFT
    };
    let mut messages = Vec::new();
    for event in [
        keyboard::Event::ModifiersChanged(modifiers),
        keyboard::Event::KeyPressed {
            key: keyboard::Key::Character("c".into()),
            modified_key: keyboard::Key::Character("c".into()),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        },
    ] {
        view.as_widget_mut().update(
            tree,
            &Event::Keyboard(event),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut clipboard,
            &mut Shell::new(&mut messages),
            &bounds,
        );
    }
    check(
        messages.iter().all(|event| {
            matches!(
                event,
                iced_term::Event::BackendCall(
                    _,
                    BackendCommand::Resize(..)
                        | BackendCommand::ProcessLink(
                            iced_term::LinkAction::Hover | iced_term::LinkAction::Clear,
                            _
                        )
                )
            )
        }),
        "copy emitted terminal input or changed layout",
    )?;
    Ok(clipboard.0)
}

fn run(
    executor: &SharedExecutor,
    renderer: &mut iced::Renderer,
    terminal: &mut Terminal,
    output: &Path,
    pid: &mut u32,
) -> io::Result<()> {
    let ready = wait(executor, terminal, "pike:unicode:boot:")?;
    *pid = ready
        .rsplit(':')
        .next()
        .unwrap()
        .parse()
        .map_err(io::Error::other)?;
    let bounds = size(terminal, 16, 10);
    send(executor, terminal, "grid")?;
    let content = terminal.renderable_content().unwrap();
    let cell = Size::new(
        content.terminal_size.cell_width,
        content.terminal_size.cell_height,
    );
    let first: Vec<_> = content
        .viewport
        .display_iter()
        .take(6)
        .map(|indexed| {
            (
                indexed.point.column.0,
                indexed.c,
                indexed.zerowidth().unwrap_or_default().to_vec(),
            )
        })
        .collect();
    check(
        first[0].1 == 'A'
            && first[1].1 == '界'
            && first[3].1 == 'B'
            && first[4].2 == ['\u{301}']
            && first[5].1 == 'Z',
        "native Unicode cell data differs from VT fixture",
    )?;
    let mut tree = Tree::empty();
    let mut observations = fs::File::create(output.join("checks.csv"))?;
    writeln!(
        observations,
        "scale,ascii_boundaries,combining,cjk_fallback,cursor,paint_work"
    )?;
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let before = terminal.work_stats();
        let snapshots = terminal.snapshot_stats().snapshots;
        let shot = render(
            terminal,
            renderer,
            &mut tree,
            bounds,
            Rectangle::with_size(SCREEN),
            scale,
        );
        report::screenshot(&output.join(format!("grid-{scale}.ppm")), shot.clone())?;
        for column in [0, 3, 5] {
            check(
                same(
                    &crop(&shot, bounds, cell, column, 0, 1)?,
                    &crop(&shot, bounds, cell, column, 1, 1)?,
                ),
                "CJK/combining shifted adjacent ASCII pixels",
            )?;
            if !same(
                &crop(&shot, bounds, cell, column, 2, 1)?,
                &crop(&shot, bounds, cell, column, 1, 1)?,
            ) {
                return Err(io::Error::other(format!(
                    "reference ASCII mismatch column {column}, scale {scale}, cell {cell:?}"
                )));
            }
        }
        let accent = crop(&shot, bounds, cell, 4, 0, 1)?;
        check(
            !same(&accent, &crop(&shot, bounds, cell, 4, 1, 1)?),
            "combining accent was not rendered",
        )?;
        let plain = crop(&shot, bounds, cell, 4, 1, 1)?;
        let lower = (accent.len() / 2 / 4) * 4;
        check(
            same(&accent[lower..], &plain[lower..]),
            "combining text lost or shifted its base glyph",
        )?;
        check(
            same(&accent, &crop(&shot, bounds, cell, 4, 2, 1)?),
            "decomposed and precomposed accent pixels differ",
        )?;
        check(
            !same(
                &crop(&shot, bounds, cell, 0, 3, 2)?,
                &crop(&shot, bounds, cell, 2, 3, 2)?,
            ),
            "CJK fallback glyphs are missing or identical tofu",
        )?;
        check(
            same(
                &crop(&shot, bounds, cell, 1, 5, 1)?,
                &crop(&shot, bounds, cell, 1, 6, 1)?,
            ),
            "combining glyph escaped its cell clip",
        )?;
        check(
            same(
                &crop(&shot, bounds, cell, 3, 4, 1)?,
                &crop(&shot, bounds, cell, 3, 1, 1)?,
            ),
            "wide cursor/glyph escaped into following ASCII",
        )?;
        let cursor_first = sample(
            &shot,
            Point::new(bounds.x + cell.width * 1.5, bounds.y + cell.height * 4.95),
        );
        let cursor_second = sample(
            &shot,
            Point::new(bounds.x + cell.width * 2.5, bounds.y + cell.height * 4.95),
        );
        let background = sample(
            &shot,
            Point::new(bounds.x + cell.width * 1.5, bounds.y + cell.height * 1.95),
        );
        check(
            cursor_first == cursor_second && cursor_second != background,
            "wide cursor did not cover both terminal columns",
        )?;
        let moved_bounds = Rectangle::new(Point::new(80.0, 52.0), bounds.size());
        let moved = render(
            terminal,
            renderer,
            &mut tree,
            moved_bounds,
            Rectangle::with_size(SCREEN),
            scale,
        );
        check(
            same(&pane(&shot, bounds)?, &pane(&moved, moved_bounds)?),
            "cached Unicode shifted while moving pane geometry",
        )?;
        let viewport = Rectangle::new(
            Point::new(bounds.x + cell.width * 1.5, bounds.y + cell.height * 0.5),
            Size::new(cell.width * 4.0, cell.height * 4.0),
        );
        let clipped = render(terminal, renderer, &mut tree, bounds, viewport, scale);
        for (index, pixel) in clipped.rgba.chunks_exact(4).enumerate() {
            let point = Point::new(
                (index % clipped.size.width as usize) as f32 / scale,
                (index / clipped.size.width as usize) as f32 / scale,
            );
            if bounds.shrink(1.0 / scale).contains(point)
                && !viewport.expand(1.0 / scale).contains(point)
            {
                check(
                    pixel == [255, 255, 255, 255] || pixel == background,
                    "Unicode ink escaped ancestor viewport",
                )?;
            }
        }
        if scale == 1.5 {
            report::screenshot(&output.join("translated.ppm"), moved)?;
            report::screenshot(&output.join("clipped.ppm"), clipped)?;
        }
        check(
            terminal.work_stats() == before && terminal.snapshot_stats().snapshots == snapshots,
            "painting remeasured fonts, resized or captured a snapshot",
        )?;
        writeln!(
            observations,
            "{scale},passed,passed,passed,passed,unchanged"
        )?;
        report::screenshot(&output.join(format!("grid-{scale}.ppm")), shot)?;
    }
    terminal.handle(Command::ProxyToBackend(BackendCommand::SelectStart(
        iced_term::SelectionType::Simple,
        (0.0, cell.height * 0.5),
    )));
    terminal.handle(Command::ProxyToBackend(BackendCommand::SelectUpdate((
        cell.width * 5.9,
        cell.height * 0.5,
    ))));
    check(
        copy(terminal, renderer, &mut tree, bounds)? == "A界Be\u{301}Z",
        "widget copy differs from displayed wide/combining cells",
    )?;
    report::screenshot(
        &output.join("selection.ppm"),
        render(
            terminal,
            renderer,
            &mut tree,
            bounds,
            Rectangle::with_size(SCREEN),
            1.5,
        ),
    )?;
    send(executor, terminal, "grid")?;
    let lead = render(
        terminal,
        renderer,
        &mut tree,
        bounds,
        Rectangle::with_size(SCREEN),
        1.0,
    );
    send(executor, terminal, "spacer")?;
    let spacer = render(
        terminal,
        renderer,
        &mut tree,
        bounds,
        Rectangle::with_size(SCREEN),
        1.0,
    );
    check(
        same(&lead.rgba, &spacer.rgba),
        "cursor on wide continuation differs from its lead cell",
    )?;
    for (columns, command, prefix, wide_column) in [(8, "wrap", 7, 0), (6, "narrow", 5, 0)] {
        let bounds = size(terminal, columns, 10);
        send(executor, terminal, command)?;
        let content = terminal.renderable_content().unwrap();
        let cells: Vec<_> = content.viewport.display_iter().collect();
        check(
            cells[prefix].c == ' ' && cells[prefix].flags.bits() & 0x0400 != 0,
            "missing leading wrap placeholder",
        )?;
        check(
            cells[columns + wide_column].c == '界' && cells[columns + 2].c == 'Z',
            "wide wrap moved following ASCII",
        )?;
        terminal.handle(Command::ProxyToBackend(BackendCommand::SelectStart(
            iced_term::SelectionType::Simple,
            (0.0, cell.height * 0.5),
        )));
        terminal.handle(Command::ProxyToBackend(BackendCommand::SelectUpdate((
            cell.width * 2.9,
            cell.height * 1.5,
        ))));
        check(
            copy(terminal, renderer, &mut tree, bounds)?
                == if columns == 8 {
                    "1234567界Z"
                } else {
                    "12345界Z"
                },
            "wrapped widget copy includes continuation padding",
        )?;
        report::screenshot(
            &output.join(format!("wrap-{columns}.ppm")),
            render(
                terminal,
                renderer,
                &mut tree,
                bounds,
                Rectangle::with_size(SCREEN),
                1.5,
            ),
        )?;
    }
    fs::write(
        output.join("passed.txt"),
        "model, pixels, fallback, clipping, cursor and widget copy passed\n",
    )
}
