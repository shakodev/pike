//! One-shot Windows launch preparation. No process-global cwd/environment edits.
use std::cmp::Ordering;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, ErrorKind};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

use windows_sys::Win32::Globalization::CompareStringOrdinal;
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

use super::win32_string;
use crate::tty::Options;

/// Safe to display or debug: never contains paths, arguments or environment data.
/// Available through the inner error of the existing `io::Result` API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchError {
    InvalidProgram,
    ProgramNotFound,
    InvalidArgument,
    CommandLineTooLong,
    InvalidDirectory,
    InvalidEnvironment,
    DuplicateEnvironmentKey,
    ProcessCreation { code: Option<i32> },
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidProgram => "invalid executable; provide an unquoted .exe path or name",
            Self::ProgramNotFound => "requested executable was not found",
            Self::InvalidArgument => "argument contains a null character",
            Self::CommandLineTooLong => "Windows command line exceeds the UTF-16 limit",
            Self::InvalidDirectory => "launch directory is not an accessible directory",
            Self::InvalidEnvironment => "invalid environment key or value",
            Self::DuplicateEnvironmentKey => {
                "environment overrides contain case-insensitive duplicate keys"
            },
            Self::ProcessCreation { .. } => "Windows process creation failed",
        };
        f.write_str(message)?;
        if let Self::ProcessCreation { code: Some(code) } = self {
            write!(f, " (OS code {code})")?;
        }
        Ok(())
    }
}

impl std::error::Error for LaunchError {}

impl From<LaunchError> for io::Error {
    fn from(error: LaunchError) -> Self {
        let kind = match error {
            LaunchError::ProgramNotFound => ErrorKind::NotFound,
            LaunchError::ProcessCreation { code: Some(code) } => {
                io::Error::from_raw_os_error(code).kind()
            },
            LaunchError::ProcessCreation { code: None } => ErrorKind::Other,
            _ => ErrorKind::InvalidInput,
        };
        Self::new(kind, error)
    }
}

pub(super) struct Prepared {
    pub program: Vec<u16>,
    pub command: Vec<u16>,
    pub directory: Vec<u16>,
    pub environment: Option<Vec<u16>>,
}

type Environment = Vec<(Vec<u16>, Vec<u16>)>;

impl Prepared {
    pub fn new(config: &Options) -> io::Result<Self> {
        let parent = std::env::current_dir().map_err(|_| LaunchError::InvalidDirectory)?;
        let directory =
            config.working_directory.as_ref().map_or_else(|| parent.clone(), |p| parent.join(p));
        if directory.as_os_str().encode_wide().any(|c| c == 0) {
            return Err(LaunchError::InvalidDirectory.into());
        }
        // Capture even drive-relative cwd once as a full path before resolution
        // or CreateProcess. Do not pass per-drive ambient state to the child.
        let directory =
            std::path::absolute(directory).map_err(|_| LaunchError::InvalidDirectory)?;
        if !directory.is_dir() {
            return Err(LaunchError::InvalidDirectory.into());
        }
        let env = environment(&config.env, std::env::vars_os())?;
        let program = match &config.shell {
            Some(shell) => {
                resolve(&shell.program, &directory, &env)?.ok_or(LaunchError::ProgramNotFound)?
            },
            None => default_shell(&directory, &env)?.ok_or(LaunchError::ProgramNotFound)?,
        };
        // argv[0] follows the special executable-token rules, not CRT argument
        // escaping. A Windows file name cannot contain a double quote.
        let mut command = vec![b'"' as u16];
        command.extend(program.as_os_str().encode_wide());
        command.push(b'"' as u16);
        if let Some(shell) = &config.shell {
            for arg in &shell.args {
                if arg.contains('\0') {
                    return Err(LaunchError::InvalidArgument.into());
                }
                if arg.encode_utf16().count() >= 32767 {
                    return Err(LaunchError::CommandLineTooLong.into());
                }
                command.push(b' ' as u16);
                if config.escape_args {
                    let mut escaped = String::new();
                    push_escaped_arg(&mut escaped, arg);
                    command.extend(escaped.encode_utf16());
                } else {
                    command.extend(arg.encode_utf16());
                }
                if command.len() >= 32767 {
                    return Err(LaunchError::CommandLineTooLong.into());
                }
            }
        }
        if command.len() >= 32767 {
            return Err(LaunchError::CommandLineTooLong.into());
        }
        command.push(0);
        Ok(Self {
            program: win32_string(&program),
            command,
            directory: win32_string(&directory),
            environment: if config.env.is_empty() { None } else { Some(environment_block(&env)) },
        })
    }
}

// The OS uppercase table gives Windows ordinal, case-insensitive semantics,
// including non-ASCII keys. Inputs are bounded below before comparison.
fn compare(left: &[u16], right: &[u16]) -> Ordering {
    // SAFETY: live UTF-16 slices; explicit lengths fit i32, TRUE is exactly 1.
    match unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            left.len() as i32,
            right.as_ptr(),
            right.len() as i32,
            1,
        )
    } {
        1 => Ordering::Less,
        2 => Ordering::Equal,
        3 => Ordering::Greater,
        _ => unreachable!("validated UTF-16 slice lengths"),
    }
}

fn environment(
    overrides: &HashMap<String, String>,
    inherited: impl Iterator<Item = (OsString, OsString)>,
) -> io::Result<Environment> {
    let mut result: Environment = Vec::with_capacity(overrides.len());
    for (key, value) in overrides {
        if key.is_empty()
            || key.contains(['=', '\0'])
            || value.contains('\0')
            || key.encode_utf16().count() > 32767
        {
            return Err(LaunchError::InvalidEnvironment.into());
        }
        result.push((key.encode_utf16().collect(), value.encode_utf16().collect()));
    }
    result.sort_unstable_by(|a, b| compare(&a.0, &b.0));
    if result.windows(2).any(|pair| compare(&pair[0].0, &pair[1].0).is_eq()) {
        return Err(LaunchError::DuplicateEnvironmentKey.into());
    }
    for (key, value) in inherited {
        let key: Vec<_> = key.encode_wide().collect();
        // Preserve inherited =C: drive-directory entries and non-Unicode data.
        if key.len() > 32767 {
            return Err(LaunchError::InvalidEnvironment.into());
        }
        if let Err(index) = result.binary_search_by(|entry| compare(&entry.0, &key)) {
            result.insert(index, (key, value.encode_wide().collect()));
        }
    }
    Ok(result)
}

fn environment_block(env: &Environment) -> Vec<u16> {
    let mut block = Vec::new();
    for (key, value) in env {
        block.extend(key);
        block.push(b'=' as u16);
        block.extend(value);
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

fn value(env: &Environment, name: &str) -> Option<OsString> {
    let key: Vec<_> = name.encode_utf16().collect();
    env.binary_search_by(|entry| compare(&entry.0, &key))
        .ok()
        .map(|i| OsString::from_wide(&env[i].1))
}

fn system_directory() -> io::Result<PathBuf> {
    let mut buffer = vec![0; 32768];
    // SAFETY: writable buffer, capacity expressed in UTF-16 units.
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(LaunchError::ProgramNotFound.into());
    }
    Ok(PathBuf::from(OsString::from_wide(&buffer[..length])))
}

fn resolve(program: &str, cwd: &Path, env: &Environment) -> io::Result<Option<PathBuf>> {
    if program.is_empty() || program.contains(['"', '\0']) {
        return Err(LaunchError::InvalidProgram.into());
    }
    let mut path = PathBuf::from(program);
    if drive_relative(&path) {
        return Err(LaunchError::InvalidProgram.into());
    }
    if path.extension().is_none() {
        path.set_extension("exe");
    }
    // Batch scripts require explicit cmd /C semantics, never an implicit shell.
    if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe")) {
        return Err(LaunchError::InvalidProgram.into());
    }
    if path.components().count() > 1 || path.is_absolute() {
        return Ok(executable(cwd.join(path)));
    }
    // Bare names search explicit session PATH, then the OS system directory.
    // Never implicitly search the current directory or wrap via cmd.exe.
    if let Some(search) = value(env, "PATH") {
        for dir in std::env::split_paths(&search).filter(|p| !p.as_os_str().is_empty()) {
            if drive_relative(&dir) {
                return Err(LaunchError::InvalidEnvironment.into());
            }
            let candidate = cwd.join(dir).join(&path);
            if let Some(found) = executable(candidate) {
                return Ok(Some(found));
            }
        }
    }
    Ok(executable(system_directory()?.join(path)))
}

// C:folder discards the base on join and uses the parent's per-drive cwd.
// Require an unambiguous path when resolving an executable inside a session.
fn drive_relative(path: &Path) -> bool {
    !path.has_root() && matches!(path.components().next(), Some(Component::Prefix(_)))
}

// Keep conventional absolute paths for hosts such as Windows PowerShell/.NET
// Framework that cannot use the verbatim paths produced by canonicalize.
// CreateProcess receives the full path separately, so no search is deferred.
fn executable(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then(|| std::path::absolute(path).ok()).flatten()
}

fn default_shell(cwd: &Path, env: &Environment) -> io::Result<Option<PathBuf>> {
    if let Some(found) = resolve("pwsh.exe", cwd, env)? {
        return Ok(Some(found));
    }
    if let Some(program_files) = value(env, "ProgramFiles") {
        let candidate = PathBuf::from(program_files).join("PowerShell/7/pwsh.exe");
        if drive_relative(&candidate) {
            return Err(LaunchError::InvalidEnvironment.into());
        }
        if let Some(found) = executable(cwd.join(candidate)) {
            return Ok(Some(found));
        }
    }
    let system = system_directory()?;
    Ok([system.join("WindowsPowerShell/v1.0/powershell.exe"), system.join("cmd.exe")]
        .into_iter()
        .find(|p| p.is_file()))
}

// CRT argument encoding, adapted from the already-vendored Alacritty helper.
fn push_escaped_arg(cmd: &mut String, arg: &str) {
    let arg_bytes = arg.as_bytes();
    let quote = arg_bytes.iter().any(|c| *c == b' ' || *c == b'\t') || arg_bytes.is_empty();
    if quote {
        cmd.push('"');
    }

    let mut backslashes: usize = 0;
    for x in arg.chars() {
        if x == '\\' {
            backslashes += 1;
        } else {
            if x == '"' {
                // Add n+1 backslashes to total 2n+1 before internal '"'.
                cmd.extend((0..=backslashes).map(|_| '\\'));
            }
            backslashes = 0;
        }
        cmd.push(x);
    }

    if quote {
        // Add n backslashes to total 2n before ending '"'.
        cmd.extend((0..backslashes).map(|_| '\\'));
        cmd.push('"');
    }
}

#[cfg(test)]
mod tests;
