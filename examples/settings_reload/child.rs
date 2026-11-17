use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
    time::{Duration, Instant},
};

#[path = "../support/cursor_input.rs"]
#[allow(dead_code)]
mod input;

pub fn run(output: &Path, argument: &str) -> io::Result<()> {
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
        return Err(io::Error::other("raw synthetic input failed"));
    }
    // Only the two fixture-owned overrides are read; never dump an environment.
    let identity = format!(
        "pid={pid}\ncwd={}\nprofile={}\nrevision={}\nargument={argument}\n",
        std::env::current_dir()?.display(),
        std::env::var("PIKE31_PROFILE").map_err(io::Error::other)?,
        std::env::var("PIKE31_REVISION").map_err(io::Error::other)?
    );
    fs::write(directory.join("identity.txt"), &identity)?;
    let input_directory = directory.clone();
    std::thread::spawn(move || -> io::Result<()> {
        let mut bytes = [0; 128];
        let mut all = Vec::new();
        loop {
            let count = io::stdin().read(&mut bytes)?;
            if count == 0 {
                return Ok(());
            }
            if all.len() + count > 4096 {
                return Err(io::Error::other("synthetic input bound"));
            }
            all.extend_from_slice(&bytes[..count]);
            fs::write(input_directory.join("input.bin"), &all)?;
        }
    });
    history(pid);
    fs::write(directory.join("ready.txt"), "ready")?;
    let start = Instant::now();
    let mut palette = false;
    let mut flood = false;
    let mut alive = false;
    loop {
        if !palette && directory.join("palette.go").exists() {
            print!("\x1b]10;?\x07\x1b]11;?\x07");
            io::stdout().flush()?;
            palette = true;
        }
        if !flood && directory.join("history.go").exists() {
            history(pid);
            fs::write(directory.join("history.done"), "sent")?;
            flood = true;
        }
        if !alive && directory.join("alive.go").exists() {
            fs::write(directory.join("alive.txt"), &identity)?;
            alive = true;
        }
        if directory.join("exit.go").exists() {
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(120) {
            return Err(io::Error::other("synthetic child deadline"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn history(pid: u32) {
    for line in 0..300 {
        print!("PIKE31_SYNTHETIC_{pid}_{line}\r\n");
    }
    print!("\x1b[2J\x1b[HPIKE31_PID={pid}\x1b[?1h\x1b[?2004h");
    io::stdout().flush().expect("synthetic output");
}
