#[derive(Debug, Clone, PartialEq, Default)]
pub enum Action {
    /// The terminal model exited; backend resources may still be closing.
    Shutdown {
        exit_code: Option<i32>,
    },
    ChildExit(i32),
    BackendCompleted(crate::BackendCompletion),
    ChangeTitle(String),
    ResetTitle,
    Bell,
    MouseCursorDirty,
    /// Cursor blink mode is exposed; the adapter does not install a blink timer.
    CursorBlinkingChanged(bool),
    EventBridgeError(crate::EventBridgeError),
    InputRejected(crate::InputError),
    PasteRejected(crate::PasteError),
    ImeRejected(crate::ImeError),
    #[default]
    Ignore,
}
