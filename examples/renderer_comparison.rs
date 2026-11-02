//! Finite, synthetic actual-App/native-window renderer comparison. No product hooks.
#[path = "renderer_comparison/child.rs"]
mod child;
#[allow(dead_code)]
#[path = "support/cursor_input.rs"]
mod fixture_input;
#[path = "renderer_comparison/options.rs"]
mod options;
#[path = "renderer_comparison/pixels.rs"]
mod pixels;
#[path = "renderer_comparison/probe.rs"]
mod probe;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().nth(1).is_some_and(|s| s == "--child") {
        return child::run().map_err(Into::into);
    }
    let options = options::Options::parse(std::env::args().skip(1))?;
    std::fs::create_dir(&options.output)?;
    probe::run(options)?;
    Ok(())
}
