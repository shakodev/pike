//! Effective command reference and keyboard/mouse action menu.
use super::{Command, TabMove, TabSelection, keymap};
use crate::app::resize::capture::KeyAction;
use crate::{
    app::{App, Control, Message, resize},
    session::Lifecycle,
    settings::shortcuts::{self, Command as Id},
};
use iced::advanced::widget::{Id as WidgetId, Operation, operate, operation};
use iced::{
    Element, Fill, Task,
    keyboard::{self, Key, key::Named},
    widget::{button, column, container, scrollable, text},
};
use iced::{Rectangle, Vector};

#[path = "recovery.rs"]
mod recovery;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandReference {
    pub command: Command,
    pub label: String,
    pub bindings: Vec<String>,
    pub enabled: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelpAction {
    Next,
    Previous,
    First,
    Last,
    PageUp,
    PageDown,
    Execute(Command),
    Recovery(crate::errors::ErrorId, crate::errors::Action),
    CopyDiagnostics,
    ClearDiagnostics,
}

impl App {
    /// Reads today's atomically committed settings. Disabled and unbound commands
    /// remain discoverable, including actions without a default direct chord.
    pub fn command_reference(&self) -> Vec<CommandReference> {
        let effective: Vec<_> = shortcuts::effective(self.settings.active.shortcuts()).collect();
        Id::ALL
            .iter()
            .copied()
            .map(|id| {
                let command = keymap::command(id);
                let mut label = id.label().to_owned();
                if let Command::NewProfile(index) = command
                    && let Some(name) = self.settings.active.profiles().nth(index.into())
                {
                    label.push_str(&format!(" ({})", name.as_str()));
                }
                CommandReference {
                    command,
                    label,
                    bindings: effective
                        .iter()
                        .filter(|binding| binding.command == id)
                        .map(|binding| binding.to_string())
                        .collect(),
                    enabled: self.command_enabled(command),
                }
            })
            .collect()
    }

    pub fn command_enabled(&self, command: Command) -> bool {
        if self.exiting
            || self.settings.booting
            || self.close_policy.confirming()
            || self.tab_dialog.active()
        {
            return false;
        }
        let order = self.model.workspace().order();
        let target = self.active_target();
        let terminal = self.active_session().and_then(|id| self.terminal(id));
        let lifecycle = self
            .active_session()
            .and_then(|id| self.model.sessions().get(id))
            .map(|s| s.lifecycle());
        let tab = target.and_then(|target| self.model.workspace().tab(target.tab));
        match command {
            Command::Help | Command::CloseWindow => true,
            Command::NewTab => self.model.sessions().len() < 32,
            Command::NewProfile(index) => {
                self.settings.enabled
                    && !self.settings.pending
                    && self.model.sessions().len() < 32
                    && self.settings.active.profiles().nth(index.into()).is_some()
            }
            Command::ReloadSettings => self.settings.path.is_some() && !self.settings.pending,
            Command::ListTabs => !order.is_empty(),
            Command::NextTab | Command::PreviousTab => order.len() > 1,
            Command::SelectTab(TabSelection::Index(index)) => usize::from(index) < order.len(),
            Command::SelectTab(TabSelection::Last) => !order.is_empty(),
            Command::Retry | Command::UseDefault => matches!(
                lifecycle,
                Some(Lifecycle::Failed(_) | Lifecycle::Exited { .. })
            ),
            Command::Font(_) | Command::Copy => terminal.is_some(),
            Command::Paste => {
                lifecycle == Some(Lifecycle::Running)
                    && terminal.is_some_and(|t| !t.input_status().closed)
            }
            Command::History(_) => {
                if self.command_state.help.is_some() {
                    self.command_state.help_history
                } else {
                    terminal
                        .and_then(|t| t.renderable_content())
                        .is_some_and(|c| !c.terminal_mode.contains(iced_term::TermMode::ALT_SCREEN))
                }
            }
            Command::Focus(direction) => {
                tab.is_some_and(|t| t.grid().adjacent(t.focused(), direction).is_some())
            }
            Command::MovePane(direction) => tab.is_some_and(|t| {
                t.grid().maximized().is_none()
                    && t.grid().adjacent(t.focused(), direction).is_some()
            }),
            Command::Resize(_) => {
                tab.is_some_and(|t| t.grid().len() > 1 && t.grid().maximized().is_none())
                    && terminal.is_some()
            }
            Command::Split(_) => {
                target.is_some() && terminal.is_some() && self.model.sessions().len() < 32
            }
            Command::MoveTab(direction) => self
                .model
                .workspace()
                .active()
                .and_then(|id| order.iter().position(|t| *t == id))
                .is_some_and(|i| match direction {
                    TabMove::Left => i > 0,
                    TabMove::Right => i + 1 < order.len(),
                }),
            Command::ClosePane
            | Command::CloseTab
            | Command::ToggleZoom
            | Command::RenameTab
            | Command::ResetTabName => target.is_some(),
        }
    }

    pub fn command_menu(&self) -> Element<'_, Control> {
        let bindings: Vec<_> = shortcuts::effective(self.settings.active.shortcuts())
            .filter(|binding| binding.command == Id::Help)
            .map(|binding| binding.to_string())
            .collect();
        let label = match bindings.as_slice() {
            [] => "Commands and shortcuts".into(),
            [only] => format!("Commands and shortcuts ({only})"),
            [first, rest @ ..] => format!("Commands and shortcuts ({first}; {} more)", rest.len()),
        };
        crate::ui::icon_maybe(
            "≡",
            label,
            (!self.interaction_blocked()).then_some(Control::Command(Command::Help)),
        )
    }

    pub(in crate::app) fn open_help(&mut self) -> Task<Message> {
        let Some(token) = self.command_state.help_token.checked_add(1) else {
            return Task::none();
        };
        self.command_state.help_token = token;
        self.command_state.help_generation = self.settings.generation;
        self.command_state.help_window = self.window_focus.input_lease();
        self.command_state.help_history = self
            .active_session()
            .and_then(|id| self.terminal(id))
            .and_then(|t| t.renderable_content())
            .is_some_and(|c| !c.terminal_mode.contains(iced_term::TermMode::ALT_SCREEN));
        self.command_state.help = Some(self.active_session());
        self.command_state.help_selected = 0;
        self.command_state.help_scroll = Some(WidgetId::unique());
        self.command_state.help_actions = self.menu_actions();
        self.command_state.help_rows = self
            .command_state
            .help_actions
            .iter()
            .map(|_| WidgetId::unique())
            .collect();
        self.synchronize();
        Task::none()
    }

    /// Reloads can complete while the menu is open. Invalidate old rendered
    /// receipts while leaving the current menu and its selected row usable.
    pub(in crate::app) fn refresh_help_reference(&mut self) {
        if self.command_state.help.is_none() {
            return;
        }
        if let Some(token) = self.command_state.help_token.checked_add(1) {
            self.command_state.help_token = token;
            self.command_state.help_generation = self.settings.generation;
            self.command_state.help_window = self.window_focus.input_lease();
            self.command_state.help_selected = self
                .command_state
                .help_selected
                .min(self.command_state.help_actions.len().saturating_sub(1));
        } else {
            drop(self.dismiss_help());
        }
    }

    /// Native focus can transition before its App message. Old modal actions
    /// retain the old immutable window epoch; current views get a fresh token.
    pub(in crate::app) fn refresh_help_focus(&mut self) {
        if self.command_state.help.is_none() {
            return;
        }
        match (
            self.window_focus.is_focused(),
            &self.command_state.help_window,
        ) {
            (true, Some(lease)) if self.window_focus.accepts_input(lease) => return,
            (false, None) => return,
            _ => {}
        }
        self.refresh_help_reference();
    }

    pub(in crate::app) fn run_help_action(
        &mut self,
        token: u64,
        action: HelpAction,
    ) -> Task<Message> {
        if self.command_state.help.is_none()
            || token != self.command_state.help_token
            || self.command_state.help_generation != self.settings.generation
            || self
                .command_state
                .help_window
                .as_ref()
                .is_none_or(|lease| !self.window_focus.accepts_input(lease))
            || self.exiting
        {
            return Task::none();
        }
        if matches!(
            action,
            HelpAction::Execute(_)
                | HelpAction::Recovery(..)
                | HelpAction::CopyDiagnostics
                | HelpAction::ClearDiagnostics
        ) {
            return self.run_menu_action(action);
        }
        let last = self.command_state.help_actions.len().saturating_sub(1);
        let selected = &mut self.command_state.help_selected;
        *selected = match action {
            HelpAction::Next => (*selected + 1) % (last + 1),
            HelpAction::Previous => (*selected + last) % (last + 1),
            HelpAction::First => 0,
            HelpAction::Last => last,
            HelpAction::PageUp => selected.saturating_sub(8),
            HelpAction::PageDown => (*selected + 8).min(last),
            _ => unreachable!(),
        };
        match (
            self.command_state.help_scroll.clone(),
            self.command_state.help_rows.get(*selected),
        ) {
            (Some(scroll), Some(row)) => reveal(scroll, row.clone()),
            _ => Task::none(),
        }
    }

    pub(in crate::app) fn help_view(&self) -> Option<Element<'_, Message>> {
        self.command_state.help?;
        let token = self.command_state.help_token;
        let references = self.command_reference();
        let selected = self
            .command_state
            .help_actions
            .get(self.command_state.help_selected)
            .copied();
        let mut rows = column![].spacing(6).width(Fill);
        for (index, action) in self.command_state.help_actions.iter().copied().enumerate() {
            let (label, detail, enabled) = self.menu_item(action, &references);
            let is_selected = index == self.command_state.help_selected;
            let label = format!(
                "{}{}{}",
                if is_selected { "› " } else { "" },
                label,
                if enabled {
                    ""
                } else {
                    " · unavailable in this context"
                }
            );
            let row = button(
                column![text(label), text(detail).size(13)]
                    .spacing(3)
                    .width(Fill),
            )
            .width(Fill)
            .padding(10)
            .on_press_maybe(enabled.then_some(Control::HelpAction(token, action)))
            .style(move |theme, status| crate::ui::style::choice(theme, status, is_selected));
            let mut row = container(row).width(Fill);
            if let Some(id) = self.command_state.help_rows.get(index) {
                row = row.id(id.clone());
            }
            rows = rows.push(row);
        }
        let mut rows = scrollable(container(rows).padding(iced::Padding {
            right: crate::ui::tokens::INSET * 2.0,
            ..Default::default()
        }))
        .height(Fill);
        if let Some(id) = &self.command_state.help_scroll {
            rows = rows.id(id.clone());
        }
        let content: Element<'_, Control> = container(
            column![
                text("Commands and shortcuts").size(22),
                text("Tab / Shift+Tab or ↑/↓ selects · Home/End · Enter chooses · Escape returns")
                    .size(13),
                rows,
                button("Return to terminal (Escape)")
                    .style(crate::ui::style::quiet)
                    .on_press(Control::DismissHelp),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(Fill)
        .height(Fill)
        .style(crate::ui::style::workspace)
        .into();
        Some(resize::capture::capture_with_window(
            content.map(Message::Control),
            false,
            None,
            self.window_focus.clone(),
            move |event| help_key(event, token, selected, self.settings.active.shortcuts()),
        ))
    }
}

fn help_key(
    event: &keyboard::Event,
    token: u64,
    selected: Option<HelpAction>,
    mappings: &[crate::settings::Shortcut],
) -> Option<KeyAction> {
    let keyboard::Event::KeyPressed {
        key,
        modifiers,
        repeat,
        ..
    } = event
    else {
        return matches!(event, keyboard::Event::KeyReleased { .. }).then_some(KeyAction::Consume);
    };
    if !repeat
        && (*key == Key::Named(Named::Escape)
            || super::resolve_with(event, mappings)
                == Some(super::Resolution::Dispatch(Command::Help)))
    {
        return Some(KeyAction::Dispatch(Control::DismissHelp));
    }
    let action = match key {
        Key::Named(Named::ArrowDown) => HelpAction::Next,
        Key::Named(Named::ArrowUp) => HelpAction::Previous,
        Key::Named(Named::Tab) if modifiers.shift() => HelpAction::Previous,
        Key::Named(Named::Tab) => HelpAction::Next,
        Key::Named(Named::Home) => HelpAction::First,
        Key::Named(Named::End) => HelpAction::Last,
        Key::Named(Named::PageUp) => HelpAction::PageUp,
        Key::Named(Named::PageDown) => HelpAction::PageDown,
        Key::Named(Named::Enter) | Key::Named(Named::Space) if !repeat => selected?,
        _ => return Some(KeyAction::Consume),
    };
    Some(KeyAction::Dispatch(Control::HelpAction(token, action)))
}

// Reveal measured rows, including wrapped lists of alternative bindings. Fresh
// modal and row identities make delayed scroll operations harmless on dismissal.
struct Reveal {
    scroll: WidgetId,
    row: WidgetId,
    viewport: Option<(Rectangle, Vector)>,
    selected: Option<Rectangle>,
}

pub(in crate::app) fn reveal(scroll: WidgetId, row: WidgetId) -> Task<Message> {
    operate(Reveal {
        scroll,
        row,
        viewport: None,
        selected: None,
    })
}
impl<T: 'static> Operation<T> for Reveal {
    fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn Operation<T>)) {
        f(self);
    }
    fn container(&mut self, id: Option<&WidgetId>, bounds: Rectangle) {
        if id == Some(&self.row) {
            self.selected = Some(bounds);
        }
    }
    fn scrollable(
        &mut self,
        id: Option<&WidgetId>,
        bounds: Rectangle,
        _: Rectangle,
        translation: Vector,
        _: &mut dyn operation::Scrollable,
    ) {
        if id == Some(&self.scroll) {
            self.viewport = Some((bounds, translation));
        }
    }
    fn finish(&self) -> operation::Outcome<T> {
        let Some(((viewport, translation), row)) = self.viewport.zip(self.selected) else {
            return operation::Outcome::None;
        };
        let top = row.y - viewport.y;
        let bottom = top + row.height;
        let offset = if top < translation.y || row.height >= viewport.height {
            top
        } else if bottom > translation.y + viewport.height {
            bottom - viewport.height
        } else {
            return operation::Outcome::None;
        };
        operation::Outcome::Chain(Box::new(operation::scrollable::scroll_to(
            self.scroll.clone(),
            operation::scrollable::AbsoluteOffset {
                x: None,
                y: Some(offset.max(0.0)),
            },
        )))
    }
}
