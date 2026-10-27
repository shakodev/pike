//! State-only VT/host changes: no printable output or resize between checks.
use crate::{SCREEN, check, pixels, report, send};
use iced::advanced::subscription::into_recipes;
use iced::advanced::widget::Tree;
use iced::futures::{StreamExt, stream};
use iced::{Event, Executor, Point, Rectangle, Size};
use iced_term::{AlacrittyEvent, BackendCommand, Command, Terminal};
use pike::executor::SharedExecutor;
use std::{fs, io, path::Path};

fn state(executor: &SharedExecutor, terminal: &mut Terminal, name: &str) -> io::Result<()> {
    let newline = if cfg!(windows) { "\r" } else { "\n" };
    terminal.handle(Command::ProxyToBackend(BackendCommand::Write(
        format!("state {name}{newline}").into_bytes(),
    )));
    let title = format!("pike:cursor:state {name}");
    executor.block_on(async {
        let mut events = into_recipes(terminal.subscription()).pop().unwrap().stream(stream::pending().boxed());
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let mut ready = false;
            while let Some(iced_term::Event::BackendCall(_, command)) = events.next().await {
                let woke = matches!(&command, BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup));
                if matches!(&command, BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Title(value)) if value == &title) { ready = true; }
                terminal.handle(Command::ProxyToBackend(command));
                if ready && woke { return Ok(()); }
            }
            Err(io::Error::other("state-only source ended before its normal Wakeup"))
        }).await.map_err(io::Error::other)?
    })
}
pub fn verify(
    executor: &SharedExecutor,
    renderer: &mut iced::Renderer,
    terminal: &mut Terminal,
    tree: &mut Tree,
    bounds: Rectangle,
    output: &Path,
) -> io::Result<()> {
    let viewport = Rectangle::with_size(SCREEN);
    let size = terminal.renderable_content().unwrap().terminal_size;
    let cell = Rectangle::new(
        Point::new(bounds.x + size.cell_width, bounds.y),
        Size::new(size.cell_width, size.cell_height),
    );
    let center = Rectangle::new(
        Point::new(cell.x + cell.width * 0.3, cell.y + cell.height * 0.3),
        Size::new(cell.width * 0.4, cell.height * 0.4),
    );
    for case in [1, 5] {
        send(executor, terminal, case, 2, true, false)?;
        let fonts = terminal.work_stats().font_measurements;
        let original: Vec<_> = terminal
            .renderable_content()
            .unwrap()
            .viewport
            .cells()
            .iter()
            .map(|cell| cell.c)
            .collect();
        state(executor, terminal, "hide")?;
        let hidden = pixels::render(terminal, renderer, tree, bounds, viewport, 1.5);
        state(executor, terminal, "show")?;
        let block = pixels::render(terminal, renderer, tree, bounds, viewport, 1.5);
        check(
            !pixels::same(&hidden.rgba, &block.rgba),
            "state-only cursor show did not repaint",
        )?;
        pixels::legible(&block, cell)?;
        for shape in ["underline", "beam"] {
            state(executor, terminal, shape)?;
            let shot = pixels::render(terminal, renderer, tree, bounds, viewport, 1.5);
            check(
                !pixels::same(&block.rgba, &shot.rgba),
                "state-only DECSCUSR did not repaint",
            )?;
            check(
                pixels::same(
                    &pixels::crop(&hidden, center)?,
                    &pixels::crop(&shot, center)?,
                ),
                "state-only non-block shape retained stale glyph color",
            )?;
        }
        state(executor, terminal, "block")?;
        state(executor, terminal, "color")?;
        let colored = pixels::render(terminal, renderer, tree, bounds, viewport, 1.5);
        if case == 5 {
            check(
                !pixels::same(&block.rgba, &colored.rgba),
                "state-only OSC12 did not repaint",
            )?;
        }
        state(executor, terminal, "reset-color")?;
        let reset = pixels::render(terminal, renderer, tree, bounds, viewport, 1.5);
        if case == 5 {
            check(
                !pixels::same(&colored.rgba, &reset.rgba),
                "state-only cursor color reset did not repaint",
            )?;
        }
        if case == 5 {
            // Owner focus can change while this terminal has no widget tree.
            // Replacing the tree must not reuse the old focused canvas pixels.
            let owner = iced_term::WindowFocus::default();
            owner.set_focused(true);
            terminal.set_window_focus(owner.clone());
            let before = terminal.snapshot_stats().snapshots;
            let mut replacement = Tree::empty();
            owner.set_focused(false);
            let inactive =
                pixels::render(terminal, renderer, &mut replacement, bounds, viewport, 1.5);
            check(
                pixels::same(
                    &pixels::crop(&hidden, center)?,
                    &pixels::crop(&inactive, center)?,
                ),
                "replacement tree retained a focused cursor after owner focus loss",
            )?;
            check(
                terminal.snapshot_stats().snapshots == before,
                "owner-only focus copied cells",
            )?;
            owner.set_focused(true);
            pixels::legible(
                &pixels::render(terminal, renderer, &mut replacement, bounds, viewport, 1.5),
                cell,
            )?;
        }
        let before = terminal.snapshot_stats().snapshots;
        pixels::event(
            terminal,
            renderer,
            tree,
            bounds,
            Event::Window(iced::window::Event::Unfocused),
        );
        let inactive = pixels::render(terminal, renderer, tree, bounds, viewport, 1.5);
        check(
            pixels::same(
                &pixels::crop(&hidden, center)?,
                &pixels::crop(&inactive, center)?,
            ),
            "unfocused cursor retained block glyph recoloring",
        )?;
        check(
            terminal.snapshot_stats().snapshots == before,
            "focus-only transition copied viewport cells",
        )?;
        if case == 5 {
            terminal.handle(Command::ChangeTheme(Box::new(iced_term::ColorPalette {
                foreground: "#ff00ff".into(),
                background: "#000000".into(),
                ..Default::default()
            })));
            let themed = pixels::render(terminal, renderer, tree, bounds, viewport, 1.5);
            check(
                !pixels::same(
                    &pixels::crop(&inactive, center)?,
                    &pixels::crop(&themed, center)?,
                ),
                "cached Unicode glyph retained stale palette color",
            )?;
            check(
                terminal.snapshot_stats().snapshots == before,
                "palette-only transition copied viewport cells",
            )?;
            terminal.handle(Command::ProxyToBackend(BackendCommand::SelectStart(
                iced_term::SelectionType::Simple,
                (size.cell_width * 1.2, size.cell_height * 0.5),
            )));
            terminal.handle(Command::ProxyToBackend(BackendCommand::SelectUpdate((
                size.cell_width * 1.9,
                size.cell_height * 0.5,
            ))));
            let selected = pixels::render(terminal, renderer, tree, bounds, viewport, 1.5);
            check(
                !pixels::same(&themed.rgba, &selected.rgba),
                "state-only selection did not repaint Unicode glyphs",
            )?;
            report::screenshot(&output.join("state-selected.ppm"), selected)?;
        }
        pixels::event(
            terminal,
            renderer,
            tree,
            bounds,
            Event::Window(iced::window::Event::Focused),
        );
        pixels::legible(
            &pixels::render(terminal, renderer, tree, bounds, viewport, 1.5),
            cell,
        )?;
        let current: Vec<_> = terminal
            .renderable_content()
            .unwrap()
            .viewport
            .cells()
            .iter()
            .map(|cell| cell.c)
            .collect();
        check(
            current == original,
            "state-only sequence changed printable cells",
        )?;
        check(
            terminal.work_stats().font_measurements == fonts,
            "state-only transition remeasured fonts",
        )?;
    }
    state(executor, terminal, "alt")?;
    check(
        terminal
            .renderable_content()
            .unwrap()
            .viewport
            .cells()
            .iter()
            .all(|cell| cell.c == ' '),
        "state-only alternate screen was not empty",
    )?;
    state(executor, terminal, "main")?;
    check(
        terminal
            .renderable_content()
            .unwrap()
            .viewport
            .cells()
            .iter()
            .any(|cell| cell.c == 'e'),
        "state-only normal-screen restoration lost text",
    )?;
    fs::write(
        output.join("state-only.txt"),
        "DECSCUSR, hide/show, OSC12/reset, inverse/selection, owner focus, palette and alternate-screen state-only updates passed; no printed text, resize or font measurements.\n",
    )
}
