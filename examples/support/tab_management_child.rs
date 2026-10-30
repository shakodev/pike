//! Controlled synthetic child for the finite R30 native App fixture.
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

#[path = "cursor_input.rs"]
#[allow(dead_code)]
mod input;

pub fn run(output: &Path) -> io::Result<()> {
    let pid = std::process::id();
    let directory = output.join(pid.to_string());
    fs::create_dir(&directory)?;
    input::configure(Some(&directory))?;
    #[cfg(unix)]
    if !std::process::Command::new("/bin/stty")
        .args(["raw", "-echo"])
        .status()?
        .success()
    {
        return Err(io::Error::other("synthetic child raw input failed"));
    }
    fs::write(directory.join("pid.txt"), pid.to_string())?;
    let input_directory = directory.clone();
    std::thread::spawn(move || -> io::Result<()> {
        let mut all = Vec::new();
        let mut bytes = [0; 128];
        loop {
            let count = io::stdin().read(&mut bytes)?;
            if count == 0 {
                return Ok(());
            }
            if all.len() + count > 2048 {
                return Err(io::Error::other("synthetic input exceeded bound"));
            }
            all.extend_from_slice(&bytes[..count]);
            fs::write(input_directory.join("input.bin"), &all)?;
        }
    });
    for line in 0..100 {
        print!("PIKE30_HISTORY_{pid}_{line}\r\n");
    }
    // Save the initial absent title, enabling a real ResetTitle via title pop
    // later without resetting the screen, history, modes or process.
    print!("\x1b[22;0t\x1b]2;Fixture {pid}\x07\x1b[2J\x1b[HPIKE30_PID={pid}\x1b[?1h\x1b[?2004h");
    io::stdout().flush()?;
    fs::write(directory.join("ready.txt"), "ready")?;
    let start = Instant::now();
    let mut changed = false;
    let mut reset = false;
    loop {
        if !changed && directory.join("title.go").exists() {
            // Only fixture-generated text is emitted or retained by this tool.
            print!("\x1b]2;OSC 日本語 🦀\u{202e}{}\x07", "界".repeat(300));
            io::stdout().flush()?;
            fs::write(directory.join("title.done"), "sent")?;
            changed = true;
        }
        if !reset && directory.join("reset.go").exists() {
            print!("\x1b[23;0t");
            io::stdout().flush()?;
            fs::write(directory.join("reset.done"), "sent")?;
            reset = true;
        }
        if directory.join("exit.go").exists() {
            print!("\x1b[2;1HPIKE30_FINAL_{pid}");
            io::stdout().flush()?;
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(90) {
            return Err(io::Error::other("synthetic child deadline"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
