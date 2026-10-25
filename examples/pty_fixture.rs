//! Synthetic console child. It owns no PTY; iced_term/Alacritty supplies ConPTY.
use std::io::{self, BufRead, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("Pike fixture failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<u8> {
    let id: u8 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .filter(|id| (1..=2).contains(id))
        .ok_or_else(|| io::Error::other("expected fixture session 1 or 2"))?;
    let mut output = io::stdout().lock();
    writeln!(output, "\x1b[{}mPIKE READY session={id}\x1b[0m", 31 + id)?;
    write!(output, "\x1b]2;pike:ready:{id}:{}\x07", std::process::id())?;
    // Titles are replaceable console state. Wait for a request before sending
    // dimensions, so ConPTY cannot coalesce the ready title with a size title.
    output.flush()?;
    let mut input = io::stdin().lock();
    loop {
        let mut line = Vec::with_capacity(65);
        let count = (&mut input).take(65).read_until(b'\n', &mut line)?;
        if count == 0 {
            return Ok(0);
        }
        if count > 64 {
            return Err(io::Error::other("fixture command exceeds 64 bytes"));
        }
        let command = line.strip_suffix(b"\n").unwrap_or(&line);
        let command = command.strip_suffix(b"\r").unwrap_or(command);
        match command {
            b"size" => report_size(&mut output, id)?,
            b"render" => writeln!(
                output,
                "SESSION {id} | wide: \u{754c} | combining: e\u{301} | \x1b[7mreverse\x1b[0m"
            )?,
            b"probe" => write!(output, "\x1b[18t\x1b]10;?\x07\x07\x1b]2;\x07")?,
            b"exit" => {
                for line in 0..64 {
                    writeln!(output, "PIKE FINAL OUTPUT session={id} line={line}")?;
                }
                write!(output, "\x1b]2;pike:final:{id}\x07")?;
                output.flush()?;
                return Ok(40 + id);
            }
            _ => writeln!(output, "PIKE fixture commands: size, render, probe, exit")?,
        }
        output.flush()?;
    }
}

fn report_size(output: &mut impl Write, id: u8) -> io::Result<()> {
    let (cols, rows) = console_size()?;
    writeln!(output, "PIKE SIZE session={id} cols={cols} rows={rows}")?;
    write!(output, "\x1b]2;pike:size:{id}:{cols}:{rows}\x07")
}

#[cfg(windows)]
fn console_size() -> io::Result<(u16, u16)> {
    use std::mem::MaybeUninit;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        GetConsoleScreenBufferInfo, GetStdHandle, STD_OUTPUT_HANDLE,
    };
    // SAFETY: GetStdHandle takes a valid constant. This is a borrowed process
    // handle; it must not be closed. The output pointer is valid storage, read
    // only after the API reports successful initialization. No PTY is created.
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::other("fixture stdout is not a console"));
        }
        let mut info = MaybeUninit::uninit();
        if GetConsoleScreenBufferInfo(handle, info.as_mut_ptr()) == 0 {
            return Err(io::Error::last_os_error());
        }
        let rect = info.assume_init().srWindow;
        Ok((
            (rect.Right - rect.Left + 1) as u16,
            (rect.Bottom - rect.Top + 1) as u16,
        ))
    }
}

#[cfg(not(windows))]
fn console_size() -> io::Result<(u16, u16)> {
    Err(io::Error::other(
        "the console-size fixture requires Windows",
    ))
}
