//! Isolated, opt-in release resource probe. Only synthetic child metadata is emitted.
use std::mem;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::time::Instant;

use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

use super::*;
use crate::tty::Shell;

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
    for mode in ["missing-executable", "invalid-cwd", "close"] {
        let mut config = options();
        if mode == "missing-executable" {
            config.shell =
                Some(Shell::new(r"C:\pike-nonexistent-8\missing.exe".into(), Vec::new()));
        } else if mode == "invalid-cwd" {
            config.working_directory = Some(r"C:\pike-nonexistent-8\cwd".into());
        }
        for _ in 0..10 {
            drop(new(&config, SIZE));
        }
        let before = sample();
        for cycle in 0..=100 {
            let start = Instant::now();
            if cycle != 0 {
                let result = new(&config, SIZE);
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
        deltas.push((mode, i64::from(sample().handles) - i64::from(before.handles)));
    }
    for (mode, delta) in deltas {
        assert!(delta <= 4, "linear handle growth in {mode}: {delta}");
    }
}
