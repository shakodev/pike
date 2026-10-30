//! Explicit native ConPTY ownership probe. Cross compilation is not execution.
use super::*;
use std::fs;
use std::io::{self, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::Command;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

const FIXTURE: &str = "app::tests::closing::windows::attached_fixture";

#[test]
#[ignore = "child fixture; invoked only by native_attached_tree_cleanup"]
fn attached_fixture() {
    let directory = std::env::var_os("PIKE28_TREE_DIRECTORY").expect("fixture directory");
    let directory = std::path::PathBuf::from(directory);
    let depth: usize = std::env::var("PIKE28_TREE_DEPTH").unwrap().parse().unwrap();
    fs::write(
        directory.join(format!("{depth}.pid")),
        std::process::id().to_string(),
    )
    .unwrap();
    let mut child = (depth < 2).then(|| {
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", FIXTURE, "--ignored", "--nocapture"])
            .env("PIKE28_TREE_DEPTH", (depth + 1).to_string())
            .spawn()
            .unwrap()
    });
    let started = Instant::now();
    let mut stdout = io::stdout().lock();
    // Bound fixture lifetime even if a platform fails to close descendants.
    // No detached process flags or second PTY implementation are involved.
    while started.elapsed() < Duration::from_secs(15) {
        if stdout
            .write_all(b"PIKE28 attached synthetic output\r\n")
            .is_err()
        {
            break;
        }
        let _ = stdout.flush();
        std::thread::sleep(Duration::from_millis(2));
    }
    if let Some(child) = &mut child {
        let _ = child.wait();
    }
}

#[test]
#[ignore = "requires native Windows/ConPTY; run explicitly, never via cross compilation"]
fn native_attached_tree_cleanup() {
    let executor = SharedExecutor::new().unwrap();
    let directory = std::env::temp_dir().join(format!("pike28-tree-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let mut settings = BackendSettings {
        program: std::env::current_exe().unwrap().to_str().unwrap().into(),
        args: vec![
            "--exact".into(),
            FIXTURE.into(),
            "--ignored".into(),
            "--nocapture".into(),
        ],
        ..Default::default()
    };
    settings.env.insert(
        "PIKE28_TREE_DIRECTORY".into(),
        directory.to_str().unwrap().into(),
    );
    settings.env.insert("PIKE28_TREE_DEPTH".into(), "0".into());
    let mut app = App {
        profile: settings,
        ..App::default()
    };
    let (ids, ready) = launch(&mut app, &executor);
    let handle = ready.shutdown_handle();
    drop(app.launched(ids, Ok(ready)));
    let readiness = Instant::now();
    while !(0..3).all(|depth| {
        fs::read_to_string(directory.join(format!("{depth}.pid"))).is_ok_and(|pid| !pid.is_empty())
    }) {
        assert!(readiness.elapsed() < DEADLINE, "attached tree readiness");
        std::thread::sleep(Duration::from_millis(5));
    }
    let processes: Vec<_> = (0..3)
        .map(|depth| {
            let pid = fs::read_to_string(directory.join(format!("{depth}.pid")))
                .unwrap()
                .parse()
                .unwrap();
            // SAFETY: acquire an observation handle before close to avoid PID reuse.
            // It grants synchronization only and OwnedHandle closes it exactly once.
            let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
            assert!(!process.is_null());
            unsafe { OwnedHandle::from_raw_handle(process.cast()) }
        })
        .collect();
    let close = Instant::now();
    drop(app.update(Message::CloseWindow));
    assert_eq!(app.close_confirmation().unwrap().1, 1);
    confirm_close(&mut app);
    let request = close.elapsed();
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, app.launches.shutdown())
            .await
            .unwrap();
    });
    assert!(
        handle.completion().is_some(),
        "pool returned before native acknowledgement"
    );
    for process in processes {
        // SAFETY: the handle remains owned through this finite observation.
        assert_eq!(
            unsafe { WaitForSingleObject(process.as_raw_handle() as HANDLE, 5000) },
            0,
            "attached descendant survived close"
        );
    }
    assert!(app.update(Message::ShutdownComplete).units() > 0);
    assert!(app.model.sessions().is_empty());
    eprintln!(
        "pike28 native Windows tree=3 request_us={} cleanup_ms={}",
        request.as_micros(),
        close.elapsed().as_millis()
    );
    fs::remove_dir_all(directory).unwrap();
}
