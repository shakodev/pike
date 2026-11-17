//! Access the clipboard.
use crate::core::clipboard::Kind;
use crate::futures::futures::channel::oneshot;
use crate::task::{self, Task};

/// Metadata only; never contains clipboard contents or platform error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    /// No native clipboard is connected to the window.
    Unavailable,
    /// The existing native clipboard backend refused the write.
    WriteFailed,
}

/// A clipboard action to be performed by some [`Task`].
///
/// [`Task`]: crate::Task
pub enum Action {
    /// Read the clipboard and produce `T` with the result.
    Read {
        /// The clipboard target.
        target: Kind,
        /// The channel to send the read contents.
        channel: oneshot::Sender<Option<String>>,
    },

    /// Write the given contents to the clipboard.
    Write {
        /// The clipboard target.
        target: Kind,
        /// The contents to be written.
        contents: String,
    },

    /// Write text and acknowledge the actual native backend result.
    WriteWithResult {
        /// The clipboard target.
        target: Kind,
        /// The contents, never included in diagnostic formatting.
        contents: String,
        /// The channel for metadata-only success or failure.
        channel: oneshot::Sender<Result<(), WriteError>>,
    },
}

/// Read the current contents of the clipboard.
pub fn read() -> Task<Option<String>> {
    task::oneshot(|channel| {
        crate::Action::Clipboard(Action::Read {
            target: Kind::Standard,
            channel,
        })
    })
}

/// Read the current contents of the primary clipboard.
pub fn read_primary() -> Task<Option<String>> {
    task::oneshot(|channel| {
        crate::Action::Clipboard(Action::Read {
            target: Kind::Primary,
            channel,
        })
    })
}

/// Write the given contents to the clipboard.
pub fn write<T>(contents: String) -> Task<T> {
    task::effect(crate::Action::Clipboard(Action::Write {
        target: Kind::Standard,
        contents,
    }))
}

/// Write text with an actual metadata-only native acknowledgement. This uses
/// the same clipboard backend as [`write`], without a readback or another task.
pub fn write_with_result(contents: String) -> Task<Result<(), WriteError>> {
    task::oneshot(|channel| {
        crate::Action::Clipboard(Action::WriteWithResult {
            target: Kind::Standard,
            contents,
            channel,
        })
    })
}

/// Write the given contents to the primary clipboard.
pub fn write_primary<Message>(contents: String) -> Task<Message> {
    task::effect(crate::Action::Clipboard(Action::Write {
        target: Kind::Primary,
        contents,
    }))
}

// A runtime Action may be formatted by host debugging. Keep even that boundary
// metadata-only; never expose clipboard values through a derived Debug.
impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { target, .. } => f
                .debug_struct("Read")
                .field("target", target)
                .finish_non_exhaustive(),
            Self::Write { target, contents } => f
                .debug_struct("Write")
                .field("target", target)
                .field("bytes", &contents.len())
                .finish_non_exhaustive(),
            Self::WriteWithResult {
                target, contents, ..
            } => f
                .debug_struct("WriteWithResult")
                .field("target", target)
                .field("bytes", &contents.len())
                .finish_non_exhaustive(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::futures::futures::task::noop_waker;
    use crate::{Action as RuntimeAction, task::into_stream};
    use std::task::{Context, Poll};

    #[test]
    fn acknowledged_write_is_one_native_action_and_metadata_only_result() {
        for result in [
            Ok(()),
            Err(WriteError::Unavailable),
            Err(WriteError::WriteFailed),
        ] {
            let task = write_with_result("SECRET_CLIPBOARD34".into());
            assert_eq!(task.units(), 1);
            let mut stream = into_stream(task).unwrap();
            let waker = noop_waker();
            let mut cx = Context::from_waker(&waker);
            let Poll::Ready(Some(RuntimeAction::Clipboard(action))) =
                stream.as_mut().poll_next(&mut cx)
            else {
                panic!("native write")
            };
            assert!(!format!("{action:?}").contains("SECRET_CLIPBOARD34"));
            let Action::WriteWithResult {
                target,
                contents,
                channel,
            } = action
            else {
                panic!("acknowledged write")
            };
            assert_eq!(target, Kind::Standard);
            assert_eq!(contents, "SECRET_CLIPBOARD34");
            assert!(stream.as_mut().poll_next(&mut cx).is_pending());
            channel.send(result).unwrap();
            assert!(
                matches!(stream.as_mut().poll_next(&mut cx), Poll::Ready(Some(RuntimeAction::Output(receipt))) if receipt == result)
            );
            assert!(matches!(
                stream.as_mut().poll_next(&mut cx),
                Poll::Ready(None)
            ));
        }
    }

    #[test]
    fn legacy_write_debug_is_redacted_and_canceled_acknowledgement_ends() {
        let action = Action::Write {
            target: Kind::Standard,
            contents: "SECRET_LEGACY34".into(),
        };
        assert!(!format!("{action:?}").contains("SECRET_LEGACY34"));
        let mut stream =
            into_stream(write_with_result("SECRET_CANCEL34".into())).unwrap();
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        drop(stream.as_mut().poll_next(&mut cx));
        assert!(matches!(
            stream.as_mut().poll_next(&mut cx),
            Poll::Ready(None)
        ));
    }
}
