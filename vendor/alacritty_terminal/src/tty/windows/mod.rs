use std::ffi::OsStr;
use std::io::{self, Result};
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;

use crate::event::{OnResize, WindowSize};
use crate::tty::windows::child::ChildExitWatcher;
use crate::tty::{ChildEvent, EventedPty, EventedReadWrite, Options, Shell};

mod blocking;
mod child;
mod conpty;

use blocking::{UnblockedReader, UnblockedWriter};
use conpty::Conpty as Backend;
use miow::pipe::{AnonRead, AnonWrite};
use polling::{Event, Poller};

pub const PTY_CHILD_EVENT_TOKEN: usize = 1;
pub const PTY_READ_WRITE_TOKEN: usize = 2;

type ReadPipe = UnblockedReader<AnonRead>;
type WritePipe = UnblockedWriter<AnonWrite>;

pub struct Pty {
    // XXX: Backend is required to be the first field, to ensure correct drop order. Dropping
    // `conout` before `backend` will cause a deadlock (with Conpty).
    backend: Option<Backend>,
    close_worker: Option<std::thread::JoinHandle<()>>,
    conout: ReadPipe,
    conin: WritePipe,
    child_watcher: ChildExitWatcher,
}

pub fn new(config: &Options, window_size: WindowSize, _window_id: u64) -> Result<Pty> {
    conpty::new(config, window_size)
}

impl Pty {
    fn new(
        backend: impl Into<Backend>,
        conout: impl Into<ReadPipe>,
        conin: impl Into<WritePipe>,
        child_watcher: ChildExitWatcher,
    ) -> Self {
        Self {
            backend: Some(backend.into()),
            close_worker: None,
            conout: conout.into(),
            conin: conin.into(),
            child_watcher,
        }
    }

    pub fn child_watcher(&self) -> &ChildExitWatcher {
        &self.child_watcher
    }
}

fn with_key(mut event: Event, key: usize) -> Event {
    event.key = key;
    event
}

impl EventedReadWrite for Pty {
    type Reader = ReadPipe;
    type Writer = WritePipe;

    #[inline]
    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        interest: polling::Event,
        poll_opts: polling::PollMode,
    ) -> io::Result<()> {
        self.conin.register(poll, with_key(interest, PTY_READ_WRITE_TOKEN), poll_opts);
        self.conout.register(poll, with_key(interest, PTY_READ_WRITE_TOKEN), poll_opts);
        self.child_watcher.register(poll, with_key(interest, PTY_CHILD_EVENT_TOKEN));

        Ok(())
    }

    #[inline]
    fn reregister(
        &mut self,
        poll: &Arc<Poller>,
        interest: polling::Event,
        poll_opts: polling::PollMode,
    ) -> io::Result<()> {
        self.conin.register(poll, with_key(interest, PTY_READ_WRITE_TOKEN), poll_opts);
        self.conout.register(poll, with_key(interest, PTY_READ_WRITE_TOKEN), poll_opts);
        self.child_watcher.register(poll, with_key(interest, PTY_CHILD_EVENT_TOKEN));

        Ok(())
    }

    #[inline]
    fn deregister(&mut self, _poll: &Arc<Poller>) -> io::Result<()> {
        self.conin.deregister();
        self.conout.deregister();
        self.child_watcher.deregister();

        Ok(())
    }

    #[inline]
    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.conout
    }

    #[inline]
    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.conin
    }

    fn input_writer_progress(&self) -> Option<Arc<crate::input::WriterProgress>> {
        Some(self.conin.writer_progress())
    }
}

impl EventedPty for Pty {
    fn deregister_io(&mut self, _: &Arc<Poller>) -> io::Result<()> {
        self.conin.deregister();
        self.conout.deregister();
        Ok(())
    }

    fn begin_shutdown(&mut self) -> io::Result<()> {
        if self.backend.is_some() {
            // Spawn before transferring backend ownership. A failed OS thread
            // creation leaves it here for the synchronous disposal fallback.
            conpty::fault(conpty::Failure::CloseWorker)?;
            let (send, recv) = std::sync::mpsc::sync_channel(1);
            let worker =
                std::thread::Builder::new().name("ConPTY close".into()).spawn(move || {
                    if let Ok(backend) = recv.recv() {
                        trace_cleanup("close-start");
                        drop(backend);
                        trace_cleanup("close-return");
                    }
                })?;
            if let Err(error) = send.send(self.backend.take().unwrap()) {
                self.backend = Some(error.0);
                let _ = worker.join();
                return Err(io::Error::other("ConPTY close worker stopped before handoff"));
            }
            self.close_worker = Some(worker);
        }
        Ok(())
    }

    fn next_child_event(&mut self) -> Option<ChildEvent> {
        match self.child_watcher.event_rx().try_recv() {
            Ok(ev) => Some(ev),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(ChildEvent::Exited(None)),
        }
    }
}

impl OnResize for Pty {
    fn on_resize(&mut self, window_size: WindowSize) {
        if let Some(backend) = self.backend.as_mut() {
            backend.on_resize(window_size);
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // This runs on the disposing PTY worker, including explicit close and
        // unwind. Drain/discard unread output to avoid full-pipe close deadlock.
        trace_cleanup("dispose-start");
        if self.backend.is_some() {
            // Explicit cancellation can race the client's console attachment.
            // ClosePseudoConsole alone can then return while its output pipe
            // remains open indefinitely. Disposal cancels the owned root child;
            // natural exits already called begin_shutdown and keep their drain.
            // SAFETY: the watcher owns this process handle throughout PTY drop.
            let terminated = unsafe {
                windows_sys::Win32::System::Threading::TerminateProcess(
                    self.child_watcher.raw_handle(),
                    1,
                )
            };
            if terminated == 0 {
                trace_cleanup("child-already-exited-or-termination-failed");
            } else {
                trace_cleanup("child-termination-requested");
            }
        }
        if let Err(error) = self.begin_shutdown() {
            // Thread exhaustion must not panic or strand a live HPCON. Let the
            // existing OS reader close its source instead of parking on piper;
            // close on this disposal thread, then join the released reader.
            log::error!("ConPTY close worker unavailable: {error}");
            self.conout.abandon_output();
            drop(self.backend.take());
        }
        self.conout.drain_and_join();
        trace_cleanup("reader-joined");
        if let Some(worker) = self.close_worker.take() {
            let _ = worker.join();
        }
        trace_cleanup("close-joined");
        self.conin.close_and_join();
        trace_cleanup("writer-joined");
        // TerminateProcess is asynchronous. Completion must also wait for the
        // owned root process, using its existing handle, on this cleanup worker.
        // SAFETY: the watcher still owns the handle; no UI thread waits here.
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(
                self.child_watcher.raw_handle(),
                windows_sys::Win32::System::Threading::INFINITE,
            );
        }
        trace_cleanup("child-exited");
    }
}

// Opt-in metadata only, checked only during cleanup (never the I/O hot path).
fn trace_cleanup(stage: &str) {
    if std::env::var_os("PIKE_TRACE_CLEANUP").is_some() {
        eprintln!("pike-cleanup {:?} {stage}", std::thread::current().id());
    }
}

// Modified per stdlib implementation.
// https://github.com/rust-lang/rust/blob/6707bf0f59485cf054ac1095725df43220e4be20/library/std/src/sys/args/windows.rs#L174
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

fn cmdline(config: &Options) -> String {
    let default_shell = Shell::new("powershell".to_owned(), Vec::new());
    let shell = config.shell.as_ref().unwrap_or(&default_shell);

    let mut cmd = String::new();
    cmd.push_str(&shell.program);

    for arg in &shell.args {
        cmd.push(' ');
        if config.escape_args {
            push_escaped_arg(&mut cmd, arg);
        } else {
            cmd.push_str(arg)
        }
    }
    cmd
}

/// Converts the string slice into a Windows-standard representation for "W"-
/// suffixed function variants, which accept UTF-16 encoded string values.
pub fn win32_string<S: AsRef<OsStr> + ?Sized>(value: &S) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

#[cfg(test)]
mod test {
    use crate::tty::windows::{cmdline, push_escaped_arg};
    use crate::tty::{Options, Shell};

    #[test]
    fn test_escape() {
        let test_set = vec![
            // Basic cases - no escaping needed
            ("abc", "abc"),
            // Cases requiring quotes (space/tab)
            ("", "\"\""),
            (" ", "\" \""),
            ("ab c", "\"ab c\""),
            ("ab\tc", "\"ab\tc\""),
            // Cases with backslashes only (no spaces, no quotes) - no quotes added
            ("ab\\c", "ab\\c"),
            // Cases with quotes only (no spaces) - quotes escaped but no outer quotes
            ("ab\"c", "ab\\\"c"),
            ("\"", "\\\""),
            ("a\"b\"c", "a\\\"b\\\"c"),
            // Cases requiring both quotes and escaping (contains spaces)
            ("ab \"c", "\"ab \\\"c\""),
            ("a \"b\" c", "\"a \\\"b\\\" c\""),
            // Complex real-world cases
            ("C:\\Program Files\\", "\"C:\\Program Files\\\\\""),
            ("C:\\Program Files\\a.txt", "\"C:\\Program Files\\a.txt\""),
            (
                r#"sh -c "cd /home/user; ARG='abc' \""'${SHELL:-sh}" -i -c '"'echo hello'""#,
                r#""sh -c \"cd /home/user; ARG='abc' \\\"\"'${SHELL:-sh}\" -i -c '\"'echo hello'\"""#,
            ),
        ];

        for (input, expected) in test_set {
            let mut escaped_arg = String::new();
            push_escaped_arg(&mut escaped_arg, input);
            assert_eq!(escaped_arg, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_cmdline() {
        let mut options = Options {
            shell: Some(Shell {
                program: "echo".to_string(),
                args: vec!["hello world".to_string()],
            }),
            working_directory: None,
            drain_on_exit: true,
            env: Default::default(),
            escape_args: false,
        };
        assert_eq!(cmdline(&options), "echo hello world");

        options.escape_args = true;
        assert_eq!(cmdline(&options), "echo \"hello world\"");
    }
}
