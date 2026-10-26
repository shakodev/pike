use std::ffi::c_void;
use std::io::Error;
use std::num::NonZeroU32;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use polling::os::iocp::{CompletionPacket, PollerIocpExt};
use polling::{Event, Poller};

use windows_sys::Win32::Foundation::{BOOLEAN, FALSE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessId, RegisterWaitForSingleObject, UnregisterWaitEx, INFINITE,
    WT_EXECUTEINWAITTHREAD, WT_EXECUTEONLYONCE,
};

use crate::tty::ChildEvent;

struct Interest {
    poller: Arc<Poller>,
    event: Event,
}

struct ChildExitSender {
    sender: mpsc::Sender<ChildEvent>,
    interest: Arc<Mutex<Option<Interest>>>,
    child_handle: Arc<OwnedHandle>,
    exited: AtomicBool,
}

/// WinAPI callback to run when child process exits.
extern "system" fn child_exit_callback(ctx: *mut c_void, timed_out: BOOLEAN) {
    if timed_out != 0 {
        return;
    }

    // SAFETY: the watcher owns this allocation and keeps it alive until the
    // blocking UnregisterWaitEx has joined every pending callback.
    let event_tx = unsafe { &*ctx.cast::<ChildExitSender>() };

    let mut exit_code = 0_u32;
    let child_handle = event_tx.child_handle.as_raw_handle() as HANDLE;
    let status = unsafe { GetExitCodeProcess(child_handle, &mut exit_code) };
    let exit_code = if status == FALSE { None } else { Some(exit_code as i32) };
    event_tx.sender.send(ChildEvent::Exited(exit_code)).ok();

    event_tx.exited.store(true, Ordering::Release);
    let interest = event_tx.interest.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(interest) = interest.as_ref() {
        interest.poller.post(CompletionPacket::new(interest.event)).ok();
    }
}

pub struct ChildExitWatcher {
    wait_handle: AtomicPtr<c_void>,
    event_rx: mpsc::Receiver<ChildEvent>,
    interest: Arc<Mutex<Option<Interest>>>,
    sender: Option<Box<ChildExitSender>>,
    pid: Option<NonZeroU32>,
}

impl ChildExitWatcher {
    pub fn new(child_handle: Arc<OwnedHandle>) -> Result<ChildExitWatcher, Error> {
        let (event_tx, event_rx) = mpsc::channel();

        let mut wait_handle: HANDLE = ptr::null_mut();
        let interest = Arc::new(Mutex::new(None));
        let mut sender_ref = Box::new(ChildExitSender {
            sender: event_tx,
            interest: interest.clone(),
            child_handle: child_handle.clone(),
            exited: AtomicBool::new(false),
        });

        super::conpty::fault(super::conpty::Failure::Wait)?;
        // SAFETY: the boxed context stays stable through registration and is
        // reclaimed only after a synchronous unregister. Failure drops it here.
        let success = unsafe {
            RegisterWaitForSingleObject(
                &mut wait_handle,
                child_handle.as_raw_handle() as HANDLE,
                Some(child_exit_callback),
                (&mut *sender_ref as *mut ChildExitSender).cast(),
                INFINITE,
                WT_EXECUTEINWAITTHREAD | WT_EXECUTEONLYONCE,
            )
        };

        if success == 0 {
            Err(Error::last_os_error())
        } else {
            let pid =
                unsafe { NonZeroU32::new(GetProcessId(child_handle.as_raw_handle() as HANDLE)) };
            Ok(ChildExitWatcher {
                event_rx,
                interest,
                pid,
                sender: Some(sender_ref),
                wait_handle: AtomicPtr::from(wait_handle),
            })
        }
    }

    pub fn event_rx(&self) -> &mpsc::Receiver<ChildEvent> {
        &self.event_rx
    }

    pub fn register(&self, poller: &Arc<Poller>, event: Event) {
        let mut interest = self.interest.lock().unwrap();
        *interest = Some(Interest { poller: poller.clone(), event });
        // A fast child can exit before poller registration. Re-post its reserved
        // notification instead of leaving an already queued exit event asleep.
        if self.sender.as_ref().unwrap().exited.load(Ordering::Acquire) {
            poller.post(CompletionPacket::new(event)).ok();
        }
    }

    pub fn deregister(&self) {
        *self.interest.lock().unwrap() = None;
    }

    /// Retrieve the process handle of the underlying child process.
    ///
    /// This function does **not** pass ownership of the raw handle to you,
    /// and the handle is only guaranteed to be valid while the hosted application
    /// has not yet been destroyed.
    ///
    /// If you terminate the process using this handle, the terminal will get a
    /// timeout error, and the child watcher will emit an `Exited` event.
    pub fn raw_handle(&self) -> HANDLE {
        self.sender.as_ref().unwrap().child_handle.as_raw_handle() as HANDLE
    }

    /// Retrieve the Process ID associated to the underlying child process.
    pub fn pid(&self) -> Option<NonZeroU32> {
        self.pid
    }
}

impl Drop for ChildExitWatcher {
    fn drop(&mut self) {
        // Never hold the interest mutex while joining the callback. This runs
        // during backend disposal, outside the UI and outside the callback.
        // SAFETY: we own this registration; INVALID_HANDLE_VALUE requests a
        // synchronous unregister, so callback state and process outlive all uses.
        let success = unsafe {
            UnregisterWaitEx(
                self.wait_handle.load(Ordering::Relaxed) as HANDLE,
                INVALID_HANDLE_VALUE,
            )
        };
        if success == 0 {
            // An unexpected unregister failure cannot prove callback quiescence.
            // Retain its context and process rather than risk use-after-free;
            // this exceptional path is logged without any terminal payload.
            log::error!("child wait unregister failed: {}", Error::last_os_error());
            if let Some(sender) = self.sender.take() {
                let _ = Box::into_raw(sender);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::windows::io::AsHandle;
    use std::process::Command;
    use std::sync::Arc;
    use std::time::Duration;

    use super::super::PTY_CHILD_EVENT_TOKEN;
    use super::*;

    #[test]
    fn destruction_joins_a_running_callback_before_releasing_context() {
        let mut child =
            Command::new("cmd.exe").stdin(std::process::Stdio::piped()).spawn().unwrap();
        let process = Arc::new(child.as_handle().try_clone_to_owned().unwrap());
        let weak = Arc::downgrade(&process);
        let watcher = ChildExitWatcher::new(process).unwrap();
        let interest = watcher.interest.clone();
        let gate = interest.lock().unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        // Callback sends before acquiring interest, so it is now blocked by gate.
        watcher.event_rx().recv_timeout(Duration::from_secs(5)).unwrap();
        let (send, recv) = mpsc::channel();
        let cleanup = std::thread::spawn(move || {
            drop(watcher);
            send.send(()).unwrap();
        });
        let pending = recv.recv_timeout(Duration::from_millis(50));
        drop(gate);
        recv.recv_timeout(Duration::from_secs(5)).unwrap();
        cleanup.join().unwrap();
        assert_eq!(pending, Err(mpsc::RecvTimeoutError::Timeout));
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn exit_before_poller_registration_is_not_lost() {
        let mut child = Command::new("cmd.exe").args(["/d", "/c", "exit 37"]).spawn().unwrap();
        let watcher =
            ChildExitWatcher::new(Arc::new(child.as_handle().try_clone_to_owned().unwrap()))
                .unwrap();
        child.wait().unwrap();
        assert_eq!(
            watcher.event_rx().recv_timeout(Duration::from_secs(5)).unwrap(),
            ChildEvent::Exited(Some(37))
        );
        // Take the callback's interest lock so its publication is complete.
        while !watcher.sender.as_ref().unwrap().exited.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        let poller = Arc::new(Poller::new().unwrap());
        watcher.register(&poller, Event::readable(PTY_CHILD_EVENT_TOKEN));
        let mut events = polling::Events::new();
        poller.wait(&mut events, Some(Duration::from_secs(5))).unwrap();
        assert!(events.iter().any(|event| event.key == PTY_CHILD_EVENT_TOKEN));
    }

    #[test]
    pub fn event_is_emitted_when_child_exits() {
        const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

        let poller = Arc::new(Poller::new().unwrap());

        let mut child = Command::new("cmd.exe").spawn().unwrap();
        let child_exit_watcher = ChildExitWatcher::new(Arc::new(
            std::os::windows::io::AsHandle::as_handle(&child).try_clone_to_owned().unwrap(),
        ))
        .unwrap();
        child_exit_watcher.register(&poller, Event::readable(PTY_CHILD_EVENT_TOKEN));

        child.kill().unwrap();
        child.wait().unwrap();

        // Poll for the event or fail with timeout if nothing has been sent.
        let mut events = polling::Events::new();
        poller.wait(&mut events, Some(WAIT_TIMEOUT)).unwrap();
        assert_eq!(events.iter().next().unwrap().key, PTY_CHILD_EVENT_TOKEN);
        // Verify that at least one `ChildEvent::Exited` was received.
        assert_eq!(child_exit_watcher.event_rx().try_recv(), Ok(ChildEvent::Exited(Some(1))));
    }
}
