//! Controlled command input must not let Windows cooked ReadConsole emit a
//! newline/scroll between state-only output sequences. This is fixture-only.
use std::io::{self, Read};
use std::path::Path;

#[cfg(windows)]
pub fn configure(diagnostics: Option<&Path>) -> io::Result<()> {
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_INPUT, GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE,
        SetConsoleMode,
    };
    let mut before = 0;
    let mut after = 0;
    // SAFETY: Borrow the controlled child's inherited input handle, without
    // closing it. Both mode pointers refer to writable local u32 values.
    // Raw VT input disables cooked line/echo/processed handling in this child.
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        if GetConsoleMode(handle, &mut before) == 0
            || SetConsoleMode(handle, ENABLE_VIRTUAL_TERMINAL_INPUT) == 0
            || GetConsoleMode(handle, &mut after) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    if after != ENABLE_VIRTUAL_TERMINAL_INPUT {
        return Err(io::Error::other("fixture did not enter raw VT input mode"));
    }
    if let Some(output) = diagnostics {
        std::fs::write(
            output.join("fixture-input.txt"),
            format!("before={before} after={after}\n"),
        )?;
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn configure(diagnostics: Option<&Path>) -> io::Result<()> {
    if !std::process::Command::new("/bin/stty")
        .arg("-echo")
        .status()?
        .success()
    {
        return Err(io::Error::other("could not disable synthetic command echo"));
    }
    if let Some(output) = diagnostics {
        std::fs::write(
            output.join("fixture-input.txt"),
            "Unix canonical input, echo disabled\n",
        )?;
    }
    Ok(())
}

/// CR on Windows raw input and LF on Unix canonical input frame fixed ASCII
/// commands. Never rely on cooked ReadConsole to produce an extra LF.
pub fn command(input: &mut impl Read) -> io::Result<Option<String>> {
    let mut line = Vec::with_capacity(32);
    loop {
        let mut byte = [0];
        match input.read(&mut byte) {
            Ok(0) if line.is_empty() => return Ok(None),
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "partial fixture command",
                ));
            }
            Ok(_) => match byte[0] {
                b'\r' | b'\n' if !line.is_empty() => {
                    return Ok(Some(String::from_utf8(line).map_err(io::Error::other)?));
                }
                b'\r' | b'\n' => {}
                value if value.is_ascii() && line.len() < 64 => line.push(value),
                _ => {
                    return Err(io::Error::other(
                        "fixture command exceeds bounded ASCII vocabulary",
                    ));
                }
            },
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_commands_accept_cr_lf_and_crlf_without_cooked_newlines() {
        let mut bytes = &b"1 2 1 0\rstate hide\nstate beam\r\n"[..];
        assert_eq!(command(&mut bytes).unwrap().as_deref(), Some("1 2 1 0"));
        assert_eq!(command(&mut bytes).unwrap().as_deref(), Some("state hide"));
        assert_eq!(command(&mut bytes).unwrap().as_deref(), Some("state beam"));
        assert_eq!(command(&mut bytes).unwrap(), None);
    }
    #[test]
    fn truncated_oversized_and_non_ascii_commands_are_rejected() {
        assert_eq!(
            command(&mut &b"state hide"[..]).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        assert!(command(&mut &[b'x'; 65][..]).is_err());
        assert!(command(&mut &[255, b'\r'][..]).is_err());
    }
}
