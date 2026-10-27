use std::borrow::Cow;
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

use crate::term::ClipboardType;
use crate::vte::ansi::Rgb;

/// Maximum retained UTF-8 bytes in one terminal-supplied title.
pub const TITLE_BYTE_LIMIT: usize = 4096;

/// Bound presentation metadata and remove controls/bidi overrides before it is
/// retained by the model, title stack, bridge or application.
pub fn sanitize_title(title: &str) -> String {
    let mut clean = String::with_capacity(title.len().min(TITLE_BYTE_LIMIT));
    for ch in title.chars() {
        if ch.is_control()
            || matches!(ch, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            continue;
        }
        if clean.len() + ch.len_utf8() > TITLE_BYTE_LIMIT {
            break;
        }
        clean.push(ch);
    }
    clean
}

/// Terminal event.
///
/// These events instruct the UI over changes that can't be handled by the terminal emulation layer
/// itself.
#[derive(Clone)]
pub enum Event {
    /// Grid has changed possibly requiring a mouse cursor shape change.
    MouseCursorDirty,

    /// Window title change.
    Title(String),

    /// Reset to the default window title.
    ResetTitle,

    /// Request to store a text string in the clipboard.
    ClipboardStore(ClipboardType, String),

    /// Request to write the contents of the clipboard to the PTY.
    ///
    /// The attached function is a formatter which will correctly transform the clipboard content
    /// into the expected escape sequence format.
    ClipboardLoad(ClipboardType, Arc<dyn Fn(&str) -> String + Sync + Send + 'static>),

    /// Request to write the RGB value of a color to the PTY.
    ///
    /// The attached function is a formatter which will correctly transform the RGB color into the
    /// expected escape sequence format.
    ColorRequest(usize, Arc<dyn Fn(Rgb) -> String + Sync + Send + 'static>),

    /// Write some text to the PTY.
    PtyWrite(String),

    /// Request to write the text area size.
    TextAreaSizeRequest(Arc<dyn Fn(TextAreaSize) -> String + Sync + Send + 'static>),

    /// Cursor blinking state has changed.
    CursorBlinkingChange,

    /// New terminal content available.
    Wakeup,

    /// Terminal bell ring.
    Bell,

    /// Shutdown request.
    Exit,

    /// Child process exited with an error code.
    ChildExit(i32),
}

impl Debug for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Event::ClipboardStore(ty, _) => write!(f, "ClipboardStore({ty:?}, [redacted])"),
            Event::ClipboardLoad(ty, _) => write!(f, "ClipboardLoad({ty:?})"),
            Event::TextAreaSizeRequest(_) => write!(f, "TextAreaSizeRequest"),
            Event::ColorRequest(index, _) => write!(f, "ColorRequest({index})"),
            Event::PtyWrite(_) => write!(f, "PtyWrite([redacted])"),
            Event::Title(_) => write!(f, "Title([redacted])"),
            Event::CursorBlinkingChange => write!(f, "CursorBlinkingChange"),
            Event::MouseCursorDirty => write!(f, "MouseCursorDirty"),
            Event::ResetTitle => write!(f, "ResetTitle"),
            Event::Wakeup => write!(f, "Wakeup"),
            Event::Bell => write!(f, "Bell"),
            Event::Exit => write!(f, "Exit"),
            Event::ChildExit(code) => write!(f, "ChildExit({code})"),
        }
    }
}

/// Byte sequences are sent to a `Notify` in response to some events.
pub trait Notify {
    /// Notify that an escape sequence should be written to the PTY.
    ///
    /// TODO this needs to be able to error somehow.
    fn notify<B: Into<Cow<'static, [u8]>>>(&self, _: B);
}

#[derive(Copy, Clone, Debug)]
pub struct WindowSize {
    pub num_lines: u16,
    pub num_cols: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

/// Aggregate virtual grid pixels for a protocol response, independent of the
/// native PTY's integer cell metadata and the monitor's physical DPI.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TextAreaSize {
    pub width: u32,
    pub height: u32,
}

/// Types that are interested in when the display is resized.
pub trait OnResize {
    fn on_resize(&mut self, window_size: WindowSize);
}

/// Event Loop for notifying the renderer about terminal events.
pub trait EventListener {
    fn send_event(&self, _event: Event) {}
}

/// Null sink for events.
pub struct VoidListener;

impl EventListener for VoidListener {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_sanitization_bounds_utf8_controls_and_retained_capacity() {
        let title = format!("safe\n\t\u{7f}\u{009b}\u{202e}\u{2066}{}", "🦀".repeat(10_000));
        let clean = sanitize_title(&title);
        assert!(clean.starts_with("safe🦀"));
        assert_eq!(clean.len(), TITLE_BYTE_LIMIT);
        assert_eq!(clean.capacity(), TITLE_BYTE_LIMIT);
        assert!(!clean.chars().any(char::is_control));
    }

    #[test]
    fn event_debug_never_contains_terminal_title_reply_or_clipboard_contents() {
        for event in [
            Event::Title("private-marker".into()),
            Event::PtyWrite("private-marker".into()),
            Event::ClipboardStore(ClipboardType::Clipboard, "private-marker".into()),
        ] {
            let debug = format!("{event:?}");
            assert!(!debug.contains("private-marker"));
            assert!(debug.contains("redacted"));
        }
    }
}
