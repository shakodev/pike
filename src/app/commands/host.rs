//! Consume typed commands through the existing tab, split and close owners.
use std::collections::BTreeMap;

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{button, column, container, scrollable, text};
use iced::{Element, Fill, Task};
use iced_term::settings::FontSettings;

use super::{Command, Error, FontChange, Resolution, TabSelection};
use crate::app::resize::capture::KeyAction;
use crate::app::{App, Control, Message, close, resize};
use crate::identity::SessionId;

#[derive(Default)]
pub(in crate::app) struct State {
    error: Option<Error>,
    pub(in crate::app) help: Option<Option<SessionId>>,
    pub(in crate::app) fonts: BTreeMap<SessionId, FontSettings>,
}

pub(in crate::app) fn on_key(event: &keyboard::Event) -> Option<KeyAction> {
    super::resolve(event)
        .map(|resolution| match resolution {
            Resolution::Consume => KeyAction::Consume,
            Resolution::Dispatch(command) => KeyAction::Dispatch(Control::Command(command)),
        })
        .or_else(|| resize::resize_key(event))
}

impl App {
    pub fn command_error(&self) -> Option<Error> {
        self.command_state.error
    }

    pub(in crate::app) fn command_refused(&mut self, error: Error) -> Task<Message> {
        self.command_state.error = Some(error);
        self.notice = Some(error.message());
        self.focus_active()
    }

    pub(in crate::app) fn run_command(&mut self, command: Command) -> Task<Message> {
        if self.interaction_blocked() {
            return Task::none();
        }
        self.command_state.error = None;
        self.notice = None;
        match command {
            Command::NewTab => {
                let count = self.model.workspace().order().len();
                let task = self.new_tab();
                if self.model.workspace().order().len() == count && self.notice.is_some() {
                    self.command_state.error = Some(Error::NewTabRefused);
                }
                task
            }
            Command::Help => {
                self.command_state.help = Some(self.active_session());
                self.synchronize();
                Task::none()
            }
            Command::RenameTab => {
                let Some(tab) = self.model.workspace().active() else {
                    return self.command_refused(Error::MissingTab);
                };
                self.begin_tab_rename(tab)
            }
            Command::ListTabs => self.begin_tab_overflow(),
            Command::ReloadSettings => self.reload_settings(),
            Command::NextTab | Command::PreviousTab | Command::SelectTab(_) => {
                use crate::tabs::Selection;
                let selection = match command {
                    Command::NextTab => Selection::Next,
                    Command::PreviousTab => Selection::Previous,
                    Command::SelectTab(TabSelection::Index(index)) => {
                        Selection::Index(usize::from(index))
                    }
                    Command::SelectTab(TabSelection::Last) => {
                        Selection::Index(self.model.workspace().order().len().saturating_sub(1))
                    }
                    _ => unreachable!(),
                };
                // Reuse R29's intent invalidation, cache and remembered focus.
                self.select_tab(selection)
            }
            command => {
                let Some(target) = self.active_target() else {
                    return self.command_refused(Error::NoActivePane);
                };
                let id = self.active_session().expect("target owns a session");
                match command {
                    Command::ClosePane => self.request_close(close::Scope::Pane(id)),
                    Command::CloseTab => self.request_close(close::Scope::Tab(target.tab)),
                    Command::Split(direction) => {
                        let count = self.model.sessions().len();
                        let task = self.split(target, direction);
                        if self.model.sessions().len() == count && self.notice.is_some() {
                            self.command_state.error = Some(Error::SplitRefused);
                        }
                        task
                    }
                    Command::Focus(direction) => match self.model.focus_adjacent(target, direction)
                    {
                        Ok(Some(target)) => self.update(Message::Control(Control::Focus(target))),
                        Ok(None) => self.update(Message::Control(Control::Focus(target))),
                        Err(error) => self.command_refused(Error::Workspace(error)),
                    },
                    Command::ToggleZoom => match self.model.zoom(target) {
                        Ok(()) => self.update(Message::Control(Control::Focus(target))),
                        Err(error) => self.command_refused(Error::Workspace(error)),
                    },
                    Command::Font(change) => {
                        if self.terminal(id).is_none() {
                            return self.command_refused(Error::TerminalNotReady);
                        }
                        let mut font = self
                            .command_state
                            .fonts
                            .get(&id)
                            .cloned()
                            .unwrap_or_else(|| self.settings.active.font().terminal());
                        font.size = match change {
                            FontChange::Larger => (font.size + 1.0).min(28.0),
                            FontChange::Smaller => (font.size - 1.0).max(10.0),
                            FontChange::Reset => self.settings.active.font().size,
                        };
                        self.change_terminal_font(id, font);
                        self.focus_active()
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    pub(in crate::app) fn dismiss_help(&mut self) -> Task<Message> {
        let Some(restore) = self.command_state.help.take() else {
            return Task::none();
        };
        self.synchronize();
        if let Some(target) = restore.and_then(|id| self.model.target(id)) {
            self.select(target)
        } else {
            self.focus_active()
        }
    }

    pub(in crate::app) fn help_view(&self) -> Option<Element<'_, Message>> {
        self.command_state.help?;
        let content: Element<'_, Control> = column![
            text("Keyboard shortcuts").size(22),
            text("Reload settings: Ctrl+Shift+F5 (or Reload settings toolbar button). Shortcut configuration is validated and reserved for the R32 remapping feature."),
            text("New tab: Ctrl+Shift+T\nClose pane / tab: Ctrl+Shift+W / Q\nNext / previous tab: Ctrl+Tab / Ctrl+Shift+Tab\nSelect tabs 1–8 / last: Ctrl+Shift+1…8 / 9\nRename tab / all tabs: Ctrl+Shift+R / L\nSplit right / down: Ctrl+Shift+O / E\nFocus adjacent pane: Alt+Arrow\nResize divider: Alt+Shift+Arrow\nMaximize / restore: Ctrl+Shift+Enter\nCopy / paste: Ctrl+Shift+C / V; Shift+Insert pastes\nHistory: Shift+PageUp / PageDown / Home / End\nFont larger / smaller / reset: Ctrl+Shift+Plus / Minus / 0\nInterrupt shell: Ctrl+C\nDismiss help: Escape or F1"),
            button("Return to terminal").on_press(Control::DismissHelp),
        ].spacing(12).into();
        let content: Element<'_, Control> = container(scrollable(content))
            .padding(24)
            .width(Fill)
            .height(Fill)
            .into();
        Some(resize::capture(
            content.map(Message::Control),
            false,
            None,
            help_key,
        ))
    }
}

fn help_key(event: &keyboard::Event) -> Option<KeyAction> {
    match event {
        keyboard::Event::KeyPressed {
            key: Key::Named(Named::Escape | Named::F1),
            repeat: false,
            ..
        } => Some(KeyAction::Dispatch(Control::DismissHelp)),
        keyboard::Event::KeyPressed { .. } | keyboard::Event::KeyReleased { .. } => {
            Some(KeyAction::Consume)
        }
        keyboard::Event::ModifiersChanged(_) => None,
    }
}

#[cfg(test)]
mod tests;
