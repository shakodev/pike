//! Synthetic launch regression. Reports assertions only, never launch payloads.
#[cfg(windows)]
#[path = "support/launch_validation.rs"]
mod validation;

#[cfg(windows)]
fn main() -> std::io::Result<()> {
    validation::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("launch_validation requires Windows ConPTY");
}
