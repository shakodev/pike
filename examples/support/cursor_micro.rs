//! Run the pinned official editor inside the existing PTY. The helper process
//! only inherits that PTY; it records both PIDs and waits for Micro's real exit.
use crate::{SCREEN, check, pixels, report};
use iced::advanced::subscription::into_recipes;
use iced::advanced::widget::Tree;
use iced::futures::{StreamExt, stream};
use iced::{Executor, Point, Rectangle, Size};
use iced_term::{BackendCommand, Command, Terminal};
use pike::executor::SharedExecutor;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

pub fn fixture(args: &[String]) -> io::Result<()> {
    let [program, config, file, output] = args else {
        return Err(io::Error::other("invalid Micro fixture arguments"));
    };
    fs::write(
        Path::new(output).join("micro-parent.pid"),
        std::process::id().to_string(),
    )?;
    let mut child = std::process::Command::new(program)
        .args([
            "-config-dir",
            config,
            "-colorscheme",
            "pike-21",
            "-cursorline",
            "true",
            "-ruler",
            "false",
            "-syntax",
            "false",
            "-savehistory",
            "false",
            file,
        ])
        .spawn()?;
    fs::write(Path::new(output).join("micro.pid"), child.id().to_string())?;
    let status = child.wait()?;
    check(status.success(), "actual Micro exited unsuccessfully")
}

pub fn verify(
    executor: &SharedExecutor,
    renderer: &mut iced::Renderer,
    program: &Path,
    output: &Path,
) -> io::Result<()> {
    for (index, (name, color)) in [("highlighted", "#305070"), ("equal-color", "#eeeeee")]
        .into_iter()
        .enumerate()
    {
        let scene = output.join(format!("micro-{name}"));
        fs::create_dir_all(&scene)?;
        verify_scene(
            executor,
            renderer,
            program,
            &scene,
            output,
            color,
            21002 + index as u64,
        )?;
    }
    fs::write(
        output.join("micro-result.txt"),
        "Official Micro2.0.14 real-PTY highlighted and equal-color custom scenes passed; natural exits0 and drained cleanup.\n",
    )
}

fn verify_scene(
    executor: &SharedExecutor,
    renderer: &mut iced::Renderer,
    program: &Path,
    output: &Path,
    root: &Path,
    cursor_line: &str,
    id: u64,
) -> io::Result<()> {
    let config = output.join("micro-config");
    fs::create_dir_all(config.join("colorschemes"))?;
    // Official v2.0.14 color syntax; custom deterministic cursor-line colors.
    fs::write(
        config.join("colorschemes/pike-21.micro"),
        format!(
            "color-link default \"#eeeeee,#102030\"\ncolor-link cursor-line \"{cursor_line}\"\ncolor-link statusline \"#eeeeee,#102030\"\n"
        ),
    )?;
    let file = output.join("micro-synthetic.txt");
    fs::write(&file, "PIKE_MICRO_CURSOR_21\nsecond synthetic row\n")?;
    let version = std::process::Command::new(program)
        .arg("-version")
        .output()?;
    check(
        version.status.success(),
        "could not read pinned Micro version",
    )?;
    fs::write(output.join("micro-version.txt"), version.stdout)?;
    let own = std::env::current_exe()?;
    let string = |path: &Path| {
        path.to_str()
            .map(str::to_owned)
            .ok_or_else(|| io::Error::other("non-Unicode Micro fixture path"))
    };
    let mut terminal = Terminal::new(
        id,
        iced_term::settings::Settings {
            theme: Default::default(),
            font: iced_term::settings::FontSettings {
                size: 20.0,
                scale_factor: 1.4,
                font_type: iced::Font::with_name(if cfg!(windows) { "Consolas" } else { "Menlo" }),
            },
            backend: iced_term::settings::BackendSettings {
                program: string(&own)?,
                args: vec![
                    "--micro-fixture".into(),
                    string(program)?,
                    string(&config)?,
                    string(&file)?,
                    string(output)?,
                ],
                env: std::collections::HashMap::from([
                    ("TERM".into(), "xterm-256color".into()),
                    ("COLORTERM".into(), "truecolor".into()),
                    ("MICRO_TRUECOLOR".into(), "1".into()),
                ]),
                ..Default::default()
            },
        },
    )?;
    let shutdown = terminal.shutdown_handle();
    let result = (|| -> io::Result<()> {
        let window = iced_term::WindowFocus::default();
        window.set_focused(true);
        terminal.set_window_focus(window);
        terminal.set_focused(true);
        let cell = terminal.renderable_content().unwrap().terminal_size;
        let bounds = Rectangle::new(
            Point::new(24.0, 32.0),
            Size::new(cell.cell_width * 36.0 + 0.01, cell.cell_height * 8.0 + 0.01),
        );
        terminal.handle(Command::ProxyToBackend(BackendCommand::Resize(
            Some(bounds.size()),
            None,
        )));
        executor.block_on(async {
            let mut events = into_recipes(terminal.subscription())
                .pop()
                .unwrap()
                .stream(stream::pending().boxed());
            tokio::time::timeout(Duration::from_secs(15), async {
                while let Some(iced_term::Event::BackendCall(_, command)) = events.next().await {
                    terminal.handle(Command::ProxyToBackend(command));
                    terminal.refresh();
                    let content = terminal.renderable_content().unwrap();
                    let text: String = content.viewport.display_iter().map(|cell| cell.c).collect();
                    if text.contains("PIKE_MICRO_CURSOR_21")
                        && content
                            .terminal_mode
                            .contains(iced_term::TermMode::SHOW_CURSOR)
                    {
                        return Ok(());
                    }
                }
                Err(io::Error::other("Micro ended before rendering its fixture"))
            })
            .await
            .map_err(io::Error::other)?
        })?;
        let content = terminal.renderable_content().unwrap();
        let point = content.cursor_point;
        check(
            point.line.0 == 0 && point.column.0 == 0,
            "actual Micro cursor was not on the known first glyph",
        )?;
        let first = content.viewport.cells()[0].bg;
        let next = content.viewport.cells()[content.viewport.columns()].bg;
        check(
            first != next,
            "actual Micro did not highlight its cursor row",
        )?;
        let mut tree = Tree::empty();
        pixels::focus(&terminal, renderer, &mut tree, bounds);
        let shot = pixels::render(
            &terminal,
            renderer,
            &mut tree,
            bounds,
            Rectangle::with_size(SCREEN),
            1.5,
        );
        pixels::legible(
            &shot,
            Rectangle::new(
                bounds.position(),
                Size::new(cell.cell_width, cell.cell_height),
            ),
        )?;
        report::screenshot(&output.join("actual-micro.ppm"), shot)?;
        fs::write(
            output.join("micro-result.txt"),
            "Official Micro2.0.14, real PTY, custom cursor-line theme, first glyph/cursor legible.\n",
        )?;
        terminal.handle(Command::ProxyToBackend(BackendCommand::Write(vec![0x11]))); // Real Ctrl+Q; file unmodified.
        Ok(())
    })();
    if result.is_err() {
        terminal.request_close();
    }
    let completed = executor
        .block_on(async { tokio::time::timeout(Duration::from_secs(10), shutdown.wait()).await })
        .map_err(io::Error::other)?;
    drop(terminal);
    result?;
    check(
        completed.reason == iced_term::ExitReason::ChildExit
            && completed.exit_code == Some(0)
            && completed.output_drained,
        "actual Micro natural cleanup failed",
    )?;
    let pid = fs::read_to_string(output.join("micro.pid"))?;
    let mut cleanup = fs::OpenOptions::new()
        .append(true)
        .open(root.join("cleanup.csv"))?;
    writeln!(cleanup, "{id},ChildExit,{}", pid.trim())?;
    Ok(())
}
