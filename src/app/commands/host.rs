//! Consume typed commands through the existing tab, split and close owners.
use std::collections::BTreeMap;

use iced::Task;
use iced::keyboard;
use iced_term::settings::FontSettings;

use super::{Command, Error, FontChange, Resolution, TabMove, TabSelection};
use crate::app::resize::capture::KeyAction;
use crate::app::{App, Control, Message, close};
use crate::identity::SessionId;

/// Redacted one-use clipboard admission receipt. Payload lives only in the
/// transient runtime reply, never in command/focus state or diagnostics.
#[derive(Clone)]
pub struct PasteRequest {
    reservation: crate::identity::Reservation,
    lease: crate::app::focus::InputLease,
    window: iced_term::WindowInputLease,
    consumed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    recovery: Option<crate::errors::ErrorId>,
}

#[derive(Default)]
pub(in crate::app) struct State {
    error: Option<Error>,
    pub(in crate::app) help: Option<Option<SessionId>>,
    pub(in crate::app) help_history: bool,
    pub(in crate::app) help_token: u64,
    pub(in crate::app) help_generation: u64,
    pub(in crate::app) help_window: Option<iced_term::WindowInputLease>,
    pub(in crate::app) help_selected: usize,
    pub(in crate::app) help_scroll: Option<iced::widget::Id>,
    pub(in crate::app) help_rows: Vec<iced::widget::Id>,
    pub(in crate::app) fonts: BTreeMap<SessionId, FontSettings>,
}

pub(in crate::app) fn on_key(
    event: &keyboard::Event,
    shortcuts: &[crate::settings::Shortcut],
    active: Option<&iced_term::Terminal>,
) -> Option<KeyAction> {
    super::resolve_with(event, shortcuts)
        .filter(|resolution| {
            !matches!(resolution, Resolution::Dispatch(Command::History(_)))
                || active
                    .and_then(|t| t.renderable_content())
                    .is_some_and(|c| !c.terminal_mode.contains(iced_term::TermMode::ALT_SCREEN))
        })
        .map(|resolution| match resolution {
            Resolution::Consume => KeyAction::Consume,
            Resolution::Dispatch(command) => KeyAction::Dispatch(Control::Command(command)),
        })
}

impl App {
    pub fn command_error(&self) -> Option<Error> {
        self.command_state.error
    }

    pub(in crate::app) fn command_refused(&mut self, error: Error) -> Task<Message> {
        self.record_command_refusal(error);
        self.focus_active()
    }

    fn record_command_refusal(&mut self, error: Error) {
        self.command_state.error = Some(error);
        self.notice = None;
        self.report_error(
            self.active_session().map_or(
                crate::errors::Owner::Application,
                crate::errors::Owner::Session,
            ),
            crate::errors::Error::Operation(error),
        );
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
                    self.record_command_refusal(Error::NewTabRefused);
                }
                task
            }
            Command::Help => self.open_help(),
            Command::CloseWindow => self.request_close(close::Scope::Window),
            Command::NewProfile(index) => self.new_profile(self.settings.generation, index),
            Command::MoveTab(direction) => {
                let Some(tab) = self.model.workspace().active() else {
                    return self.command_refused(Error::MissingTab);
                };
                let order = self.model.workspace().order();
                let index = order
                    .iter()
                    .position(|id| *id == tab)
                    .expect("active ordered tab");
                let next = match direction {
                    TabMove::Left => index.checked_sub(1),
                    TabMove::Right => index.checked_add(1).filter(|i| *i < order.len()),
                };
                let Some(next) = next else {
                    return self.focus_active();
                };
                self.tab_action(crate::tab_strip::TabAction::Move {
                    tab,
                    anchor: order[next],
                    placement: match direction {
                        TabMove::Left => crate::tabs::Placement::Before,
                        TabMove::Right => crate::tabs::Placement::After,
                    },
                })
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
                    Command::Retry => self.retry(id, false),
                    Command::UseDefault => self.retry(id, true),
                    Command::ResetTabName => {
                        self.tab_action(crate::tab_strip::TabAction::ResetName(target.tab))
                    }
                    Command::Resize(step) => self.resize_step(step),
                    Command::MovePane(direction) => {
                        let Some(tab) = self.model.workspace().tab(target.tab) else {
                            return self.command_refused(Error::MissingTab);
                        };
                        if tab.grid().maximized().is_some() {
                            return self.focus_active();
                        }
                        let Some(pane) = tab.grid().adjacent(target.pane, direction) else {
                            return self.focus_active();
                        };
                        match self.model.swap(
                            target,
                            crate::layout::PaneTarget {
                                tab: target.tab,
                                pane,
                            },
                        ) {
                            Ok(()) => {
                                self.focus_on_attach.clear();
                                self.synchronize();
                                self.focus_active()
                            }
                            Err(error) => self.command_refused(Error::Workspace(error)),
                        }
                    }
                    Command::Copy => {
                        let copy = self.copy_owned(id);
                        Task::batch([copy, self.focus_active()])
                    }
                    Command::Paste => {
                        if !self.command_enabled(command) {
                            return self.command_refused(Error::TerminalNotReady);
                        }
                        self.read_command_paste(None)
                            .unwrap_or_else(|| self.focus_active())
                    }
                    Command::History(scroll) => self.update(Message::Terminal((
                        id,
                        iced_term::Event::BackendCall(
                            id.get(),
                            iced_term::BackendCommand::ScrollHistory(scroll),
                        ),
                    ))),
                    Command::ClosePane => self.request_close(close::Scope::Pane(id)),
                    Command::CloseTab => self.request_close(close::Scope::Tab(target.tab)),
                    Command::Split(direction) => {
                        let count = self.model.sessions().len();
                        let task = self.split(target, direction);
                        if self.model.sessions().len() == count && self.notice.is_some() {
                            self.record_command_refusal(Error::SplitRefused);
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
        self.command_state.help_window = None;
        self.synchronize();
        if let Some(target) = restore.and_then(|id| self.model.target(id)) {
            self.select(target)
        } else {
            self.focus_active()
        }
    }

    /// The clipboard reply retains the requesting session and never follows a
    /// newer pane, modal, native blur or IME owner into another terminal.
    pub(in crate::app) fn begin_command_paste(&self) -> Option<PasteRequest> {
        let id = self.active_session()?;
        if self.interaction_blocked()
            || !self.command_enabled(Command::Paste)
            || self.terminal(id)?.is_composing()
        {
            return None;
        }
        Some(PasteRequest {
            reservation: self.model.sessions().get(id)?.reservation(),
            lease: self.focus_authority.input_lease(id)?,
            window: self.window_focus.input_lease()?,
            consumed: Default::default(),
            recovery: None,
        })
    }

    /// The one native reader is shared by commands and owner-bound recovery.
    pub(in crate::app) fn read_command_paste(
        &self,
        recovery: Option<crate::errors::ErrorId>,
    ) -> Option<Task<Message>> {
        let mut request = self.begin_command_paste()?;
        request.recovery = recovery;
        Some(Task::batch([
            iced::clipboard::read().map(move |text| Message::CommandPaste(request.clone(), text)),
            self.focus_active(),
        ]))
    }

    pub(in crate::app) fn command_paste(
        &mut self,
        request: PasteRequest,
        text: Option<String>,
    ) -> Task<Message> {
        let id = request.reservation.session;
        if request
            .consumed
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            return Task::none();
        }
        if !self.focus_authority.accepts_input(&request.lease, id)
            || !self.window_focus.accepts_input(&request.window)
            || self
                .model
                .sessions()
                .get(id)
                .is_none_or(|s| s.reservation() != request.reservation)
            || self.interaction_blocked()
            || self.active_session() != Some(id)
            || !self.window_focus.is_focused()
            || !self.command_enabled(Command::Paste)
            || self.terminal(id).is_none_or(|t| t.is_composing())
        {
            if let Some(recovery) = request.recovery {
                self.errors
                    .cancel_retry(crate::errors::Owner::Session(id), recovery);
            }
            return Task::none();
        }
        if let Some(terminal) = self
            .sessions
            .entries
            .get_mut(&id)
            .and_then(|entry| entry.terminal_mut())
        {
            match terminal.paste(text) {
                iced_term::actions::Action::PasteRejected(error) => self.report_error(
                    crate::errors::Owner::Session(id),
                    crate::errors::Error::Clipboard(error),
                ),
                _ => self
                    .errors
                    .resolve(crate::errors::Owner::Session(id), |error| {
                        matches!(error, crate::errors::Error::Clipboard(_))
                    }),
            }
        }
        self.refresh_resize_layout();
        self.focus_active()
    }
}

#[cfg(test)]
mod tests;
