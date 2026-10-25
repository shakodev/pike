//! Controlled raw-console byte reader, or a child that never reads input.
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Input fixture failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let mode = args
        .next()
        .ok_or_else(|| io::Error::other("expected mode"))?;
    let count: usize = args
        .next()
        .ok_or_else(|| io::Error::other("expected byte count"))?
        .parse()
        .map_err(io::Error::other)?;
    if count > 2 * 1024 * 1024 || !["echo", "stall"].contains(&mode.as_str()) {
        return Err(io::Error::other("invalid controlled fixture parameters"));
    }
    let directory = PathBuf::from(
        args.next()
            .ok_or_else(|| io::Error::other("expected directory"))?,
    );
    raw_console()?;
    fs::create_dir_all(&directory)?;
    fs::write(directory.join("ready"), std::process::id().to_string())?;
    if mode == "stall" {
        loop {
            std::thread::park();
        }
    }
    let mut input = io::stdin().lock();
    let mut bytes = 0;
    let mut hash = 0xcbf29ce484222325_u64;
    let mut buffer = [0; 4096];
    while bytes < count {
        let read = input.read(&mut buffer[..(count - bytes).min(4096)])?;
        if read == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        for byte in &buffer[..read] {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
        bytes += read;
    }
    // Only numeric metadata leaves the fixture, never a terminal transcript.
    let mut report = fs::File::create(directory.join("result"))?;
    writeln!(report, "{bytes},{hash}")?;
    report.flush()?;
    fs::write(directory.join("done"), b"done")?;
    loop {
        let extra = input.read(&mut buffer)?;
        if extra == 0 {
            return Ok(());
        }
        writeln!(report, "extra,{extra}")?;
        report.flush()?;
    }
}

#[cfg(windows)]
fn raw_console() -> io::Result<()> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_INPUT_HANDLE, SetConsoleCP, SetConsoleMode,
    };
    // SAFETY: Borrow the process's standard console handle; never close it.
    // A zero mode disables line buffering, echo and processed input in this
    // isolated fixture only. UTF-8 makes ReadConsoleW's Rust conversion stable.
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        if handle.is_null()
            || handle == INVALID_HANDLE_VALUE
            || SetConsoleMode(handle, 0) == 0
            || SetConsoleCP(65001) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn raw_console() -> io::Result<()> {
    Err(io::Error::other("input fixture requires Windows"))
}
