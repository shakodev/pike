pub mod actions;
pub mod bindings;
pub mod settings;

mod backend;
mod event_bridge;
mod font;
mod lifecycle;
mod terminal;
mod theme;
mod view;

pub use alacritty_terminal::event::Event as AlacrittyEvent;
pub use alacritty_terminal::index::Point as AlacrittyPoint;
pub use alacritty_terminal::input::{
    InputError, InputStatus, INPUT_BYTE_LIMIT,
};
pub use alacritty_terminal::selection::SelectionType;
pub use alacritty_terminal::term::TermMode;
#[cfg(windows)]
pub use alacritty_terminal::tty::LaunchError;
pub use backend::Command as BackendCommand;
pub use backend::{LinkAction, MouseButton, RenderableContent};
pub use event_bridge::{EventBridgeError, EventBridgeEvent};
pub use lifecycle::{BackendCompletion, ExitReason, ShutdownHandle};
pub use terminal::{Command, Event, Terminal};
pub use theme::{ColorPalette, Theme};
pub use view::TerminalView;

#[cfg(test)]
mod lifecycle_tests;
