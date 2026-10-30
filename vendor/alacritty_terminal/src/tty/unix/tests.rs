use super::*;
use crate::tty::Shell;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[test]
fn dropping_output_backlog_closes_master_and_reaps_child_before_returning() {
    // Isolate the numeric closed-fd oracle: parallel tests must not reuse this
    // descriptor between destruction and F_GETFD. Only this test runs below.
    const CHILD: &str = "PIKE28_NATIVE_DROP_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "tty::unix::tests::dropping_output_backlog_closes_master_and_reaps_child_before_returning",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success(), "native disposal subprocess failed: {status}");
        return;
    }

    let mut pty = new(
        &Options {
            shell: Some(Shell::new(
                "/bin/sh".into(),
                vec!["-c".into(), "while :; do printf 'PIKE28_FLOOD\\r\\n'; done".into()],
            )),
            ..Default::default()
        },
        WindowSize { num_lines: 24, num_cols: 80, cell_width: 8, cell_height: 16 },
        28,
    )
    .unwrap();
    let child_pid = pty.child().id() as libc::pid_t;
    let master_fd = pty.file().as_raw_fd();
    let ready_at = Instant::now();
    loop {
        match pty.reader().read(&mut [0]) {
            Ok(1) => break,
            Err(error) if error.kind() == ErrorKind::WouldBlock => (),
            result => panic!("missing fixture output: {result:?}"),
        }
        assert!(ready_at.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(1)); // Fixture readiness only.
    }
    // Deliberately stop reading and let the controlled flood fill the output
    // queue. A dying macOS child can wait for this queue while closing its tty.
    std::thread::sleep(Duration::from_millis(100));
    let (completed, receipt) = mpsc::channel();
    let began = Instant::now();
    let worker = std::thread::spawn(move || {
        drop(pty);
        // SAFETY: these syscalls accept raw numeric IDs even after closure.
        // No borrowed descriptor is constructed for the now-closed master.
        let fd_result = unsafe { libc::fcntl(master_fd, libc::F_GETFD) };
        let fd_error = Error::last_os_error().raw_os_error();
        let mut status = 0;
        // SAFETY: status is writable, and this is our own exact child PID.
        let child_result = unsafe { libc::waitpid(child_pid, &mut status, libc::WNOHANG) };
        let child_error = Error::last_os_error().raw_os_error();
        completed.send((fd_result, fd_error, child_result, child_error)).unwrap();
    });
    let (fd_result, fd_error, child_result, child_error) = receipt
        .recv_timeout(Duration::from_secs(10))
        .expect("native disposal still pending with an unread output backlog");
    worker.join().unwrap();
    assert_eq!((fd_result, fd_error), (-1, Some(libc::EBADF)), "master still open");
    assert_eq!((child_result, child_error), (-1, Some(libc::ECHILD)), "child not reaped");
    eprintln!(
        "pike28 native disposal child_pid={child_pid} master_fd={master_fd} closed=true reaped=true cleanup_us={}",
        began.elapsed().as_micros()
    );
}
