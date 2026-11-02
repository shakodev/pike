//! Finite native-renderer/actual-App fixture. Every terminal byte is synthetic.
#[path = "native_chrome/child.rs"]
mod child;
#[path = "native_chrome/frame.rs"]
mod frame;
#[allow(dead_code)]
#[path = "command_routing/host.rs"]
mod host;
#[path = "support/host_viewport.rs"]
mod host_viewport;
#[allow(dead_code)]
#[path = "compatibility/report.rs"]
mod report;
#[path = "native_chrome/scenes.rs"]
mod scenes;
#[path = "native_chrome/window.rs"]
mod window;

use iced::advanced::{
    Layout, layout, mouse,
    renderer::{Headless, Renderer as _, Style},
    widget::Tree,
};
use iced::{Color, Executor, Rectangle, Size};
use pike::{
    app::{App, Control},
    executor::SharedExecutor,
};
use std::{
    fs,
    io::{self},
    path::Path,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args
        .get(1)
        .is_some_and(|arg| arg == "--child" || arg == "--child-exit" || arg == "--child-gated")
    {
        return child::run(
            args.get(1).is_some_and(|arg| arg == "--child-exit"),
            args.get(1).is_some_and(|arg| arg == "--child-gated"),
        )
        .map_err(Into::into);
    }
    let output = Path::new(args.get(1).ok_or("expected fresh output directory")?);
    fs::create_dir(output)?;
    if args.get(2).is_some_and(|s| s == "--window-idle") {
        return window::run(output.to_path_buf()).map_err(Into::into);
    }
    let executor = SharedExecutor::new()?;
    let mut renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(14.0),
            None,
        ))
        .ok_or("native graphics required")?;
    fs::write(output.join("renderer.txt"), renderer.name())?;
    if args.get(2).is_some_and(|s| s == "--components") {
        return components(&mut renderer, output).map_err(Into::into);
    }
    let config = output.join("settings.toml");
    let readiness_only = args.get(2).is_some_and(|s| s == "--readiness");
    fs::write(
        &config,
        scenes::configuration(
            &std::env::current_exe()?.to_string_lossy(),
            if readiness_only {
                "--child-gated"
            } else {
                "--child"
            },
        ),
    )?;
    let (app, task) = executor.enter(|| App::with_config_path(config));
    let mut host = host::Host::new(app, task, renderer, executor);
    let idle_only = args.get(2).is_some_and(|s| s == "--idle");
    let result = if readiness_only {
        scenes::verify_readiness(&mut host, output)
    } else if idle_only {
        scenes::ready(&mut host).and_then(|_| idle(&mut host, output))
    } else {
        scenes::verify(&mut host, output)
    };
    if let Err(error) = &result {
        fs::write(output.join("failure.txt"), error.to_string())?;
    }
    scenes::close(&mut host)?;
    fs::write(
        output.join("cleanup.txt"),
        "All actual App sessions acknowledged native cleanup.\n",
    )?;
    result?;
    drop(host);
    if !idle_only && !readiness_only {
        scenes::error_empty(output)?;
    }
    fs::write(output.join("completed.txt"), "passed\n")?;
    Ok(())
}

fn counters(host: &host::Host) -> [u64; 5] {
    window::counters(&host.app)
}

fn idle(host: &mut host::Host, output: &Path) -> io::Result<()> {
    // This finite driver polls queued native runtime messages at 10ms. Both
    // builds use this identical driver; this is not a production redraw loop.
    let settle = Instant::now();
    host.until(|_| settle.elapsed() >= Duration::from_secs(2))?;
    let before = counters(host);
    let cpu = || {
        std::process::Command::new("/bin/ps")
            .args(["-o", "time=,rss=", "-p", &std::process::id().to_string()])
            .output()
    };
    let initial = cpu()?;
    let start = Instant::now();
    for _ in 0..3 {
        let interval = Instant::now();
        host.until(|_| interval.elapsed() >= Duration::from_secs(8))?;
    }
    let elapsed = start.elapsed().as_secs_f64();
    let final_cpu = cpu()?;
    let after = counters(host);
    let deltas: Vec<_> = after.iter().zip(before).map(|(a, b)| a - b).collect();
    fs::write(
        output.join("idle.txt"),
        format!(
            "platform={} arch={} renderer={}\nsettle_s=2 interval_s={elapsed:.3} externally_requested_redraws=0\nps_before_time_rss={}ps_after_time_rss={}counter_order=font_measurements,pty_resizes,grid_resizes,cache_invalidations,snapshots\ncounter_delta={deltas:?}\n",
            std::env::consts::OS,
            std::env::consts::ARCH,
            host.renderer.name(),
            String::from_utf8_lossy(&initial.stdout),
            String::from_utf8_lossy(&final_cpu.stdout)
        ),
    )?;
    Ok(())
}

fn components(renderer: &mut iced::Renderer, output: &Path) -> io::Result<()> {
    use iced::advanced::{
        Shell, clipboard,
        widget::{Id, Operation},
    };
    use pike::{
        tab_strip::{self, Item},
        tabs::{Activity, Title},
    };
    #[derive(Default)]
    struct Bounds {
        strip: Option<Rectangle>,
        overflow: Option<Rectangle>,
        ellipses: usize,
    }
    impl Operation for Bounds {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
            if id == Some(&Id::from("tab-strip")) {
                self.strip = Some(bounds);
            }
            if id == Some(&Id::from("tab-overflow-control")) {
                self.overflow = Some(bounds);
            }
        }
        fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
            assert!(
                bounds.x.abs() < 10000.0
                    && bounds.y.abs() < 10000.0
                    && bounds.width < 10000.0
                    && bounds.height < 10000.0,
                "nonfinite component text layout: {bounds:?} controlled_label={text}"
            );
            if text.ends_with('…') {
                assert!(bounds.width <= 134.01);
                self.ellipses += 1;
            }
        }
    }
    let mut model = pike::workspace::Model::default();
    for _ in 0..12 {
        model
            .create_tab()
            .map_err(|_| io::Error::other("controlled tab setup"))?;
    }
    for (name, theme) in [("dark", iced::Theme::Dark), ("light", iced::Theme::Light)] {
        for width in [200.0, 320.0, 1200.0] {
            let size = Size::new(width, pike::ui::tokens::TAB_HEIGHT);
            let items = model
                .workspace()
                .order()
                .iter()
                .map(|id| Item {
                    id: *id,
                    title: Title::new(if id.get().is_multiple_of(2) {
                        "界界界界界界界界界界界界界界界界界界界界界界界界界界界界"
                    } else {
                        "Controlled long tab name with Unicode and truncation"
                    })
                    .unwrap(),
                    activity: Activity::default(),
                })
                .collect();
            let mut view = tab_strip::view(
                items,
                model.workspace().active(),
                Control::ActivateTab,
                Control::CloseTab,
                Control::TabAction,
            );
            let mut tree = Tree::new(view.as_widget());
            let node =
                view.as_widget_mut()
                    .layout(&mut tree, renderer, &layout::Limits::new(size, size));
            let mut bounds = Bounds::default();
            view.as_widget_mut()
                .operate(&mut tree, Layout::new(&node), renderer, &mut bounds);
            let strip = bounds
                .strip
                .ok_or_else(|| io::Error::other("missing actual strip"))?;
            let overflow = bounds
                .overflow
                .ok_or_else(|| io::Error::other("missing overflow"))?;
            assert_eq!(strip.height, 34.0);
            assert_eq!(bounds.ellipses, if width < 600.0 { 1 } else { 5 });
            assert_eq!(overflow.width, 26.0);
            assert_eq!(overflow.height, 26.0);
            assert_eq!(
                Rectangle::with_size(size).intersection(&overflow),
                Some(overflow)
            );
            let mut messages = Vec::new();
            for event in [
                iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),
                iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left),
            ] {
                view.as_widget_mut().update(
                    &mut tree,
                    &iced::Event::Mouse(event),
                    Layout::new(&node),
                    mouse::Cursor::Available(overflow.center()),
                    renderer,
                    &mut clipboard::Null,
                    &mut Shell::new(&mut messages),
                    &Rectangle::with_size(size),
                );
            }
            assert!(
                messages
                    .iter()
                    .any(|m| matches!(m, Control::TabAction(tab_strip::TabAction::OpenOverflow)))
            );
            for _ in 0..2 {
                let mut messages = Vec::new();
                let mut shell = Shell::new(&mut messages);
                view.as_widget_mut().update(
                    &mut tree,
                    &iced::Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
                    Layout::new(&node),
                    mouse::Cursor::Unavailable,
                    renderer,
                    &mut clipboard::Null,
                    &mut shell,
                    &Rectangle::with_size(size),
                );
                assert_eq!(shell.redraw_request(), iced::window::RedrawRequest::Wait);
            }
            for scale in [1.0, 1.25, 1.5, 2.0] {
                renderer.reset(Rectangle::with_size(size));
                view.as_widget().draw(
                    &tree,
                    renderer,
                    &theme,
                    &Style::default(),
                    Layout::new(&node),
                    mouse::Cursor::Unavailable,
                    &Rectangle::with_size(size),
                );
                let dimensions =
                    Size::new((size.width * scale) as u32, (size.height * scale) as u32);
                let rgba = Headless::screenshot(renderer, dimensions, scale, Color::BLACK);
                report::screenshot(
                    &output.join(format!(
                        "tabs-{name}-{width}-{}.ppm",
                        (scale * 100.0) as u32
                    )),
                    iced::window::Screenshot::new(rgba, dimensions, scale),
                )?;
            }
        }
    }
    fs::write(
        output.join("completed.txt"),
        "Actual tab strip: 34 DIP; overflow 26x26 DIP and real hit-tested at 320/1200 DIP in dark/light at 100/125/150/200%.\n",
    )?;
    Ok(())
}
