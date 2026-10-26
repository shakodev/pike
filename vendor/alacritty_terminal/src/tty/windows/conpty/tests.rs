//! Isolated, opt-in release resource probe. Only synthetic child metadata is emitted.
use std::cell::Cell;
use std::io::{Read, Write};
use std::mem;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::time::Duration;
use std::time::Instant;

use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

use super::*;
use crate::tty::{EventedReadWrite, Shell};

thread_local! {
    pub(super) static FAILURE: Cell<Option<Failure>> = const { Cell::new(None) };
}

fn injected<T>(point: Option<Failure>, f: impl FnOnce() -> T) -> T {
    struct Reset(Option<Failure>);
    impl Drop for Reset {
        fn drop(&mut self) {
            FAILURE.with(|failure| failure.set(self.0));
        }
    }
    let _reset = Reset(FAILURE.with(|failure| failure.replace(point)));
    f()
}

const SIZE: WindowSize = WindowSize { num_lines: 24, num_cols: 80, cell_width: 8, cell_height: 16 };

fn options() -> Options {
    Options {
        shell: Some(Shell::new("cmd.exe".into(), vec!["/d".into(), "/c".into(), "exit 0".into()])),
        ..Options::default()
    }
}

struct Sample {
    handles: u32,
    private: usize,
    threads: usize,
}

fn sample() -> Sample {
    // SAFETY: pseudo process handle is borrowed; output buffers are initialized,
    // sized SDK structs. The snapshot is adopted exactly once and closed by RAII.
    unsafe {
        let mut handles = 0;
        assert_ne!(GetProcessHandleCount(GetCurrentProcess(), &mut handles), 0);
        let mut memory: PROCESS_MEMORY_COUNTERS_EX = mem::zeroed();
        memory.cb = mem::size_of_val(&memory) as u32;
        assert_ne!(
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                memory.cb
            ),
            0
        );
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        assert_ne!(snapshot, INVALID_HANDLE_VALUE);
        let snapshot = OwnedHandle::from_raw_handle(snapshot);
        let mut entry: THREADENTRY32 = mem::zeroed();
        entry.dwSize = mem::size_of_val(&entry) as u32;
        let mut more = Thread32First(snapshot.as_raw_handle() as HANDLE, &mut entry);
        let mut threads = 0;
        while more != 0 {
            threads += usize::from(entry.th32OwnerProcessID == std::process::id());
            more = Thread32Next(snapshot.as_raw_handle() as HANDLE, &mut entry);
        }
        Sample { handles, private: memory.PrivateUsage, threads }
    }
}

#[test]
#[ignore = "isolated release resource measurement; run with --ignored --exact --nocapture"]
fn resource_cycles() {
    let mut deltas = Vec::new();
    let mut modes = vec![
        ("missing-executable".to_string(), None),
        ("invalid-cwd".to_string(), None),
        ("close".to_string(), None),
    ];
    modes.extend(
        [
            Failure::OutputPipe,
            Failure::InputPipe,
            Failure::Console,
            Failure::AttributeSize,
            Failure::AttributeInit,
            Failure::AttributeUpdate,
            Failure::Process,
            Failure::Wait,
            Failure::ReaderWorker,
            Failure::WriterWorker,
        ]
        .map(|point| (format!("injected-{point:?}"), Some(point))),
    );
    for (mode, point) in modes {
        let mut config = options();
        if mode == "missing-executable" {
            config.shell =
                Some(Shell::new(r"C:\pike-nonexistent-8\missing.exe".into(), Vec::new()));
        } else if mode == "invalid-cwd" {
            config.working_directory = Some(r"C:\pike-nonexistent-8\cwd".into());
        }
        for _ in 0..10 {
            drop(injected(point, || new(&config, SIZE)));
        }
        let before = sample();
        for cycle in 0..=100 {
            let start = Instant::now();
            if cycle != 0 {
                let result = injected(point, || new(&config, SIZE));
                assert_eq!(result.is_ok(), mode == "close");
                drop(result);
            }
            let sample = sample();
            println!(
                "resource,{mode},{cycle},{},{},{},{:.3}",
                sample.handles,
                sample.private,
                sample.threads,
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
        let after = sample();
        println!(
            "delta,{mode},{},{},{}",
            i64::from(after.handles) - i64::from(before.handles),
            after.private as i64 - before.private as i64,
            after.threads as i64 - before.threads as i64
        );
        deltas.push((mode, i64::from(after.handles) - i64::from(before.handles)));
    }
    for (mode, delta) in deltas {
        assert!(delta <= 4, "linear handle growth in {mode}: {delta}");
    }
}

#[test]
fn api_errors_do_not_panic() {
    assert!(new(&options(), WindowSize { num_cols: 0, ..SIZE }).is_err());
    let mut pty = new(&options(), SIZE).unwrap();
    pty.on_resize(WindowSize { num_cols: 0, ..SIZE });
    drop(pty);
}

#[test]
fn failed_registration_releases_callback_and_process() {
    let mut process =
        std::process::Command::new("cmd.exe").args(["/d", "/c", "exit 0"]).spawn().unwrap();
    use std::os::windows::io::AsHandle;
    let handle = Arc::new(process.as_handle().try_clone_to_owned().unwrap());
    let weak = Arc::downgrade(&handle);
    let result = injected(Some(Failure::Wait), || ChildExitWatcher::new(handle));
    assert!(result.is_err());
    assert!(weak.upgrade().is_none(), "callback context retained process on registration failure");
    process.wait().unwrap();
}

#[test]
fn unregister_before_exit_releases_context() {
    let mut child =
        std::process::Command::new("cmd.exe").stdin(std::process::Stdio::piped()).spawn().unwrap();
    use std::os::windows::io::AsHandle;
    let handle = Arc::new(child.as_handle().try_clone_to_owned().unwrap());
    let weak = Arc::downgrade(&handle);
    let watcher = ChildExitWatcher::new(handle).unwrap();
    drop(watcher);
    assert!(weak.upgrade().is_none(), "unfired callback retained its context");
    child.kill().unwrap();
    child.wait().unwrap();
}

// The same test executable is a controlled child fixture. No shell scripting,
// extra PTY, real terminal transcript, or production fault-injection hooks.
const FIXTURE: &str = "tty::windows::conpty::tests::attached_fixture";

#[test]
#[ignore = "child fixture invoked only by attached_tree_and_flood"]
fn attached_fixture() {
    let directory = std::env::var_os("PIKE_AUDIT_DIRECTORY").unwrap();
    let directory = Path::new(&directory);
    let depth: u32 = std::env::var("PIKE_AUDIT_DEPTH").unwrap().parse().unwrap();
    let mut children = Vec::new();
    if depth < 2 {
        children.push(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--ignored", "--exact", FIXTURE, "--nocapture"])
                .env("PIKE_AUDIT_DEPTH", (depth + 1).to_string())
                .spawn()
                .unwrap(),
        );
    }
    std::fs::write(directory.join(format!("{depth}.pid")), std::process::id().to_string()).unwrap();
    if std::env::var("PIKE_AUDIT_MODE").unwrap() != "attached" {
        let block = [b'x'; 8192];
        loop {
            std::io::stdout().write_all(&block).unwrap();
        }
    }
    loop {
        std::thread::park();
    }
}

#[test]
#[ignore = "isolated Windows integration probe; invoked explicitly by CI"]
fn attached_tree_and_flood() {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};
    for mode in ["attached", "flood", "close-worker-failure"] {
        let directory =
            std::env::temp_dir().join(format!("pike-audit-{}-{mode}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let mut config = Options {
            shell: Some(Shell::new(
                std::env::current_exe().unwrap().to_str().unwrap().into(),
                vec!["--ignored".into(), "--exact".into(), FIXTURE.into(), "--nocapture".into()],
            )),
            escape_args: true,
            ..Options::default()
        };
        config.env.insert("PIKE_AUDIT_DIRECTORY".into(), directory.to_str().unwrap().into());
        config.env.insert("PIKE_AUDIT_DEPTH".into(), "0".into());
        config.env.insert("PIKE_AUDIT_MODE".into(), mode.into());
        let mut pty = new(&config, SIZE).unwrap();
        let start = Instant::now();
        let mut buffer = [0; 8192];
        // Drain during attachment so the fixture can publish all three PIDs.
        while !(0..3).all(|depth| {
            std::fs::read_to_string(directory.join(format!("{depth}.pid")))
                .is_ok_and(|pid| !pid.is_empty())
        }) {
            assert!(start.elapsed() < Duration::from_secs(10), "fixture readiness deadline");
            let _ = pty.reader().read(&mut buffer);
            std::thread::sleep(Duration::from_millis(2));
        }
        let processes: Vec<_> = (0..3)
            .map(|depth| {
                let pid: u32 = std::fs::read_to_string(directory.join(format!("{depth}.pid")))
                    .unwrap()
                    .parse()
                    .unwrap();
                // SAFETY: owned observation handle, acquired before disposal to avoid PID reuse.
                let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
                assert!(!process.is_null());
                unsafe { OwnedHandle::from_raw_handle(process) }
            })
            .collect();
        // Leave output unread briefly so close must handle a full pipe.
        std::thread::sleep(Duration::from_millis(50));
        let close = Instant::now();
        injected((mode == "close-worker-failure").then_some(Failure::CloseWorker), || drop(pty));
        for process in processes {
            // SAFETY: process remains owned through this finite wait.
            assert_eq!(
                unsafe { WaitForSingleObject(process.as_raw_handle() as HANDLE, 5000) },
                0,
                "attached descendant survived close"
            );
        }
        println!("tree,{mode},3,{:.3}", close.elapsed().as_secs_f64() * 1000.0);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
