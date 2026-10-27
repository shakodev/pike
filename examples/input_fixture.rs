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
    let keyboard = args.next();
    if args.next().is_some()
        || keyboard
            .as_deref()
            .is_some_and(|mode| !["keyboard-normal", "keyboard-app"].contains(&mode))
    {
        return Err(io::Error::other("invalid keyboard fixture mode"));
    }
    raw_console(keyboard.is_some())?;
    fs::create_dir_all(&directory)?;
    if let Some(mode) = keyboard {
        let mut output = io::stdout().lock();
        write!(
            output,
            "\x1b[?1{}\x1b[?u\x1b[>31u\x1b]2;pike:keys-ready\x07",
            if mode == "keyboard-app" { 'h' } else { 'l' }
        )?;
        output.flush()?;
    }
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
fn raw_console(virtual_terminal: bool) -> io::Result<()> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
        GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleCP,
        SetConsoleMode,
    };
    // SAFETY: Borrow the process's standard console handle; never close it.
    // Line buffering, echo and processed input stay disabled in this isolated
    // fixture. Keyboard tests also enable VT input so navigation keys are read
    // as sequences. UTF-8 makes ReadConsoleW's Rust conversion stable. The mode
    // output pointer below is valid local storage, read only after success.
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        if handle.is_null()
            || handle == INVALID_HANDLE_VALUE
            || SetConsoleMode(
                handle,
                if virtual_terminal {
                    ENABLE_VIRTUAL_TERMINAL_INPUT
                } else {
                    0
                },
            ) == 0
            || SetConsoleCP(65001) == 0
        {
            return Err(io::Error::last_os_error());
        }
        if virtual_terminal {
            let output = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut mode = 0;
            if output.is_null()
                || output == INVALID_HANDLE_VALUE
                || GetConsoleMode(output, &mut mode) == 0
                || SetConsoleMode(
                    output,
                    mode | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING,
                ) == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn raw_console(_virtual_terminal: bool) -> io::Result<()> {
    // Test child only. Configure its inherited PTY without another PTY crate
    // or any production terminal-mode change. Existing Windows path is above.
    let status = std::process::Command::new("/bin/stty")
        .args(["raw", "-echo"])
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            "could not configure synthetic child raw input",
        ))
    }
}
