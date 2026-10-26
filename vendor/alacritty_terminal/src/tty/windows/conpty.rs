use log::{info, warn};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::io::{Error, ErrorKind, Result};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::sync::Arc;
use std::{mem, ptr};

use windows_sys::core::HRESULT;
use windows_sys::s;
use windows_sys::Win32::Foundation::{FreeLibrary, ERROR_INSUFFICIENT_BUFFER, HANDLE, HMODULE};
use windows_sys::Win32::System::Console::{COORD, HPCON};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
};

use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList, ResumeThread,
    TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject, CREATE_SUSPENDED,
    CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, INFINITE,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
    STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

use crate::event::{OnResize, WindowSize};
use crate::tty::windows::blocking::{UnblockedReader, UnblockedWriter};
use crate::tty::windows::child::ChildExitWatcher;
use crate::tty::windows::{cmdline, win32_string, Pty};
use crate::tty::Options;

const PIPE_CAPACITY: usize = crate::event_loop::READ_BUFFER_SIZE;

// The package's inc/conpty.h uses the same signatures as the inbox SDK API,
// with Conpty-prefixed exports. Keep one matched DLL/host pair beside the exe.
// Do not search PATH or fall back to leaking inbox APIs (issue #55).
type CreatePseudoConsoleFn =
    unsafe extern "system" fn(COORD, HANDLE, HANDLE, u32, *mut HPCON) -> HRESULT;
type ResizePseudoConsoleFn = unsafe extern "system" fn(HPCON, COORD) -> HRESULT;
type ClosePseudoConsoleFn = unsafe extern "system" fn(HPCON);

struct ConptyApi {
    create: CreatePseudoConsoleFn,
    resize: ResizePseudoConsoleFn,
    close: ClosePseudoConsoleFn,
    _library: Library,
}

// The module must outlive all calls through its function pointers.
struct Library(HMODULE);

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns exactly one successful LoadLibraryExW reference.
        unsafe {
            FreeLibrary(self.0);
        }
    }
}

impl ConptyApi {
    fn new() -> Result<Self> {
        let executable = std::env::current_exe()?;
        let directory =
            executable.parent().ok_or_else(|| Error::other("missing executable directory"))?;
        Self::load_from(directory)
    }

    fn load_from(directory: &Path) -> Result<Self> {
        let required = |name: &str| -> Result<()> {
            if !directory.join(name).is_file() {
                return Err(Error::new(ErrorKind::NotFound,
                    format!("Pike requires packaged {name} beside its executable; run tools/prepare-conpty.ps1 or reinstall the complete Windows package")));
            }
            Ok(())
        };
        required("conpty.dll")?;
        required("OpenConsole.exe")?;
        let path = win32_string(&directory.join("conpty.dll"));
        // SAFETY: a terminated absolute path selects only our app-local module.
        // Dependency lookup is limited to that directory and System32. The
        // library owner remains live through every function-pointer call.
        unsafe {
            let module = LoadLibraryExW(
                path.as_ptr(),
                ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            );
            if module.is_null() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("cannot load packaged ConPTY runtime: {}", Error::last_os_error()),
                ));
            }
            let library = Library(module);
            let missing = || {
                Error::new(ErrorKind::InvalidData, "packaged ConPTY runtime is missing required exports; reinstall the complete Windows package")
            };
            let create =
                GetProcAddress(library.0, s!("ConptyCreatePseudoConsole")).ok_or_else(missing)?;
            let resize =
                GetProcAddress(library.0, s!("ConptyResizePseudoConsole")).ok_or_else(missing)?;
            let close =
                GetProcAddress(library.0, s!("ConptyClosePseudoConsole")).ok_or_else(missing)?;
            type LoadedFn = unsafe extern "system" fn() -> isize;
            info!("Using packaged ConPTY runtime");
            Ok(Self {
                create: mem::transmute::<LoadedFn, CreatePseudoConsoleFn>(create),
                resize: mem::transmute::<LoadedFn, ResizePseudoConsoleFn>(resize),
                close: mem::transmute::<LoadedFn, ClosePseudoConsoleFn>(close),
                _library: library,
            })
        }
    }
}

/// RAII Pseudoconsole.
pub struct Conpty {
    pub handle: HPCON,
    api: ConptyApi,
}

impl Drop for Conpty {
    fn drop(&mut self) {
        // Before build 26100 this can block until output is drained. Normal
        // disposal runs it concurrently with reading; failed startup closes the
        // read end first. Keep the API module loaded through this final call.
        // https://learn.microsoft.com/en-us/windows/console/closepseudoconsole
        unsafe { (self.api.close)(self.handle) }
    }
}

// The ConPTY handle can be sent between threads.
unsafe impl Send for Conpty {}

/// Own initialized, pointer-aligned storage until DeleteProcThreadAttributeList.
struct AttributeList(Box<[usize]>);

impl AttributeList {
    fn new() -> Result<Self> {
        let mut size = 0;
        fault(Failure::AttributeSize)?;
        // SAFETY: the documented sizing call takes a null list and writes size.
        let success =
            unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut size) };
        let error = Error::last_os_error();
        if success != 0
            || error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
            || size == 0
        {
            return Err(error);
        }
        let mut storage = vec![0usize; size.div_ceil(mem::size_of::<usize>())].into_boxed_slice();
        fault(Failure::AttributeInit)?;
        // SAFETY: storage is pointer-aligned and at least size bytes long.
        if unsafe {
            InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), 1, 0, &mut size)
        } == 0
        {
            return Err(Error::last_os_error());
        }
        Ok(Self(storage))
    }

    fn as_mut_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.0.as_mut_ptr().cast()
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        // SAFETY: only successful initialization constructs this owner. Delete
        // runs exactly once, before the backing allocation is deallocated.
        unsafe {
            DeleteProcThreadAttributeList(self.as_mut_ptr());
        }
    }
}

/// Before pipe workers exist, startup failure closes the output read end first.
/// Microsoft explicitly permits this instead of draining during ConPTY close.
/// Never drop a live ConPTY with an unread, open synchronous output pipe.
struct Startup {
    output: Option<miow::pipe::AnonRead>,
    backend: Option<Conpty>,
    process: Option<Arc<OwnedHandle>>,
    writer: Option<UnblockedWriter<miow::pipe::AnonWrite>>,
}

impl Drop for Startup {
    fn drop(&mut self) {
        if let Some(process) = &self.process {
            // SAFETY: a successful CreateProcessW produced this owned handle.
            // No client is returned on failure; terminate the owned root child.
            unsafe {
                TerminateProcess(process.as_raw_handle() as HANDLE, 1);
            }
        }
        drop(self.output.take());
        drop(self.backend.take());
        if let Some(writer) = &mut self.writer {
            writer.close_and_join();
        }
        if let Some(process) = &self.process {
            // SAFETY: termination is asynchronous; retain ownership through wait.
            unsafe {
                WaitForSingleObject(process.as_raw_handle() as HANDLE, INFINITE);
            }
        }
    }
}

// Test-only, thread-local injection: no environment switches, shared production
// state, or checks on the I/O hot path. Release builds optimize fault() away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Failure {
    OutputPipe,
    InputPipe,
    Console,
    AttributeSize,
    AttributeInit,
    AttributeUpdate,
    Process,
    Wait,
    ReaderWorker,
    WriterWorker,
    CloseWorker,
    Resume,
}

pub(super) fn fault(point: Failure) -> Result<()> {
    #[cfg(test)]
    if tests::FAILURE.with(|failure| failure.get() == Some(point)) {
        // Give an incorrectly running child time to publish its synthetic side
        // effect before faulting. This delay exists only in the regression.
        tests::FAILURE_DELAY.with(|delay| std::thread::sleep(delay.get()));
        return Err(Error::from_raw_os_error(5));
    }
    let _ = point;
    Ok(())
}

fn hresult(result: HRESULT) -> Result<()> {
    if result >= 0 {
        Ok(())
    } else {
        // HRESULT is not GetLastError. Preserve the actual HRESULT, including
        // non-Win32 facilities, without misinterpreting it as an OS error code.
        Err(Error::other(format!("ConPTY API failed (HRESULT 0x{:08X})", result as u32)))
    }
}

pub fn new(config: &Options, window_size: WindowSize) -> Result<Pty> {
    let api = ConptyApi::new()?;
    let mut pty_handle = 0;
    fault(Failure::OutputPipe)?;
    let (conout, conout_pty_handle) = miow::pipe::anonymous(0)?;
    fault(Failure::InputPipe)?;
    let (conin_pty_handle, conin) = miow::pipe::anonymous(0)?;
    fault(Failure::Console)?;
    // SAFETY: both pipe endpoints remain owned through this call; only a
    // successful HRESULT yields an HPCON which must be closed.
    hresult(unsafe {
        (api.create)(
            window_size.into(),
            conin_pty_handle.as_raw_handle() as HANDLE,
            conout_pty_handle.as_raw_handle() as HANDLE,
            0,
            &mut pty_handle,
        )
    })?;
    let mut startup = Startup {
        output: Some(conout),
        backend: Some(Conpty { handle: pty_handle, api }),
        process: None,
        writer: None,
    };
    // ConPTY owns copies. Parent copies must not prevent EOF on any error path.
    drop(conin_pty_handle);
    drop(conout_pty_handle);

    let mut attributes = AttributeList::new()?;
    fault(Failure::AttributeUpdate)?;
    // SAFETY: initialized attribute list and live HPCON outlive CreateProcessW.
    if unsafe {
        UpdateProcThreadAttribute(
            attributes.as_mut_ptr(),
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            pty_handle as *mut _,
            mem::size_of::<HPCON>(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(Error::last_os_error());
    }
    // SAFETY: zero is valid for unused STARTUPINFOEXW/PROCESS_INFORMATION fields.
    let mut startup_info_ex: STARTUPINFOEXW = unsafe { mem::zeroed() };
    startup_info_ex.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
    startup_info_ex.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup_info_ex.lpAttributeList = attributes.as_mut_ptr();

    // CreateProcessW may modify its command line; use genuinely mutable storage.
    let mut cmdline = win32_string(&cmdline(config));
    let cwd = config.working_directory.as_ref().map(win32_string);
    // No shell code or descendant may run while fallible setup can still
    // reject the session. Startup rollback then owns only a suspended root.
    let mut creation_flags = EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED;
    let custom_env_block = convert_custom_env(&config.env);
    let environment = match &custom_env_block {
        Some(block) => {
            creation_flags |= CREATE_UNICODE_ENVIRONMENT;
            block.as_ptr() as *mut std::ffi::c_void
        },
        None => ptr::null_mut(),
    };
    let mut proc_info: PROCESS_INFORMATION = unsafe { mem::zeroed() };
    fault(Failure::Process)?;
    // SAFETY: UTF-16 buffers are terminated, the command line is writable, and
    // the initialized attribute list and HPCON remain live for the entire call.
    if unsafe {
        CreateProcessW(
            ptr::null(),
            cmdline.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            creation_flags,
            environment,
            cwd.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
            &startup_info_ex.StartupInfo,
            &mut proc_info,
        )
    } == 0
    {
        return Err(Error::last_os_error());
    }
    // SAFETY: successful CreateProcessW transfers these two distinct handles.
    // Keep the primary thread suspended through wait/pipe-worker setup and
    // retain its handle for resume. Startup rollback retains the process.
    let process = unsafe { Arc::new(OwnedHandle::from_raw_handle(proc_info.hProcess)) };
    let thread = unsafe { OwnedHandle::from_raw_handle(proc_info.hThread) };
    startup.process = Some(process.clone());
    let child_watcher = ChildExitWatcher::new(process)?;
    drop(attributes);

    startup.writer = Some(UnblockedWriter::new(conin, PIPE_CAPACITY)?);
    let conout = UnblockedReader::new(startup.output.take().unwrap(), PIPE_CAPACITY)?;
    let conin = startup.writer.take().unwrap();
    let backend = startup.backend.take().unwrap();
    startup.process.take();
    let pty = Pty::new(backend, conout, conin, child_watcher);
    // From here any resume failure disposes a fully owned PTY with concurrent
    // output draining, rather than closing a live client's raw pipe too early.
    fault(Failure::Resume)?;
    // SAFETY: we own this newly created thread's handle with SUSPEND_RESUME
    // access. Its only suspend count was set by CREATE_SUSPENDED above.
    if unsafe { ResumeThread(thread.as_raw_handle() as HANDLE) } == u32::MAX {
        return Err(Error::last_os_error());
    }
    Ok(pty)
}

// Windows environment variables are case-insensitive, and the caller is responsible for
// deduplicating environment variables, so do that here while converting.
//
// https://learn.microsoft.com/en-us/previous-versions/troubleshoot/windows/win32/createprocess-cannot-eliminate-duplicate-variables#environment-variables
fn convert_custom_env(custom_env: &HashMap<String, String>) -> Option<Vec<u16>> {
    // Windows inherits parent's env when no `lpEnvironment` parameter is specified.
    if custom_env.is_empty() {
        return None;
    }

    let mut converted_block = Vec::new();
    let mut all_env_keys = HashSet::new();
    for (custom_key, custom_value) in custom_env {
        let custom_key_os = OsStr::new(custom_key);
        if all_env_keys.insert(custom_key_os.to_ascii_uppercase()) {
            add_windows_env_key_value_to_block(
                &mut converted_block,
                custom_key_os,
                OsStr::new(&custom_value),
            );
        } else {
            warn!(
                "Omitting environment variable pair with duplicate key: \
                 '{custom_key}'"
            );
        }
    }

    // Pull the current process environment after, to avoid overwriting the user provided one.
    for (inherited_key, inherited_value) in std::env::vars_os() {
        if all_env_keys.insert(inherited_key.to_ascii_uppercase()) {
            add_windows_env_key_value_to_block(
                &mut converted_block,
                &inherited_key,
                &inherited_value,
            );
        }
    }

    converted_block.push(0);
    Some(converted_block)
}

// According to the `lpEnvironment` parameter description:
// https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessa#parameters
//
// > An environment block consists of a null-terminated block of null-terminated strings. Each
// string is in the following form:
// >
// > name=value\0
fn add_windows_env_key_value_to_block(block: &mut Vec<u16>, key: &OsStr, value: &OsStr) {
    block.extend(key.encode_wide());
    block.push('=' as u16);
    block.extend(value.encode_wide());
    block.push(0);
}

impl OnResize for Conpty {
    fn on_resize(&mut self, window_size: WindowSize) {
        let result = unsafe { (self.api.resize)(self.handle, window_size.into()) };
        if let Err(error) = hresult(result) {
            // OnResize has no error return. Keep the existing session alive.
            warn!("ConPTY resize failed: {error}");
        }
    }
}

impl From<WindowSize> for COORD {
    fn from(window_size: WindowSize) -> Self {
        let lines = window_size.num_lines;
        let columns = window_size.num_cols;
        COORD { X: columns as i16, Y: lines as i16 }
    }
}

#[cfg(test)]
mod tests;
