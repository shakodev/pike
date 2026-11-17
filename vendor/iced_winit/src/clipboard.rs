//! Access the clipboard.

use crate::core::clipboard::Kind;
use std::sync::Arc;
use winit::window::{Window, WindowId};

/// A buffer for short-term storage and transfer within and between
/// applications.
pub struct Clipboard {
    state: State,
}

enum State {
    Connected {
        clipboard: window_clipboard::Clipboard,
        // Held until drop to satisfy the safety invariants of
        // `window_clipboard::Clipboard`.
        //
        // Note that the field ordering is load-bearing.
        #[allow(dead_code)]
        window: Arc<Window>,
    },
    Unavailable,
}

impl Clipboard {
    /// Creates a new [`Clipboard`] for the given window.
    pub fn connect(window: Arc<Window>) -> Clipboard {
        // SAFETY: The window handle will stay alive throughout the entire
        // lifetime of the `window_clipboard::Clipboard` because we hold
        // the `Arc<Window>` together with `State`, and enum variant fields
        // get dropped in declaration order.
        #[allow(unsafe_code)]
        let clipboard =
            unsafe { window_clipboard::Clipboard::connect(&window) };

        let state = match clipboard {
            Ok(clipboard) => State::Connected { clipboard, window },
            Err(_) => State::Unavailable,
        };

        Clipboard { state }
    }

    /// Creates a new [`Clipboard`] that isn't associated with a window.
    /// This clipboard will never contain a copied value.
    pub fn unconnected() -> Clipboard {
        Clipboard {
            state: State::Unavailable,
        }
    }

    /// Reads the current content of the [`Clipboard`] as text.
    pub fn read(&self, kind: Kind) -> Option<String> {
        match &self.state {
            State::Connected { clipboard, .. } => match kind {
                Kind::Standard => clipboard.read().ok(),
                Kind::Primary => clipboard.read_primary().and_then(Result::ok),
            },
            State::Unavailable => None,
        }
    }

    /// Writes the given text contents to the [`Clipboard`].
    pub fn write(&mut self, kind: Kind, contents: String) {
        let _ = self.try_write(kind, contents);
    }

    /// Use the existing native backend and return metadata only. Raw platform
    /// errors may contain user values, so they are neither retained nor logged.
    pub fn try_write(
        &mut self,
        kind: Kind,
        contents: String,
    ) -> Result<(), crate::runtime::clipboard::WriteError> {
        use crate::runtime::clipboard::WriteError;
        match &mut self.state {
            State::Connected { clipboard, .. } => {
                let result = match kind {
                    Kind::Standard => clipboard.write(contents),
                    Kind::Primary => {
                        clipboard.write_primary(contents).unwrap_or(Ok(()))
                    },
                };

                write_result(result)
            },
            State::Unavailable => Err(WriteError::Unavailable),
        }
    }

    /// Returns the identifier of the window used to create the [`Clipboard`], if any.
    pub fn window_id(&self) -> Option<WindowId> {
        match &self.state {
            State::Connected { window, .. } => Some(window.id()),
            State::Unavailable => None,
        }
    }
}

fn write_result<E>(
    result: Result<(), E>,
) -> Result<(), crate::runtime::clipboard::WriteError> {
    result.map_err(|_| crate::runtime::clipboard::WriteError::WriteFailed)
}

pub(crate) fn handle(
    action: crate::runtime::clipboard::Action,
    clipboard: &mut Clipboard,
) {
    use crate::runtime::clipboard::Action;
    match action {
        Action::Read { target, channel } => {
            let _ = channel.send(clipboard.read(target));
        },
        Action::Write { target, contents } => clipboard.write(target, contents),
        Action::WriteWithResult {
            target,
            contents,
            channel,
        } => {
            let _ = channel.send(clipboard.try_write(target, contents));
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        futures::futures::channel::oneshot,
        runtime::clipboard::{Action, WriteError},
    };

    #[test]
    fn unavailable_native_writer_is_acknowledged_without_readback_or_raw_values()
     {
        let mut clipboard = Clipboard::unconnected();
        let (channel, mut receiver) = oneshot::channel();
        let action = Action::WriteWithResult {
            target: Kind::Standard,
            contents: "SECRET_CLIPBOARD34".into(),
            channel,
        };
        assert!(!format!("{action:?}").contains("SECRET_CLIPBOARD34"));
        handle(action, &mut clipboard);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Some(Err(WriteError::Unavailable))
        );
    }

    #[test]
    fn native_failure_mapping_discards_platform_context() {
        let result =
            write_result(Err(std::io::Error::other("SECRET_NATIVE34")));
        assert_eq!(result, Err(WriteError::WriteFailed));
        assert!(!format!("{result:?}").contains("SECRET_NATIVE34"));
        assert_eq!(write_result::<std::io::Error>(Ok(())), Ok(()));
    }
}

impl crate::core::Clipboard for Clipboard {
    fn read(&self, kind: Kind) -> Option<String> {
        self.read(kind)
    }

    fn write(&mut self, kind: Kind, contents: String) {
        self.write(kind, contents);
    }
}
