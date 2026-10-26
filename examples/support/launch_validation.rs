use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced_term::settings::{BackendSettings, Settings};
use iced_term::{BackendCommand, Command, ExitReason, LaunchError, Terminal};

const TIMEOUT: Duration = Duration::from_secs(20);
const ENV_KEY: &str = "PIKE_LAUNCH_FIXTURE";

pub fn run() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|s| s == "--fixture") {
        return fixture(&args[1..]);
    }
    let directory =
        std::env::temp_dir().join(format!("pike-launch-{} 雪's space", std::process::id()));
    fs::create_dir(&directory)?;
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let executable = directory.join("fixture 雪 space.exe");
    fs::copy(std::env::current_exe()?, &executable)?;
    // Only the parent loads ConPTY; the copied child just dumps synthetic data.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let inherited = std::env::var_os("SystemRoot");
    let parent_cwd = std::env::current_dir()?;
    let parent_value = std::env::var_os(ENV_KEY);
    let payload: Vec<String> = [
        "",
        "a b",
        "\t",
        "quote\"inside",
        "\\",
        "trailing space \\",
        "backslash\\\"quote",
        "雪🦀",
        "&|<>^!%PIKE_LAUNCH_FIXTURE%",
        "'single'",
        "\r\n",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let mut sessions = Vec::new();
    for id in 0..2 {
        let cwd = directory.join(format!("session {id}"));
        fs::create_dir(&cwd)?;
        let output = directory.join(format!("result-{id}"));
        let value = format!("synthetic-{id}");
        let mut args = vec!["--fixture".into(), string(&output)?];
        args.extend(payload.clone());
        let settings = BackendSettings {
            program: string(&executable)?,
            args,
            working_directory: Some(cwd.clone()),
            env: [
                (ENV_KEY.to_ascii_lowercase(), value.clone()),
                ("PIKE_EMPTY".into(), "".into()),
            ]
            .into(),
            ..Default::default()
        };
        sessions.push((
            Terminal::new(
                id,
                Settings {
                    backend: settings,
                    ..Default::default()
                },
            )?,
            output,
            cwd,
            value,
        ));
    }
    for (terminal, output, cwd, value) in &mut sessions {
        wait_file(output)?;
        let observed = fs::read(output)?;
        let mut expected = Vec::new();
        encode(&mut expected, cwd.as_os_str());
        encode(&mut expected, &OsString::from(value.as_str()));
        encode(&mut expected, &OsString::new());
        encode(&mut expected, inherited.as_deref().unwrap_or_default());
        for arg in &payload {
            encode(&mut expected, std::ffi::OsStr::new(arg));
        }
        ensure(observed == expected, "argument/cwd/environment round trip")?;
        terminal.handle(Command::ProxyToBackend(BackendCommand::Write(
            b"exit\r".to_vec(),
        )));
    }
    for (terminal, _, _, _) in sessions {
        finish(&runtime, &terminal, 0)?;
    }
    ensure(
        std::env::current_dir()? == parent_cwd && std::env::var_os(ENV_KEY) == parent_value,
        "parent settings unchanged",
    )?;
    println!("PASS: argv round trip and two independent live sessions");

    for (program, cwd, expected) in [
        (
            "pike_missing_PRIVATE_SENTINEL.exe",
            None,
            LaunchError::ProgramNotFound,
        ),
        (
            "cmd.exe",
            Some(directory.join("missing_PRIVATE_SENTINEL")),
            LaunchError::InvalidDirectory,
        ),
    ] {
        let settings = BackendSettings {
            program: program.into(),
            working_directory: cwd,
            ..Default::default()
        };
        let error = Terminal::new(
            100,
            Settings {
                backend: settings,
                ..Default::default()
            },
        )
        .err()
        .ok_or_else(|| io::Error::other("invalid launch accepted"))?;
        ensure(
            error
                .get_ref()
                .and_then(|e| e.downcast_ref::<LaunchError>())
                == Some(&expected),
            "structured launch error",
        )?;
        ensure(
            !format!("{error} {error:?}").contains("PRIVATE_SENTINEL"),
            "private launch diagnostic",
        )?;
    }
    let private = BackendSettings {
        program: "PRIVATE_SENTINEL".into(),
        args: vec!["PRIVATE_SENTINEL".into()],
        env: [("PRIVATE_SENTINEL".into(), "PRIVATE_SENTINEL".into())].into(),
        ..Default::default()
    };
    ensure(
        !format!("{private:?}").contains("PRIVATE_SENTINEL"),
        "private configuration debug",
    )?;
    println!("PASS: structured errors and private diagnostics");

    let cmd_output = directory.join("cmd result.txt");
    let cmd = Terminal::new(
        200,
        Settings {
            backend: BackendSettings {
                program: "cmd.exe".into(),
                args: vec![
                    "/D".into(),
                    "/V:OFF".into(),
                    "/S".into(),
                    "/C".into(),
                    format!(
                        "\"echo cmd-semantic>\"{}\" & exit /b 19\"",
                        cmd_output.display()
                    ),
                ],
                escape_args: false,
                ..Default::default()
            },
            ..Default::default()
        },
    )?;
    finish(&runtime, &cmd, 19)?;
    ensure(
        fs::read_to_string(cmd_output)?.trim() == "cmd-semantic",
        "cmd command semantics",
    )?;
    println!("PASS: cmd explicit raw command semantics");

    // Windows PowerShell is installed on both supported CI images. PowerShell 7
    // is checked when available; CI runners install it in their standard PATH.
    for program in ["powershell.exe", "pwsh.exe"] {
        let output = directory.join(format!("{program}.txt"));
        let script = format!(
            "[IO.File]::WriteAllText('{}', 'powershell-semantic'); exit 23",
            string(&output)?.replace('\'', "''")
        );
        let result = Terminal::new(
            201,
            Settings {
                backend: BackendSettings {
                    program: program.into(),
                    args: vec![
                        "-NoLogo".into(),
                        "-NoProfile".into(),
                        "-NonInteractive".into(),
                        "-Command".into(),
                        script,
                    ],
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let terminal = match result {
            Ok(terminal) => terminal,
            Err(error)
                if program == "pwsh.exe"
                    && error
                        .get_ref()
                        .and_then(|e| e.downcast_ref::<LaunchError>())
                        == Some(&LaunchError::ProgramNotFound) =>
            {
                println!("SKIP: PowerShell 7 is not installed");
                continue;
            }
            Err(error) => return Err(error),
        };
        finish(&runtime, &terminal, 23)?;
        ensure(
            fs::read_to_string(output)? == "powershell-semantic",
            "PowerShell command semantics",
        )?;
        println!("PASS: {program} explicit command semantics");
    }
    let automatic = Terminal::new(202, Settings::default())?;
    automatic.request_close();
    let handle = automatic.shutdown_handle();
    runtime
        .block_on(async { tokio::time::timeout(TIMEOUT, handle.wait()).await })
        .map_err(|_| io::Error::other("default shell cleanup deadline"))?;
    println!("PASS: automatic shell startup and cleanup");
    Ok(())
}

fn string(path: &Path) -> io::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| io::Error::other("fixture path encoding"))
}

fn encode(output: &mut Vec<u8>, value: &std::ffi::OsStr) {
    let units: Vec<_> = value.encode_wide().collect();
    output.extend((units.len() as u32).to_le_bytes());
    for unit in units {
        output.extend(unit.to_le_bytes());
    }
}

fn fixture(args: &[OsString]) -> io::Result<()> {
    let output = args
        .first()
        .ok_or_else(|| io::Error::other("fixture output missing"))?;
    let mut report = Vec::new();
    encode(&mut report, std::env::current_dir()?.as_os_str());
    encode(&mut report, &std::env::var_os(ENV_KEY).unwrap_or_default());
    encode(
        &mut report,
        &std::env::var_os("PIKE_EMPTY").unwrap_or_else(|| "MISSING".into()),
    );
    encode(
        &mut report,
        &std::env::var_os("SystemRoot").unwrap_or_default(),
    );
    for arg in &args[1..] {
        encode(&mut report, arg);
    }
    let temporary = PathBuf::from(output).with_extension("pending");
    fs::write(&temporary, report)?;
    fs::rename(temporary, output)?;
    // Keep both sessions alive during isolation checks, without a polling task.
    let mut byte = [0];
    while io::stdin().read(&mut byte)? != 0 {
        if byte[0] == b'\n' {
            break;
        }
    }
    Ok(())
}

fn wait_file(path: &Path) -> io::Result<()> {
    let start = Instant::now();
    while !path.is_file() {
        if start.elapsed() > TIMEOUT {
            return Err(io::Error::other("fixture readiness deadline"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

fn finish(runtime: &tokio::runtime::Runtime, terminal: &Terminal, code: i32) -> io::Result<()> {
    let handle = terminal.shutdown_handle();
    let result = runtime
        .block_on(async { tokio::time::timeout(TIMEOUT, handle.wait()).await })
        .map_err(|_| io::Error::other("fixture completion deadline"))?;
    ensure(
        result.reason == ExitReason::ChildExit
            && result.exit_code == Some(code)
            && result.output_drained,
        "natural exit and complete cleanup",
    )
}

fn ensure(condition: bool, label: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(label))
    }
}
