//! Fixed emergency cleanup choice when the renderer cannot draw. No terminal
//! data enters this adapter, and no timer or independent executor is installed.
use std::sync::{Mutex, MutexGuard};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Wait,
    Force,
    Cancelled,
    Unavailable(Option<i32>),
}

#[derive(Default)]
struct State {
    cancelled: bool,
    window: usize,
}

#[derive(Default)]
pub(crate) struct Prompt(Mutex<State>);

impl Prompt {
    fn state(&self) -> MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn cancel(&self) {
        let mut state = self.state();
        state.cancelled = true;
        #[cfg(windows)]
        if state.window != 0 {
            // SAFETY: publication and WM_NCDESTROY clear use this same lock.
            // PostMessage is asynchronous; it never reenters the callback here.
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                    state.window as _,
                    windows_sys::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                    0,
                    0,
                );
            }
        }
    }

    pub(crate) fn show(&self) -> Decision {
        if self.state().cancelled {
            return Decision::Cancelled;
        }
        #[cfg(windows)]
        {
            windows::show(self)
        }
        #[cfg(not(windows))]
        {
            eprintln!(
                "Pike is waiting for terminal cleanup after a renderer failure. Cleanup remains pending; no automatic force close was attempted."
            );
            Decision::Unavailable(None)
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    // The byte format is documented by DLGTEMPLATE/DLGITEMTEMPLATE. Store u32
    // words so every template/control header has the required DWORD alignment.
    struct Template {
        words: Vec<u16>,
    }
    impl Template {
        fn word(&mut self, value: u16) {
            self.words.push(value);
        }
        fn dword(&mut self, value: u32) {
            self.words.extend([value as u16, (value >> 16) as u16]);
        }
        fn string(&mut self, value: &str) {
            self.words.extend(value.encode_utf16());
            self.word(0);
        }
        fn align(&mut self) {
            if self.words.len() % 2 != 0 {
                self.word(0);
            }
        }
        fn control(&mut self, class: u16, id: u16, label: &str, rect: [u16; 4], style: u32) {
            self.align();
            self.dword(WS_CHILD | WS_VISIBLE | style);
            self.dword(0);
            self.words.extend(rect);
            self.word(id);
            self.words.extend([0xffff, class]);
            self.string(label);
            self.word(0); // no creation data
        }
    }

    fn template() -> Vec<u32> {
        let mut t = Template { words: Vec::new() };
        t.dword(WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_MODALFRAME as u32);
        t.dword(0);
        t.words.extend([3, 10, 10, 260, 95]); // controls, x, y, width, height
        t.words.extend([0, 0]); // no menu, default dialog class
        t.string("Pike — terminal cleanup");
        t.control(0x82, 100, "The renderer stopped and terminal cleanup is taking longer than expected. Wait continues cleanup. Force close exits Pike before cleanup is acknowledged.", [10, 10, 240, 48], 0);
        t.control(
            0x80,
            IDOK as u16,
            "Wait",
            [80, 67, 70, 16],
            WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        );
        t.control(
            0x80,
            3,
            "Force close Pike",
            [158, 67, 92, 16],
            WS_TABSTOP | BS_PUSHBUTTON as u32,
        );
        t.align();
        t.words
            .chunks_exact(2)
            .map(|w| u32::from(w[0]) | (u32::from(w[1]) << 16))
            .collect()
    }

    // No allocation, formatting, unwrap or application callback may unwind over
    // this FFI boundary. The Prompt lives throughout the synchronous modal call.
    unsafe extern "system" fn callback(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> isize {
        if message == WM_INITDIALOG {
            let prompt = unsafe { &*(l as *const Prompt) };
            unsafe {
                SetWindowLongPtrW(window, GWLP_USERDATA, l);
            }
            let mut state = prompt.state();
            state.window = window as usize;
            let cancelled = state.cancelled;
            drop(state);
            if cancelled {
                unsafe {
                    EndDialog(window, 0);
                }
            }
            return 1; // first tab stop is the default Wait button
        }
        let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const Prompt;
        if pointer.is_null() {
            return 0;
        }
        let prompt = unsafe { &*pointer };
        match message {
            WM_COMMAND if w & 0xffff == IDOK as usize || w & 0xffff == IDCANCEL as usize => {
                unsafe {
                    EndDialog(window, 1);
                }
                1
            }
            WM_COMMAND if w & 0xffff == 3 => {
                unsafe {
                    EndDialog(window, 3);
                }
                1
            }
            WM_CLOSE => {
                unsafe {
                    EndDialog(window, 1);
                }
                1
            }
            WM_NCDESTROY => {
                prompt.state().window = 0;
                unsafe {
                    SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                }
                0
            }
            _ => 0,
        }
    }

    pub(super) fn show(prompt: &Prompt) -> Decision {
        let data = template();
        // SAFETY: data is a DWORD-aligned, bounded standard dialog template.
        // Both it and Prompt outlive all callbacks, including WM_NCDESTROY.
        // Only system-provided standard controls are used; no v6 activation.
        let result = unsafe {
            DialogBoxIndirectParamW(
                std::ptr::null_mut(),
                data.as_ptr().cast(),
                std::ptr::null_mut(),
                Some(callback),
                prompt as *const Prompt as isize,
            )
        };
        let mut state = prompt.state();
        state.window = 0;
        if state.cancelled {
            Decision::Cancelled
        } else {
            match result {
                1 => Decision::Wait,
                3 => Decision::Force,
                _ => Decision::Unavailable(std::io::Error::last_os_error().raw_os_error()),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn fixed_template_is_aligned_and_wait_is_default_before_force() {
            let words = template();
            assert_eq!(words.as_ptr() as usize % 4, 0);
            let utf16: Vec<_> = words
                .into_iter()
                .flat_map(|w| [w as u16, (w >> 16) as u16])
                .collect();
            let text = String::from_utf16_lossy(&utf16);
            assert!(text.find("Wait\0").unwrap() < text.find("Force close Pike\0").unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_prompt_cancellation_prevents_native_open_and_is_idempotent() {
        let prompt = Prompt::default();
        prompt.cancel();
        prompt.cancel();
        assert_eq!(prompt.show(), Decision::Cancelled);
        assert_eq!(prompt.state().window, 0);
    }
    #[test]
    fn poison_recovery_keeps_cancellation_available() {
        let prompt = std::sync::Arc::new(Prompt::default());
        let other = prompt.clone();
        let _ = std::thread::spawn(move || {
            let _guard = other.0.lock().unwrap();
            panic!("synthetic poison");
        })
        .join();
        prompt.cancel();
        assert_eq!(prompt.show(), Decision::Cancelled);
    }
}
