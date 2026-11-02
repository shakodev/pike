//! Finite native-renderer/actual-App fixture. Every terminal byte is synthetic.
#[path = "native_chrome/child.rs"]
mod child;
#[path = "native_chrome/components.rs"]
mod components;
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

use iced::Executor;
use iced::advanced::renderer::Headless;
use pike::{app::App, executor::SharedExecutor};
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
    if args.get(2).is_some_and(|s| s == "--activity-overflow") {
        return components::verify_overflow(&mut renderer, output).map_err(Into::into);
    }
    if args
        .get(2)
        .is_some_and(|s| s == "--components" || s == "--activity")
    {
        return components::verify(
            &mut renderer,
            output,
            args.get(2).is_some_and(|s| s == "--activity"),
        )
        .map_err(Into::into);
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
