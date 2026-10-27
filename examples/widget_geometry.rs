//! Opt-in real-PTY/offscreen-renderer probe. Screenshots contain synthetic data only.
#[path = "compatibility/report.rs"]
mod report;
#[path = "support/widget_events.rs"]
mod widget_events;

use iced::advanced::renderer::{Headless, Renderer as _, Style};
use iced::advanced::subscription::into_recipes;
use iced::advanced::widget::Tree;
use iced::advanced::{Layout, layout, mouse};
use iced::futures::{StreamExt, stream};
use iced::widget::{PaneGrid, column, container, pane_grid, stack, text};
use iced::{Color, Element, Executor, Fill, Point, Rectangle, Size, Theme};
use iced_term::{
    AlacrittyEvent, BackendCommand, Command, Event, PreparedTerminal, Terminal, TerminalView,
};
use pike::executor::SharedExecutor;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Duration;

const SCREEN: Size<f32> = Size::new(640.0, 400.0);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--fixture") {
        return fixture(args.get(2).ok_or("missing fixture id")?).map_err(Into::into);
    }
    let output = Path::new(args.get(1).ok_or("expected fresh output directory")?);
    if output.exists() {
        return Err("choose a fresh output directory".into());
    }
    std::fs::create_dir_all(output)?;
    let executor = SharedExecutor::new()?;
    let mut renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or("offscreen renderer unavailable")?;
    let program = std::env::current_exe()?
        .into_os_string()
        .into_string()
        .map_err(|_| "non-Unicode fixture path")?;
    let mut terminals = Vec::with_capacity(3);
    // A partial preparation failure follows the same acknowledged cleanup path
    // as a failed pixel check. Never strand earlier synthetic children.
    let result = (|| -> io::Result<()> {
        for id in 1..=3 {
            terminals.push(
                PreparedTerminal::new(
                    id,
                    iced_term::settings::BackendSettings {
                        program: program.clone(),
                        args: vec!["--fixture".into(), id.to_string()],
                        ..Default::default()
                    },
                )?
                .attach(Default::default(), Default::default()),
            );
        }
        run(&executor, &mut renderer, &mut terminals, output)
    })();
    let handles: Vec<_> = terminals.iter().map(Terminal::shutdown_handle).collect();
    drop(terminals);
    let completions = executor.block_on(async {
        let mut completions = Vec::with_capacity(handles.len());
        for handle in handles {
            let completion = tokio::time::timeout(Duration::from_secs(10), handle.wait())
                .await
                .map_err(io::Error::other)?;
            check(
                completion.reason == iced_term::ExitReason::Requested,
                "fixture shutdown was not requested cleanup",
            )?;
            completions.push(completion);
        }
        Ok::<_, io::Error>(completions)
    })?;
    let mut cleanup = std::fs::File::create(output.join("cleanup.csv"))?;
    writeln!(cleanup, "session,reason,output_drained")?;
    for (index, completion) in completions.iter().enumerate() {
        writeln!(
            cleanup,
            "{},Requested,{}",
            index + 1,
            completion.output_drained
        )?;
    }
    result?;
    std::fs::write(output.join("completed.txt"), "passed\n")?;
    Ok(())
}

fn fixture(id: &str) -> io::Result<()> {
    let mut output = io::stdout().lock();
    for _ in 0..150 {
        writeln!(output, "SYNTHETIC HISTORY {id}")?;
    }
    // Leave an oversized colored grid so screenshots also test the interval
    // before the application's next resize message has been handled.
    write!(output, "\x1b[2J\x1b[H\x1b[48;2;180;20;30m")?;
    for row in 1..=40 {
        write!(output, "\x1b[{row};1HSESSION {id} {}", "X".repeat(60))?;
    }
    if id == "2" {
        write!(output, "\x1b[?1000h")?;
    }
    write!(
        output,
        "\x1b[0m\x1b[2;2H\x1b[?25h\x1b]2;pike:geometry:{id}:{}\x07",
        std::process::id()
    )?;
    output.flush()?;
    drop(output);
    io::stdin().read_exact(&mut [0_u8; 1])?;
    Ok(())
}

async fn ready(terminal: &mut Terminal) -> io::Result<u32> {
    let mut events = into_recipes(terminal.subscription())
        .pop()
        .unwrap()
        .stream(stream::pending().boxed());
    while let Some(Event::BackendCall(_, command)) = events.next().await {
        let pid = match &command {
            BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Title(title)) => title
                .strip_prefix(&format!("pike:geometry:{}:", terminal.id))
                .and_then(|pid| pid.parse().ok()),
            BackendCommand::EventBridge(iced_term::EventBridgeEvent::Error(_)) => {
                return Err(io::Error::other("fixture event bridge failed"));
            }
            _ => None,
        };
        terminal.handle(Command::ProxyToBackend(command));
        if let Some(pid) = pid {
            // A title is a control event and may be delivered before the
            // coalesced Wakeup. Synchronize the fixture output/modes explicitly.
            terminal.refresh();
            let content = terminal.renderable_content().unwrap();
            let expected = format!("SESSION {}", terminal.id);
            check(
                content
                    .viewport
                    .display_iter()
                    .take(expected.len())
                    .map(|cell| cell.c)
                    .eq(expected.chars()),
                "fixture output was not synchronized at readiness",
            )?;
            if terminal.id == 2 {
                check(
                    content
                        .terminal_mode
                        .intersects(iced_term::TermMode::MOUSE_MODE),
                    "synthetic mouse reporting mode was not synchronized",
                )?;
            }
            return Ok(pid);
        }
    }
    Err(io::Error::other("fixture ended before readiness"))
}

fn render(
    mut element: Element<'_, Event>,
    tree: &mut Tree,
    renderer: &mut iced::Renderer,
    bounds: Rectangle,
    viewport: Rectangle,
    scale: f32,
) -> iced::window::Screenshot {
    renderer.reset(Rectangle::with_size(SCREEN));
    tree.diff(element.as_widget());
    let node = element
        .as_widget_mut()
        .layout(
            tree,
            renderer,
            &layout::Limits::new(bounds.size(), bounds.size()),
        )
        .move_to(bounds.position());
    element.as_widget().draw(
        tree,
        renderer,
        &Theme::Dark,
        &Style::default(),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &viewport,
    );
    let size = Size::new(
        (SCREEN.width * scale) as u32,
        (SCREEN.height * scale) as u32,
    );
    iced::window::Screenshot::new(
        Headless::screenshot(renderer, size, scale, Color::WHITE),
        size,
        scale,
    )
}

fn check(condition: bool, message: &'static str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}

fn crop(
    shot: &iced::window::Screenshot,
    bounds: Rectangle,
) -> io::Result<iced::window::Screenshot> {
    // Compare interior pixels with identical fractional phases after moving.
    let scale = shot.scale_factor;
    shot.crop(Rectangle {
        x: (bounds.x * scale).ceil() as u32 + 1,
        y: (bounds.y * scale).ceil() as u32 + 1,
        width: (bounds.width * scale).floor() as u32 - 3,
        height: (bounds.height * scale).floor() as u32 - 3,
    })
    .map_err(io::Error::other)
}

fn compare_translation(
    a: &iced::window::Screenshot,
    b: &iced::window::Screenshot,
) -> io::Result<(u8, f64)> {
    check(a.size == b.size, "translation crop dimensions differ")?;
    let mut maximum = 0_u8;
    let mut total = 0_u64;
    let mut changed = 0;
    for (a, b) in a.rgba.chunks_exact(4).zip(b.rgba.chunks_exact(4)) {
        let delta = a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        maximum = maximum.max(delta);
        total += a
            .iter()
            .zip(b)
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum::<u64>();
        changed += usize::from(delta > 32);
    }
    let mean = total as f64 / a.rgba.len() as f64;
    // Float transforms can vary edge antialiasing slightly by screen origin.
    // A stale glyph/cursor position changes much more than these tolerances.
    check(
        mean < 0.25 && changed * 1000 <= a.rgba.len() / 4,
        "cached glyphs/cursor did not move with the pane",
    )?;
    Ok((maximum, mean))
}

fn check_clipped(shot: &iced::window::Screenshot, bounds: Rectangle) -> io::Result<()> {
    let scale = shot.scale_factor;
    for (index, pixel) in shot.rgba.chunks_exact(4).enumerate() {
        let x = (index % shot.size.width as usize) as f32;
        let y = (index / shot.size.width as usize) as f32;
        if x < (bounds.x * scale).floor()
            || x >= ((bounds.x + bounds.width) * scale).ceil()
            || y < (bounds.y * scale).floor()
            || y >= ((bounds.y + bounds.height) * scale).ceil()
        {
            check(
                pixel == [255, 255, 255, 255],
                "terminal painted outside its pane",
            )?;
        }
    }
    Ok(())
}

fn run(
    executor: &SharedExecutor,
    renderer: &mut iced::Renderer,
    terminals: &mut [Terminal],
    output: &Path,
) -> io::Result<()> {
    let mut report = report::Report::new(output)?;
    for terminal in terminals.iter_mut() {
        let pid = executor
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(10), ready(terminal)).await
            })
            .map_err(io::Error::other)??;
        report.record("fixture", terminal.id, pid, 0)?;
    }
    let first = Rectangle::new(Point::new(8.5, 32.25), Size::new(200.0, 120.0));
    let moved = Rectangle::new(Point::new(288.5, 144.25), first.size());
    let screen = Rectangle::with_size(SCREEN);
    let mut tree = Tree::empty();
    terminals[0].handle(Command::ProxyToBackend(BackendCommand::SelectStart(
        iced_term::SelectionType::Simple,
        (0.0, 0.0),
    )));
    terminals[0].handle(Command::ProxyToBackend(BackendCommand::SelectUpdate((
        80.0, 0.0,
    ))));
    for scale in [1.25, 1.5, 2.0] {
        let before = terminals[0].work_stats();
        let initial = render(
            TerminalView::show(&terminals[0]),
            &mut tree,
            renderer,
            first,
            screen,
            scale,
        );
        let translated = render(
            TerminalView::show(&terminals[0]),
            &mut tree,
            renderer,
            moved,
            screen,
            scale,
        );
        check_clipped(&initial, first)?;
        check_clipped(&translated, moved)?;
        report::screenshot(
            &output.join(format!("initial-{scale}.ppm")),
            initial.clone(),
        )?;
        report::screenshot(
            &output.join(format!("translated-{scale}.ppm")),
            translated.clone(),
        )?;
        let (max_delta, mean_delta) =
            compare_translation(&crop(&initial, first)?, &crop(&translated, moved)?)?;
        report.record(
            "translation_delta",
            1,
            scale,
            format!("{max_delta}:{mean_delta:.6}"),
        )?;
        check(
            terminals[0].work_stats() == before,
            "moving geometry performed session work",
        )?;
        report::screenshot(&output.join(format!("moved-{scale}.ppm")), translated)?;
        // Replace an equal-size view in the very same tree, then return to A.
        let other = render(
            TerminalView::show(&terminals[1]),
            &mut tree,
            renderer,
            first,
            screen,
            scale,
        );
        check(
            initial.rgba != other.rgba,
            "different terminal IDs rendered identical fixtures",
        )?;
        let restored = render(
            TerminalView::show(&terminals[0]),
            &mut tree,
            renderer,
            first,
            screen,
            scale,
        );
        check(
            initial.rgba == restored.rgba,
            "tab replacement changed retained terminal rendering",
        )?;
        // The viewport may hide part of a pane underneath surrounding chrome.
        let viewport = Rectangle::new(Point::new(60.0, 60.0), Size::new(80.0, 60.0));
        let clipped = render(
            TerminalView::show(&terminals[0]),
            &mut tree,
            renderer,
            first,
            viewport,
            scale,
        );
        for (index, pixel) in clipped.rgba.chunks_exact(4).enumerate() {
            let position = Point::new(
                (index % clipped.size.width as usize) as f32 / scale,
                (index / clipped.size.width as usize) as f32 / scale,
            );
            // Leave one physical pixel at clip edges for renderer rounding.
            if !viewport.expand(1.0 / scale).contains(position)
                && first.shrink(1.0 / scale).contains(position)
            {
                check(
                    pixel == [255, 255, 255, 255] || pixel == [24, 24, 24, 255],
                    "terminal text/cursor/selection escaped the ancestor viewport",
                )?;
            }
        }
        report::screenshot(&output.join(format!("clipped-{scale}.ppm")), clipped)?;
        report.record("pixel_checks", 1, scale, 0)?;
    }
    let (mut panes, a) = pane_grid::State::new(0_usize);
    let (b, _) = panes.split(pane_grid::Axis::Vertical, a, 1).unwrap();
    let (c, _) = panes.split(pane_grid::Axis::Horizontal, a, 2).unwrap();
    for step in 0..5 {
        match step {
            1 => panes.swap(b, c),
            2 => panes.maximize(a),
            3 => panes.restore(),
            _ => {}
        }
        let grid = PaneGrid::new(&panes, |_, index, _| {
            pane_grid::Content::new(TerminalView::show(&terminals[*index]))
        })
        .spacing(6)
        .width(Fill)
        .height(Fill);
        let scene =
            container(column![text("SYNTHETIC HEADER — must remain unobscured"), grid].spacing(8))
                .padding(8)
                .width(Fill)
                .height(Fill);
        let scene: Element<'_, Event> = if step == 4 {
            let modal = container(text("SYNTHETIC MODAL"))
                .padding(8)
                .width(220)
                .height(80)
                .style(|_| container::Style {
                    background: Some(Color::from_rgb8(0, 255, 255).into()),
                    text_color: Some(Color::BLACK),
                    ..Default::default()
                });
            stack![scene, container(modal).center_x(Fill).center_y(Fill)].into()
        } else {
            scene.into()
        };
        let shot = render(scene, &mut tree, renderer, screen, screen, 1.25);
        if step == 4 {
            let index = (287 * shot.size.width as usize + 400) * 4;
            check(
                shot.rgba[index..index + 4] == [0, 255, 255, 255],
                "terminal painted over an opaque modal",
            )?;
        }
        report::screenshot(&output.join(format!("nested-{step}.ppm")), shot)?;
    }
    check(
        terminals[0]
            .renderable_content()
            .unwrap()
            .selectable_range
            .is_some(),
        "session selection was discarded",
    )?;
    let before = terminals[0].work_stats();
    terminals[0].handle(Command::ChangeFont(iced_term::settings::FontSettings {
        size: 22.0,
        ..Default::default()
    }));
    terminals[0].handle(Command::ChangeTheme(Box::new(iced_term::ColorPalette {
        foreground: "#00ff00".into(),
        ..Default::default()
    })));
    let shot = render(
        TerminalView::show(&terminals[0]),
        &mut tree,
        renderer,
        first,
        screen,
        1.5,
    );
    check_clipped(&shot, first)?;
    report::screenshot(&output.join("font-theme.ppm"), shot)?;
    check(
        terminals[0].work_stats().font_measurements == before.font_measurements + 1,
        "font change was not measured exactly once",
    )?;

    for terminal in terminals.iter() {
        check(
            terminal.shutdown_handle().completion().is_none(),
            "layout change terminated a fixture",
        )?;
    }
    widget_events::verify(renderer, terminals, first)?;
    report.record("event_checks", 0, 32, 0)?;
    std::fs::write(output.join("renderer.txt"), renderer.name())?;
    report.record("passed", 0, 3, 0)
}
