//! Standalone identical release/native before/after driver, independent of UI
//! modules so it also compiles against archived actual-main source.
#[path = "native_chrome/child.rs"]
mod child;
#[path = "native_chrome/window.rs"]
mod window;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "--child") {
        return child::run(false).map_err(Into::into);
    }
    let output = std::path::absolute(args.get(1).ok_or("expected fresh output directory")?)?;
    std::fs::create_dir(&output)?;
    window::run(output)?;
    Ok(())
}
