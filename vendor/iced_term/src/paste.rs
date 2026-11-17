//! Clipboard policy, independent of the renderer and the platform clipboard.
use crate::InputError;
use std::fmt;
use std::sync::{Arc, Mutex, Weak};

/// Maximum original clipboard UTF-8 bytes, before filtering or framing.
pub const PASTE_BYTE_LIMIT: usize = 1024 * 1024;
const START: &[u8] = b"\x1b[200~";
const END: &[u8] = b"\x1b[201~";

/// Metadata only; clipboard contents are never included in an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteError {
    ClipboardUnavailable,
    Pending,
    WrongSession,
    TooLarge { bytes: usize, limit: usize },
    Input(InputError),
}

impl fmt::Display for PasteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClipboardUnavailable => {
                f.write_str("Clipboard text is unavailable. Copy text and try again.")
            },
            Self::Pending => f.write_str("Paste was not sent: another paste is awaiting processing. Try again shortly."),
            Self::WrongSession => f.write_str("Paste was not sent: the target session changed."),
            Self::TooLarge { bytes, limit } => write!(
                f,
                "Paste was not sent: {bytes} clipboard bytes exceed the {limit}-byte limit."
            ),
            Self::Input(InputError::Full { requested, available }) => write!(
                f,
                "Paste was not sent: {requested} input bytes requested, only {available} available. Wait for the program to read input and try again."
            ),
            Self::Input(error) => write!(f, "Paste was not sent: {error}"),
        }
    }
}

impl std::error::Error for PasteError {}

impl From<InputError> for PasteError {
    fn from(error: InputError) -> Self {
        Self::Input(error)
    }
}

struct Request {
    owner: u64,
    text: Mutex<Option<Box<str>>>,
}

/// Owner-bound request, not accepted input. Clones share one consumable payload.
/// Debug is metadata only. Dropping the last clone releases the pending slot;
/// cancel invalidates every clone without writing anything.
#[derive(Clone)]
pub struct PasteRequest(Arc<Request>);

impl PasteRequest {
    pub fn session_id(&self) -> u64 {
        self.0.owner
    }

    pub fn cancel(&self) {
        self.0.text.lock().unwrap_or_else(|e| e.into_inner()).take();
    }

    pub(crate) fn take(
        &self,
        owner: u64,
    ) -> Result<Option<Box<str>>, PasteError> {
        if owner != self.0.owner {
            self.cancel();
            return Err(PasteError::WrongSession);
        }
        Ok(self.0.text.lock().unwrap_or_else(|e| e.into_inner()).take())
    }
}

impl fmt::Debug for PasteRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = self.0.text.lock().unwrap_or_else(|e| e.into_inner());
        f.debug_struct("PasteRequest")
            .field("session", &self.0.owner)
            .field("pending_bytes", &text.as_ref().map_or(0, |s| s.len()))
            .finish()
    }
}

/// One separately bounded, unaccepted clipboard request per session. The slot
/// owns only a Weak pointer: stale/dropped UI messages cannot retain contents.
#[derive(Default)]
pub(crate) struct PasteSlot(Mutex<Weak<Request>>);

impl PasteSlot {
    pub(crate) fn stage(
        &self,
        owner: u64,
        text: String,
    ) -> Result<PasteRequest, PasteError> {
        if text.len() > PASTE_BYTE_LIMIT {
            return Err(PasteError::TooLarge {
                bytes: text.len(),
                limit: PASTE_BYTE_LIMIT,
            });
        }
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if slot.upgrade().is_some_and(|old| {
            old.text.lock().unwrap_or_else(|e| e.into_inner()).is_some()
        }) {
            return Err(PasteError::Pending);
        }
        let request = Arc::new(Request {
            owner,
            text: Mutex::new(Some(text.into_boxed_str())),
        });
        *slot = Arc::downgrade(&request);
        Ok(PasteRequest(request))
    }

    pub(crate) fn cancel(&self) {
        let request =
            self.0.lock().unwrap_or_else(|e| e.into_inner()).upgrade();
        if let Some(request) = request {
            PasteRequest(request).cancel();
        }
    }
}

// Normalize CRLF and lone CR/LF to LF in bracketed mode, CR otherwise.
// Preserve tabs and printable Unicode; strip other C0/C1 controls and DEL in
// both modes. In particular neither ESC nor ETX can break the paste framing.
fn characters(text: &str, bracketed: bool) -> impl Iterator<Item = char> + '_ {
    let mut chars = text.chars().peekable();
    std::iter::from_fn(move || loop {
        let c = chars.next()?;
        return match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                Some(if bracketed { '\n' } else { '\r' })
            },
            '\n' => Some(if bracketed { '\n' } else { '\r' }),
            '\t' => Some(c),
            c if c.is_control() => continue,
            c => Some(c),
        };
    })
}

pub(crate) fn wire_len(
    text: &str,
    bracketed: bool,
) -> Result<usize, PasteError> {
    if text.len() > PASTE_BYTE_LIMIT {
        return Err(PasteError::TooLarge {
            bytes: text.len(),
            limit: PASTE_BYTE_LIMIT,
        });
    }
    let body: usize = characters(text, bracketed).map(char::len_utf8).sum();
    // Empty/fully filtered pastes emit nothing, including no empty markers.
    Ok(body + usize::from(bracketed && body != 0) * (START.len() + END.len()))
}

/// Check the entire wire size before allocating or writing any encoded bytes.
/// The actual sender must still admit the returned Vec in one atomic call.
pub(crate) fn prepare(
    text: &str,
    bracketed: bool,
    preflight: impl FnOnce(usize) -> Result<(), InputError>,
) -> Result<Vec<u8>, PasteError> {
    let len = wire_len(text, bracketed)?;
    preflight(len)?;
    let mut bytes = Vec::with_capacity(len);
    if len == 0 {
        return Ok(bytes);
    }
    if bracketed {
        bytes.extend_from_slice(START);
    }
    let mut utf8 = [0; 4];
    for c in characters(text, bracketed) {
        bytes.extend_from_slice(c.encode_utf8(&mut utf8).as_bytes());
    }
    if bracketed {
        bytes.extend_from_slice(END);
    }
    debug_assert_eq!(bytes.len(), len);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_bytes_modes_newlines_unicode_and_controls() {
        let input =
            "α界🦀e\u{301}\r\nB\nC\rD\t\0\x03\x1b[201~E\x1b[200~\x08\x7f\u{85}";
        for (mode, expected) in [
            (false, "α界🦀e\u{301}\rB\rC\rD\t[201~E[200~"),
            (
                true,
                "\x1b[200~α界🦀e\u{301}\nB\nC\nD\t[201~E[200~\x1b[201~",
            ),
        ] {
            let bytes = prepare(input, mode, |_| Ok(())).unwrap();
            assert_eq!(bytes, expected.as_bytes());
            assert!(!bytes.ends_with(b"\r"), "no appended Enter");
        }
        assert_eq!(prepare("", true, |_| Ok(())).unwrap(), b"");
        assert_eq!(prepare("\x03\x1b\0", true, |_| Ok(())).unwrap(), b"");
    }

    #[test]
    fn original_limit_and_entire_framed_budget_checked_before_encoding() {
        let text = "x".repeat(PASTE_BYTE_LIMIT);
        let mut requested = 0;
        assert_eq!(
            prepare(&text, true, |len| {
                requested = len;
                Err(InputError::Full {
                    requested: len,
                    available: len - 1,
                })
            }),
            Err(PasteError::Input(InputError::Full {
                requested: PASTE_BYTE_LIMIT + 12,
                available: PASTE_BYTE_LIMIT + 11,
            }))
        );
        assert_eq!(requested, PASTE_BYTE_LIMIT + 12);
        assert_eq!(
            prepare(&text, false, |_| Ok(())).unwrap().len(),
            PASTE_BYTE_LIMIT
        );
        let oversized = "\x1b".repeat(PASTE_BYTE_LIMIT + 1);
        assert_eq!(
            prepare(&oversized, true, |_| panic!(
                "no budget access for oversize"
            )),
            Err(PasteError::TooLarge {
                bytes: PASTE_BYTE_LIMIT + 1,
                limit: PASTE_BYTE_LIMIT
            })
        );
    }

    #[test]
    fn paste_requests_are_owner_bound_redacted_single_use_and_release_the_slot()
    {
        let slot = PasteSlot::default();
        let first = slot.stage(16, "synthetic_secret".into()).unwrap();
        let replay = first.clone();
        assert!(!format!("{first:?}").contains("synthetic_secret"));
        assert!(matches!(
            slot.stage(16, "next".into()),
            Err(PasteError::Pending)
        ));
        assert_eq!(
            first.take(16).unwrap().as_deref(),
            Some("synthetic_secret")
        );
        assert!(replay.take(16).unwrap().is_none());
        let wrong = slot.stage(16, "other".into()).unwrap();
        assert_eq!(wrong.take(17), Err(PasteError::WrongSession));
        assert!(wrong.take(16).unwrap().is_none());
        let dropped = slot.stage(16, "dropped".into()).unwrap();
        let clone = dropped.clone();
        drop(dropped);
        assert!(matches!(
            slot.stage(16, "still pending".into()),
            Err(PasteError::Pending)
        ));
        drop(clone);
        let cancelled = slot.stage(16, "cancelled".into()).unwrap();
        slot.cancel();
        assert!(cancelled.take(16).unwrap().is_none());
        assert!(slot.stage(16, "released".into()).is_ok());
    }
}
