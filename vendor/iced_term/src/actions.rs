#[derive(Debug, Clone, PartialEq, Default)]
pub enum Action {
    Shutdown,
    ChangeTitle(String),
    EventBridgeError(crate::EventBridgeError),
    #[default]
    Ignore,
}
