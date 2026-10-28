pub mod actions;
pub mod bindings;
pub mod settings;

mod backend;
mod event_bridge;
mod font;
mod geometry;
mod keyboard;
mod lifecycle;
mod snapshot;
mod terminal;
mod theme;
mod view;
mod window_focus;

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
pub use backend::{LinkAction, MouseButton};
pub use event_bridge::{EventBridgeError, EventBridgeEvent};
pub use lifecycle::{BackendCompletion, ExitReason, ShutdownHandle};
pub use snapshot::{RenderableContent, SnapshotStats, ViewportSnapshot};
pub use terminal::{Command, Event, PreparedTerminal, Terminal, WorkStats};
pub use theme::{ColorPalette, Theme};
pub use view::TerminalView;
pub use window_focus::WindowFocus;

#[cfg(test)]
mod lifecycle_tests;
