//! Last-resort fixed report when the renderer cannot display recovery controls.
//! No driver/OS error strings, arguments, environment or terminal data enter it.

pub fn renderer_failure() {
    eprintln!("{}", crate::errors::renderer_report());
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let title: Vec<u16> = "Pike\0".encode_utf16().collect();
        let message: Vec<u16> = crate::errors::renderer_report()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: Both fixed, NUL-terminated UTF-16 buffers remain alive for the
        // synchronous call. A null owner is valid before any window exists; no
        // borrowed native handles or callbacks are involved. Release builds use
        // windows_subsystem, so stderr alone cannot provide a visible report.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}
