//! Opt-in isolated Windows runner fixture, never a production clipboard backend.
use std::cell::Cell;
use std::io;
use std::ptr;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use windows_sys::Win32::Foundation::{GlobalFree, HGLOBAL, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardOwner,
    GetClipboardSequenceNumber, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE};

// Standard SDK format value; avoid enabling the unrelated OLE API module.
const CF_UNICODETEXT: u32 = 13;
const MAX_FIXTURE_BYTES: usize = 2 * 1024 * 1024;
// Exercise an actual allocation larger than its initialized text prefix. These
// bytes deliberately remain outside every Rust reference and clipboard read.
const FIXTURE_PADDING_BYTES: usize = 64;

#[derive(Clone, Copy)]
struct Written {
    // Identity only; never pass this old value to a Windows API after closing.
    handle_identity: usize,
    initialized_units: usize,
    sequence: u32,
}

pub struct Clipboard {
    window: HWND,
    written: Cell<Option<Written>>,
}
struct Open<'a> {
    _owner: &'a Clipboard,
}
impl Drop for Open<'_> {
    fn drop(&mut self) {
        // SAFETY: This thread opened the clipboard and retains its owner window.
        unsafe {
            CloseClipboard();
        }
    }
}
struct Allocation(HGLOBAL);
impl Drop for Allocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Allocation is still ours unless SetClipboardData took it.
            unsafe {
                GlobalFree(self.0);
            }
        }
    }
}

impl Clipboard {
    pub fn new() -> io::Result<Self> {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let title: Vec<u16> = "Pike synthetic clipboard owner\0".encode_utf16().collect();
        // SAFETY: Valid null-terminated class/title, built-in system class and
        // message-only parent. This thread creates and destroys this hidden HWND.
        let window = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if window.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            window,
            written: Cell::new(None),
        })
    }
    fn open(&self) -> io::Result<Open<'_>> {
        // SAFETY: The owner window remains alive on this thread; the guard
        // closes every successful acquisition, including all error paths.
        if unsafe { OpenClipboard(self.window) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Open { _owner: self })
    }
    pub fn write(&self, text: String) -> io::Result<()> {
        let units: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let bytes = units
            .len()
            .checked_mul(2)
            .filter(|bytes| *bytes <= MAX_FIXTURE_BYTES)
            .ok_or_else(|| {
                io::Error::other("controlled clipboard payload exceeds fixture limit")
            })?;
        let allocation_bytes = bytes
            .checked_add(FIXTURE_PADDING_BYTES)
            .filter(|bytes| *bytes <= MAX_FIXTURE_BYTES)
            .ok_or_else(|| {
                io::Error::other("controlled clipboard padding exceeds fixture limit")
            })?;
        // SAFETY: Allocate checked movable storage with deliberate spare bytes.
        // Only the text prefix is initialized. Allocation owns it until transfer.
        let mut allocation = Allocation(unsafe { GlobalAlloc(GMEM_MOVEABLE, allocation_bytes) });
        if allocation.0.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: Lock our allocation, then initialize exactly units.len()
        // values with NUL-terminated UTF-16. Spare bytes remain unreferenced.
        unsafe {
            let destination = GlobalLock(allocation.0).cast::<u16>();
            if destination.is_null() {
                return Err(io::Error::last_os_error());
            }
            ptr::copy_nonoverlapping(units.as_ptr(), destination, units.len());
            GlobalUnlock(allocation.0);
        }
        let _open = self.open()?;
        self.written.set(None);
        // SAFETY: Clipboard is open with our real message-only owner window.
        // Successful SetClipboardData transfers allocation ownership to Windows.
        unsafe {
            if EmptyClipboard() == 0 || SetClipboardData(CF_UNICODETEXT, allocation.0).is_null() {
                return Err(io::Error::last_os_error());
            }
        }
        let handle_identity = allocation.0 as usize;
        allocation.0 = ptr::null_mut();
        // SAFETY: Clipboard remains open after our non-delayed write. The
        // sequence and owner identify these initialized bytes on later reads.
        let sequence = unsafe { GetClipboardSequenceNumber() };
        if sequence == 0 {
            return Err(io::Error::other("clipboard sequence unavailable"));
        }
        self.written.set(Some(Written {
            handle_identity,
            initialized_units: units.len(),
            sequence,
        }));
        Ok(())
    }
    pub fn read(&self) -> io::Result<String> {
        // This fixture is intentionally not a general-purpose clipboard reader.
        // Only read the exact initialized prefix of our own successful write;
        // unknown/foreign formats and allocation padding have no Rust value
        // initialization guarantee, even when GlobalSize says they are in bounds.
        let written = self.written.get().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "no owned clipboard write to observe",
            )
        })?;
        let _open = self.open()?;
        // SAFETY: The clipboard stays open while borrowing OS-owned storage.
        // Reject changed content BEFORE locking or referencing it. A valid
        // clipboard consumer may not modify/free a GetClipboardData handle.
        // Only the recorded prefix of this non-delayed, unchanged owned write
        // is known initialized; never create a slice over GlobalSize padding.
        // Copy and unlock before closing; never dereference a retained handle.
        unsafe {
            if GetClipboardOwner() != self.window
                || GetClipboardSequenceNumber() != written.sequence
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "controlled clipboard ownership changed",
                ));
            }
            let handle = GetClipboardData(CF_UNICODETEXT);
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            if handle as usize != written.handle_identity {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "controlled clipboard allocation changed",
                ));
            }
            let bytes = GlobalSize(handle);
            let initialized_bytes = written
                .initialized_units
                .checked_mul(2)
                .filter(|bytes| *bytes > 0 && *bytes <= MAX_FIXTURE_BYTES)
                .ok_or_else(|| io::Error::other("invalid initialized clipboard extent"))?;
            if bytes < initialized_bytes || bytes > MAX_FIXTURE_BYTES || !bytes.is_multiple_of(2) {
                return Err(io::Error::other("invalid controlled clipboard allocation"));
            }
            let data = GlobalLock(handle).cast::<u16>();
            if data.is_null() {
                return Err(io::Error::last_os_error());
            }
            let units = std::slice::from_raw_parts(data, written.initialized_units);
            let result = units
                .iter()
                .position(|unit| *unit == 0)
                .ok_or_else(|| io::Error::other("controlled clipboard text is not terminated"))
                .and_then(|end| String::from_utf16(&units[..end]).map_err(io::Error::other));
            GlobalUnlock(handle);
            result
        }
    }
}
impl Drop for Clipboard {
    fn drop(&mut self) {
        // SAFETY: This thread owns the message-only HWND and no Open guard can
        // outlive its borrowed owner. Windows owns transferred text allocations.
        unsafe {
            DestroyWindow(self.window);
        }
    }
}

pub struct Blocker {
    release: Option<mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<io::Result<()>>>,
}
impl Blocker {
    pub fn new() -> io::Result<Self> {
        let (ready, ready_rx) = mpsc::sync_channel(1);
        let (release, release_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let clipboard = Clipboard::new()?;
            let _open = clipboard.open()?;
            ready.send(()).map_err(io::Error::other)?;
            release_rx
                .recv_timeout(Duration::from_secs(5))
                .map_err(io::Error::other)?;
            Ok(())
        });
        let result = Self {
            release: Some(release),
            worker: Some(worker),
        };
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(io::Error::other)?;
        Ok(result)
    }
    pub fn finish(mut self) -> io::Result<()> {
        self.release
            .take()
            .unwrap()
            .send(())
            .map_err(io::Error::other)?;
        self.worker
            .take()
            .unwrap()
            .join()
            .map_err(|_| io::Error::other("clipboard blocker thread failed"))?
    }
}
impl Drop for Blocker {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
