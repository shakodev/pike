#[derive(Debug, Clone, PartialEq, Default)]
pub enum Action {
    /// The terminal model exited; backend resources may still be closing.
    Shutdown,
    BackendCompleted(crate::BackendCompletion),
    ChangeTitle(String),
    EventBridgeError(crate::EventBridgeError),
    InputRejected(crate::InputError),
    #[default]
    Ignore,
}
