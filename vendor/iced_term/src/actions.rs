#[derive(Debug, Clone, PartialEq, Default)]
pub enum Action {
    Shutdown,
    ChangeTitle(String),
    EventBridgeError(crate::EventBridgeError),
    InputRejected(crate::InputError),
    #[default]
    Ignore,
}
