//! Public VT/widget/pixel evidence. Output and editor files are synthetic.
#[path = "support/cursor_input.rs"]
mod input;
#[path = "support/cursor_micro.rs"]
mod micro;
#[path = "support/cursor_micro_status.rs"]
mod micro_status;
#[path = "support/cursor_pixels.rs"]
mod pixels;
#[path = "compatibility/report.rs"]
mod report;
#[path = "support/cursor_transitions.rs"]
mod transitions;

use iced::advanced::renderer::Headless;
use iced::advanced::subscription::into_recipes;
use iced::advanced::widget::Tree;
use iced::futures::{FutureExt, StreamExt, stream};
use iced::{Event, Executor, Font, Point, Rectangle, Size};
use iced_term::{AlacrittyEvent, BackendCommand, Command, Terminal};
use pike::executor::SharedExecutor;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

const SCREEN: Size<f32> = Size::new(640.0, 320.0);
fn check(ok: bool, message: &'static str) -> io::Result<()> {
    if ok {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--fixture") {
        fixture(args.get(2).map(Path::new))?;
        return Ok(());
    }
    if args.get(1).is_some_and(|arg| arg == "--micro-fixture") {
        micro::fixture(&args[2..])?;
        return Ok(());
    }
    let output = std::path::absolute(args.get(1).ok_or("expected fresh output directory")?)?;
    check(!output.exists(), "choose a fresh output directory")?;
    fs::create_dir_all(&output)?;
    let mut events = report::Report::new(&output)?;
    let executor = SharedExecutor::new()?;
    let mut renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            Font::MONOSPACE,
            iced::Pixels(20.0),
            None,
        ))
        .ok_or("offscreen renderer unavailable")?;
    fs::write(output.join("renderer.txt"), renderer.name())?;
    let program = std::env::current_exe()?
        .into_os_string()
        .into_string()
        .map_err(|_| "non-Unicode fixture path")?;
    let mut terminal = Terminal::new(
        21001,
        iced_term::settings::Settings {
            font: iced_term::settings::FontSettings {
                size: 20.0,
                scale_factor: 1.4,
                font_type: Font::with_name(if cfg!(windows) { "Consolas" } else { "Menlo" }),
            },
            theme: iced_term::settings::ThemeSettings::new(Box::new(iced_term::ColorPalette {
                foreground: "#ffffff".into(),
                background: "#000000".into(),
                ..Default::default()
            })),
            backend: iced_term::settings::BackendSettings {
                program,
                args: vec![
                    "--fixture".into(),
                    output.to_str().ok_or("non-Unicode output path")?.into(),
                ],
                ..Default::default()
            },
        },
    )?;
    let mut pid = 0;
    let result = run(&executor, &mut renderer, &mut terminal, &output, &mut pid);
    let shutdown = terminal.shutdown_handle();
    drop(terminal);
    let done = executor
        .block_on(async { tokio::time::timeout(Duration::from_secs(10), shutdown.wait()).await })?;
    check(
        done.reason == iced_term::ExitReason::Requested,
        "synthetic cursor cleanup failed",
    )?;
    fs::write(
        output.join("cleanup.csv"),
        format!("session,reason,pid\n21001,Requested,{pid}\n"),
    )?;
    result?;
    events.record("fixture", 21001, pid as usize, "Requested")?;
    if let Some(program) = args.get(2) {
        micro::verify(&executor, &mut renderer, Path::new(program), &output)?;
    } else {
        fs::write(
            output.join("micro-unavailable.txt"),
            "No pinned official Micro executable supplied; synthetic reproduction only.\n",
        )?;
    }
    fs::write(output.join("completed.txt"), "passed\n")?;
    events.record("passed", 0, 1, "steady cursor")?;
    Ok(())
}

fn fixture(diagnostics: Option<&Path>) -> io::Result<()> {
    input::configure(diagnostics)?;
    let mut output = io::stdout().lock();
    write!(output, "\x1b]2;pike:cursor:boot:{}\x07", std::process::id())?;
    output.flush()?;
    let mut input = io::stdin().lock();
    while let Some(line) = input::command(&mut input)? {
        if let Some(state) = line.strip_prefix("state ") {
            let sequence = match state {
                "block" => "\x1b[2 q",
                "underline" => "\x1b[4 q",
                "beam" => "\x1b[6 q",
                "hide" => "\x1b[?25l",
                "show" => "\x1b[?25h",
                "color" => "\x1b]12;#ff8000\x07",
                "reset-color" => "\x1b]112\x07",
                "alt" => "\x1b[?1049h",
                "main" => "\x1b[?1049l",
                _ => return Err(io::Error::other("unknown synthetic state transition")),
            };
            write!(output, "{sequence}\x1b]2;pike:cursor:{line}\x07")?;
            output.flush()?;
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        check(fields.len() == 4, "invalid synthetic cursor command")?;
        let case: usize = fields[0].parse().map_err(io::Error::other)?;
        let style: u8 = fields[1].parse().map_err(io::Error::other)?;
        let show: u8 = fields[2].parse().map_err(io::Error::other)?;
        let alt: u8 = fields[3].parse().map_err(io::Error::other)?;
        write!(output, "\x1b[?1049l\x1b[0m\x1b[2J\x1b[H")?;
        if alt == 1 {
            write!(output, "\x1b[?1049h")?;
        }
        let scene = match case {
            0 | 3 => "AXB\x1b[2G",
            1 => "X\x1b[7mY\x1b[27mZ\x1b[2D", // Upstream PR86.
            2 => "\x1b[48;2;45;75;100mAXYZ       \x1b[2G", // Micro issue100.
            4 => "A界B\x1b[3G",               // Cursor points at the continuation.
            5 => "Ae\u{301}B\x1b[2G",
            6 => "\x1b[37;47mAXB\x1b[2G",
            _ => return Err(io::Error::other("unknown synthetic scene")),
        };
        write!(
            output,
            "{scene}\x1b[{style} q\x1b]12;#00ff00\x07\x1b[?25{}\x1b]2;pike:cursor:{line}\x07",
            if show == 0 { "l" } else { "h" }
        )?;
        output.flush()?;
    }
    Ok(())
}

fn wait(executor: &SharedExecutor, terminal: &mut Terminal, title: &str) -> io::Result<String> {
    executor.block_on(async {
        let mut events = into_recipes(terminal.subscription())
            .pop()
            .unwrap()
            .stream(stream::pending().boxed());
        tokio::time::timeout(Duration::from_secs(10), async {
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
                if let Some(value) = ready {
                    terminal.refresh();
                    return Ok(value);
                }
            }
            Err(io::Error::other("cursor fixture ended before readiness"))
        })
        .await
        .map_err(io::Error::other)?
    })
}
fn send(
    executor: &SharedExecutor,
    terminal: &mut Terminal,
    case: usize,
    style: u8,
    show: bool,
    alt: bool,
) -> io::Result<()> {
    let command = format!("{case} {style} {} {}", u8::from(show), u8::from(alt));
    let newline = if cfg!(windows) { "\r" } else { "\n" };
    terminal.handle(Command::ProxyToBackend(BackendCommand::Write(
        format!("{command}{newline}").into_bytes(),
    )));
    wait(executor, terminal, &format!("pike:cursor:{command}"))?;
    if case == 3 {
        let cell = terminal.renderable_content().unwrap().terminal_size;
        terminal.handle(Command::ProxyToBackend(BackendCommand::SelectStart(
            iced_term::SelectionType::Simple,
            (cell.cell_width * 1.2, cell.cell_height * 0.5),
        )));
        terminal.handle(Command::ProxyToBackend(BackendCommand::SelectUpdate((
            cell.cell_width * 1.9,
            cell.cell_height * 0.5,
        ))));
    }
    Ok(())
}

fn run(
    executor: &SharedExecutor,
    renderer: &mut iced::Renderer,
    terminal: &mut Terminal,
    output: &Path,
    pid: &mut u32,
) -> io::Result<()> {
    *pid = wait(executor, terminal, "pike:cursor:boot:")?
        .rsplit(':')
        .next()
        .unwrap()
        .parse()
        .map_err(io::Error::other)?;
    let window = iced_term::WindowFocus::default();
    window.set_focused(true);
    terminal.set_window_focus(window);
    terminal.set_focused(true);
    let size = terminal.renderable_content().unwrap().terminal_size;
    let cell = Size::new(size.cell_width, size.cell_height);
    let bounds = Rectangle::new(
        Point::new(24.0, 32.0),
        Size::new(cell.width * 12.0 + 0.01, cell.height * 4.0 + 0.01),
    );
    terminal.handle(Command::ProxyToBackend(BackendCommand::Resize(
        Some(bounds.size()),
        None,
    )));
    let viewport = Rectangle::with_size(SCREEN);
    let mut tree = Tree::empty();
    pixels::focus(terminal, renderer, &mut tree, bounds);
    let mut checks = fs::File::create(output.join("pixels.csv"))?;
    writeln!(checks, "case,style,scale,visible,legible,clip,blink_policy")?;
    for case in 0..7 {
        let width = if case == 4 { 2.0 } else { 1.0 };
        let cursor = Rectangle::new(
            Point::new(bounds.x + cell.width, bounds.y),
            Size::new(cell.width * width, cell.height),
        );
        let mut previous = Vec::new();
        for style in 1..=6 {
            send(executor, terminal, case, style, false, false)?;
            let hidden = pixels::render(terminal, renderer, &mut tree, bounds, viewport, 1.5);
            if case == 4 && style == 1 {
                report::screenshot(&output.join("wide-hidden.ppm"), hidden.clone())?;
            }
            send(executor, terminal, case, style, true, false)?;
            for scale in [1.0, 1.25, 1.5, 2.0] {
                let shown = pixels::render(terminal, renderer, &mut tree, bounds, viewport, scale);
                if let Err(error) = pixels::legible(&shown, cursor) {
                    report::screenshot(&output.join("failed-glyph.ppm"), shown.clone())?;
                    let content = terminal.renderable_content().unwrap();
                    fs::write(
                        output.join("failed-glyph.txt"),
                        format!(
                            "case={case} style={style} scale={scale} bounds={bounds:?} cell={cell:?} cursor={:?}\n",
                            content.cursor_point
                        ),
                    )?;
                    return Err(error);
                }
                if case == 0 && style <= 2 {
                    let colored = pixels::crop(&shown, cursor.shrink(1.0))?;
                    check(
                        colored
                            .chunks_exact(4)
                            .filter(|p| *p == [0, 255, 0, 255])
                            .count()
                            > 20,
                        "OSC12 cursor color was not painted",
                    )?;
                }
                if scale == 1.5 {
                    check(
                        !pixels::same(
                            &pixels::crop(&hidden, cursor.shrink(0.5))?,
                            &pixels::crop(&shown, cursor.shrink(0.5))?,
                        ),
                        "cursor remained invisible on highlighted cells",
                    )?;
                    if style >= 3 && case != 6 {
                        let center = Rectangle::new(
                            Point::new(
                                cursor.x + cursor.width * 0.3,
                                cursor.y + cursor.height * 0.3,
                            ),
                            Size::new(cursor.width * 0.4, cursor.height * 0.4),
                        );
                        check(
                            pixels::same(
                                &pixels::crop(&hidden, center)?,
                                &pixels::crop(&shown, center)?,
                            ),
                            "non-block cursor recolored the covered glyph",
                        )?;
                    }
                    if style % 2 == 1 {
                        previous = shown.rgba.to_vec();
                    } else {
                        check(
                            pixels::same(&previous, &shown.rgba),
                            "blinking style did not use its steady counterpart",
                        )?;
                    }
                    if style % 2 == 0 {
                        report::screenshot(
                            &output.join(format!("case-{case}-style-{style}.ppm")),
                            shown.clone(),
                        )?;
                    }
                }
                // Clip through the cursor and raw Unicode span, not only pane edges.
                let clip = Rectangle::new(
                    Point::new(cursor.x + cell.width * 0.5, cursor.y + cell.height * 0.5),
                    Size::new(cell.width * 2.0, cell.height),
                );
                let clipped = pixels::render(terminal, renderer, &mut tree, bounds, clip, scale);
                for (index, pixel) in clipped.rgba.chunks_exact(4).enumerate() {
                    let point = Point::new(
                        (index % clipped.size.width as usize) as f32 / scale,
                        (index / clipped.size.width as usize) as f32 / scale,
                    );
                    if bounds.shrink(1.0 / scale).contains(point)
                        && !clip.expand(1.0 / scale).contains(point)
                    {
                        check(
                            pixel == [255, 255, 255, 255] || pixel == [0, 0, 0, 255],
                            "cursor/glyph escaped ancestor clip",
                        )?;
                    }
                }
                writeln!(checks, "{case},{style},{scale},passed,passed,passed,steady")?;
            }
        }
        // Inactive hollow presentation must also reveal an equal-color glyph.
        pixels::event(
            terminal,
            renderer,
            &mut tree,
            bounds,
            Event::Window(iced::window::Event::Unfocused),
        );
        pixels::legible(
            &pixels::render(terminal, renderer, &mut tree, bounds, viewport, 1.5),
            cursor,
        )?;
        pixels::event(
            terminal,
            renderer,
            &mut tree,
            bounds,
            Event::Window(iced::window::Event::Focused),
        );
    }
    // Normal/alternate transitions and owner focus go through public VT/widget paths.
    send(executor, terminal, 0, 2, true, true)?;
    pixels::legible(
        &pixels::render(terminal, renderer, &mut tree, bounds, viewport, 1.0),
        Rectangle::new(Point::new(bounds.x + cell.width, bounds.y), cell),
    )?;
    send(executor, terminal, 5, 2, true, false)?;
    transitions::verify(executor, renderer, terminal, &mut tree, bounds, output)?;
    counters(executor, renderer, terminal, &mut tree, bounds, output)
}

fn counters(
    executor: &SharedExecutor,
    renderer: &mut iced::Renderer,
    terminal: &mut Terminal,
    tree: &mut Tree,
    bounds: Rectangle,
    output: &Path,
) -> io::Result<()> {
    // Drain already queued fixture Wakeups before observing a steady interval.
    let mut stream = into_recipes(terminal.subscription())
        .pop()
        .unwrap()
        .stream(stream::pending().boxed());
    while let Some(Some(iced_term::Event::BackendCall(_, command))) = stream.next().now_or_never() {
        terminal.handle(Command::ProxyToBackend(command));
    }
    let viewport = Rectangle::with_size(SCREEN);
    let mut samples = fs::File::create(output.join("work.csv"))?;
    writeln!(
        samples,
        "phase,external_frames,cursor_redraw_requests,font_delta,snapshot_delta,cache_delta,transition_fonts,transition_snapshots,transition_cache"
    )?;
    for phase in [
        "focused",
        "unfocused",
        "restored",
        "minimized",
        "unminimized",
        "hidden",
        "visible",
    ] {
        let transition = terminal.work_stats();
        let transition_snapshots = terminal.snapshot_stats().snapshots;
        let bounds = if phase == "minimized" {
            Rectangle::new(bounds.position(), Size::ZERO)
        } else {
            bounds
        };
        match phase {
            "unfocused" => {
                pixels::event(
                    terminal,
                    renderer,
                    tree,
                    bounds,
                    Event::Window(iced::window::Event::Unfocused),
                );
            }
            "restored" => {
                pixels::event(
                    terminal,
                    renderer,
                    tree,
                    bounds,
                    Event::Window(iced::window::Event::Focused),
                );
            }
            "hidden" => {
                terminal.set_visible(false);
            }
            "visible" => {
                terminal.set_visible(true);
                terminal.set_focused(true);
            }
            _ => {}
        }
        pixels::render(terminal, renderer, tree, bounds, viewport, 1.0);
        let before = terminal.work_stats();
        let snapshots = terminal.snapshot_stats().snapshots;
        executor.block_on(async { tokio::time::sleep(Duration::from_millis(200)).await });
        let mut requests = 0;
        for _ in 0..32 {
            if phase != "hidden" {
                let request = pixels::event(
                    terminal,
                    renderer,
                    tree,
                    bounds,
                    Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
                );
                requests += usize::from(request != iced::window::RedrawRequest::Wait);
            }
            pixels::render(terminal, renderer, tree, bounds, viewport, 1.0);
        }
        let after = terminal.work_stats();
        let snapshot_delta = terminal.snapshot_stats().snapshots - snapshots;
        check(
            requests == 0 && before == after && snapshot_delta == 0,
            "steady idle/focus/hide/minimize scheduled cursor work",
        )?;
        writeln!(
            samples,
            "{phase},32,{requests},{},{snapshot_delta},{},{},{},{}",
            after.font_measurements - before.font_measurements,
            after.cache_invalidations - before.cache_invalidations,
            before.font_measurements - transition.font_measurements,
            snapshots - transition_snapshots,
            before.cache_invalidations - transition.cache_invalidations
        )?;
    }
    Ok(())
}
