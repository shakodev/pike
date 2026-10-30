//! Tab dialogs use the existing modal/focus boundary and stable action tokens.
use iced::advanced::widget::{Id, Operation, operate, operation};
use iced::keyboard::{self, Key, key::Named};
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Fill, Rectangle, Task};

use super::resize::capture::KeyAction;
use super::{App, Control, Message, close, resize};
use crate::identity::{SessionId, TabId};
use crate::tab_strip;
use crate::tabs::title_draft;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogAction {
    Save,
    Cancel,
    Previous,
    Next,
    First,
    Last,
    Choose(TabId),
    Close(TabId),
    Composing(bool),
}

enum Kind {
    Rename {
        tab: TabId,
        draft: String,
        input: Id,
        composing: bool,
    },
    Overflow {
        selected: TabId,
        scroll: Id,
    },
}

#[derive(Clone)]
enum Edit {
    Name(String),
    Action(DialogAction),
}

struct Dialog {
    token: u64,
    restore: Option<SessionId>,
    kind: Kind,
}

#[derive(Default)]
pub(super) struct State {
    next: u64,
    current: Option<Dialog>,
}

impl State {
    pub(super) fn active(&self) -> bool {
        self.current.is_some()
    }
    pub(super) fn clear(&mut self) {
        self.current = None;
    }
    fn token(&mut self) -> Option<u64> {
        self.next = self.next.checked_add(1)?;
        Some(self.next)
    }
}

impl App {
    /// App-level blur must retire the editor even if no widget event is painted
    /// while the window is away. Preserve the draft, retire queued UI receipts.
    pub(super) fn blur_tab_dialog(&mut self) {
        if !self.tab_dialog.active() {
            return;
        }
        let Some(token) = self.tab_dialog.token() else {
            self.tab_dialog.clear();
            self.notice = Some("Tab dialog identifiers exhausted. Restart Pike when convenient.");
            return;
        };
        let dialog = self.tab_dialog.current.as_mut().unwrap();
        dialog.token = token;
        match &mut dialog.kind {
            Kind::Rename {
                input, composing, ..
            } => {
                *input = Id::unique();
                *composing = false;
            }
            Kind::Overflow { scroll, .. } => *scroll = Id::unique(),
        }
    }

    pub(super) fn focus_tab_dialog(&self) -> Option<Task<Message>> {
        match &self.tab_dialog.current.as_ref()?.kind {
            Kind::Rename { input, .. } => Some(operate(FocusName {
                input: input.clone(),
                window: self.window_focus.clone(),
            })),
            Kind::Overflow { selected, scroll } => {
                Some(operate(tab_strip::reveal(scroll.clone(), *selected)))
            }
        }
    }

    pub fn tab_dialog_token(&self) -> Option<u64> {
        self.tab_dialog.current.as_ref().map(|dialog| dialog.token)
    }

    pub fn tab_dialog_selected(&self) -> Option<TabId> {
        match &self.tab_dialog.current.as_ref()?.kind {
            Kind::Overflow { selected, .. } => Some(*selected),
            _ => None,
        }
    }

    pub fn tab_name_widget_id(&self) -> Option<Id> {
        match &self.tab_dialog.current.as_ref()?.kind {
            Kind::Rename { input, .. } => Some(input.clone()),
            _ => None,
        }
    }

    pub(super) fn begin_tab_rename(&mut self, tab: TabId) -> Task<Message> {
        let Some(title) = self.tab_title(tab) else {
            self.notice = Some("That tab is no longer open. Select an existing tab to rename it.");
            return self.focus_active();
        };
        let Some(token) = self.tab_dialog.token() else {
            self.notice = Some("Tab dialog identifiers exhausted. Restart Pike when convenient.");
            return self.focus_active();
        };
        let input = Id::unique();
        self.tab_dialog.current = Some(Dialog {
            token,
            restore: self.active_session(),
            kind: Kind::Rename {
                tab,
                draft: title.as_str().to_owned(),
                input: input.clone(),
                composing: false,
            },
        });
        self.notice = None;
        self.synchronize();
        // Only focus this unique, present editor. Do not unfocus unrelated
        // widgets: a delayed operation cannot disturb a replacement view.
        operate(FocusName {
            input,
            window: self.window_focus.clone(),
        })
    }

    pub(super) fn begin_tab_overflow(&mut self) -> Task<Message> {
        let Some(selected) = self.model.workspace().active() else {
            return self.command_refused(super::commands::Error::MissingTab);
        };
        let Some(token) = self.tab_dialog.token() else {
            self.notice = Some("Tab dialog identifiers exhausted. Restart Pike when convenient.");
            return self.focus_active();
        };
        let scroll = Id::unique();
        self.tab_dialog.current = Some(Dialog {
            token,
            restore: self.active_session(),
            kind: Kind::Overflow {
                selected,
                scroll: scroll.clone(),
            },
        });
        self.notice = None;
        self.synchronize();
        operate(tab_strip::reveal(scroll, selected))
    }

    pub(super) fn tab_name(&mut self, token: u64, value: &str) {
        if !self.window_focus.is_focused() {
            return;
        }
        if let Some(Dialog {
            token: current,
            kind: Kind::Rename { draft, .. },
            ..
        }) = &mut self.tab_dialog.current
            && *current == token
        {
            *draft = title_draft(value);
        }
    }

    fn dismiss_tab_dialog(&mut self) -> Task<Message> {
        let Some(dialog) = self.tab_dialog.current.take() else {
            return Task::none();
        };
        self.synchronize();
        if let Some(target) = dialog.restore.and_then(|id| self.model.target(id)) {
            self.select(target)
        } else {
            self.focus_active()
        }
    }

    pub(super) fn tab_dialog_action(&mut self, token: u64, action: DialogAction) -> Task<Message> {
        if self.exiting
            || self.close_policy.confirming()
            || self.command_state.help.is_some()
            || self.tab_dialog_token() != Some(token)
        {
            return Task::none();
        }
        if !self.window_focus.is_focused() && !matches!(action, DialogAction::Composing(false)) {
            return Task::none();
        }
        match action {
            DialogAction::Cancel => return self.dismiss_tab_dialog(),
            DialogAction::Composing(value) => {
                if let Some(Dialog {
                    kind: Kind::Rename { composing, .. },
                    ..
                }) = &mut self.tab_dialog.current
                {
                    *composing = value && self.window_focus.is_focused();
                }
                return Task::none();
            }
            DialogAction::Save => {
                let Some(Dialog {
                    kind:
                        Kind::Rename {
                            tab,
                            draft,
                            composing: false,
                            ..
                        },
                    ..
                }) = &self.tab_dialog.current
                else {
                    return Task::none();
                };
                if self.model.rename_tab(*tab, draft).is_err() {
                    self.notice =
                        Some("That tab is no longer open. Select an existing tab to rename it.");
                }
                return self.dismiss_tab_dialog();
            }
            _ => {}
        }
        let Some(Dialog {
            kind: Kind::Overflow { selected, scroll },
            ..
        }) = &mut self.tab_dialog.current
        else {
            return Task::none();
        };
        match action {
            DialogAction::Choose(tab) | DialogAction::Close(tab) => {
                if self.model.workspace().tab(tab).is_none() {
                    return Task::none();
                }
                self.tab_dialog.clear();
                self.synchronize();
                return if matches!(action, DialogAction::Close(_)) {
                    self.request_close(close::Scope::Tab(tab))
                } else {
                    self.activate_tab(tab)
                };
            }
            DialogAction::Next
            | DialogAction::Previous
            | DialogAction::First
            | DialogAction::Last => {
                let order = self.model.workspace().order();
                if order.is_empty() {
                    return self.dismiss_tab_dialog();
                }
                let index = order.iter().position(|tab| tab == selected).unwrap_or(0);
                let next = match action {
                    DialogAction::Next => (index + 1) % order.len(),
                    DialogAction::Previous => (index + order.len() - 1) % order.len(),
                    DialogAction::First => 0,
                    _ => order.len() - 1,
                };
                *selected = order[next];
                return operate(tab_strip::reveal(scroll.clone(), *selected));
            }
            _ => {}
        }
        Task::none()
    }

    pub(super) fn tab_dialog_view(&self) -> Option<Element<'_, Message>> {
        let dialog = self.tab_dialog.current.as_ref()?;
        let token = dialog.token;
        let content: Element<'_, Message> = match &dialog.kind {
            Kind::Rename {
                draft,
                input,
                composing,
                ..
            } => {
                let content: Element<'_, Edit> = container(column![
                    text("Rename tab").size(22),
                    text("Up to 256 characters. Leave blank to use the automatic name."),
                    text_input("Automatic tab name", draft)
                        .id(input.clone())
                        .on_input(Edit::Name),
                    text("Manual names stay until reset. Automatic names follow the focused terminal title, then its launch profile."),
                    row![
                        button("Cancel").on_press(Edit::Action(DialogAction::Cancel)),
                        button("Save").on_press_maybe((!*composing).then_some(Edit::Action(DialogAction::Save))),
                    ].spacing(12),
                ].spacing(12)).padding(24).max_width(620).width(Fill).into();
                let content = content.map(move |edit| match edit {
                    Edit::Name(value) => Message::TabName(token, value),
                    Edit::Action(action) => Message::Control(Control::TabDialog(token, action)),
                });
                resize::capture::capture_text(
                    container(content).center_x(Fill).center_y(Fill),
                    input.clone(),
                    token,
                    self.window_focus.clone(),
                    rename_key,
                )
            }
            Kind::Overflow { selected, scroll } => {
                let content = tab_strip::overflow(
                    self.tab_items(),
                    Some(*selected),
                    scroll.clone(),
                    Control::ActivateTab,
                    Control::CloseTab,
                    Control::TabDialog(token, DialogAction::Cancel),
                );
                resize::capture(
                    container(content.map(Message::Control))
                        .center_x(Fill)
                        .center_y(Fill),
                    false,
                    None,
                    overflow_key,
                )
            }
        };
        let selected = self.tab_dialog_selected();
        Some(content.map(move |message| match message {
            Message::Control(Control::TabDialog(_, DialogAction::Save)) if selected.is_some() => {
                Message::Control(Control::TabDialog(
                    token,
                    DialogAction::Choose(selected.unwrap()),
                ))
            }
            Message::Control(Control::TabDialog(_, action)) => {
                Message::Control(Control::TabDialog(token, action))
            }
            Message::Control(Control::ActivateTab(tab)) => {
                Message::Control(Control::TabDialog(token, DialogAction::Choose(tab)))
            }
            Message::Control(Control::CloseTab(tab)) => {
                Message::Control(Control::TabDialog(token, DialogAction::Close(tab)))
            }
            message => message,
        }))
    }
}

fn rename_key(event: &keyboard::Event) -> Option<KeyAction> {
    match event {
        keyboard::Event::KeyPressed {
            key: Key::Named(Named::Escape),
            repeat: false,
            ..
        } => Some(KeyAction::Dispatch(Control::TabDialog(
            0,
            DialogAction::Cancel,
        ))),
        keyboard::Event::KeyPressed {
            key: Key::Named(Named::Enter),
            repeat: false,
            ..
        } => Some(KeyAction::Dispatch(Control::TabDialog(
            0,
            DialogAction::Save,
        ))),
        _ if super::commands::resolve(event).is_some() => Some(KeyAction::Consume),
        _ => None,
    }
}

fn overflow_key(event: &keyboard::Event) -> Option<KeyAction> {
    let keyboard::Event::KeyPressed {
        key,
        modifiers,
        repeat,
        ..
    } = event
    else {
        return matches!(event, keyboard::Event::KeyReleased { .. }).then_some(KeyAction::Consume);
    };
    let action = match key {
        Key::Named(Named::ArrowUp) => DialogAction::Previous,
        Key::Named(Named::ArrowDown) => DialogAction::Next,
        Key::Named(Named::Home) => DialogAction::First,
        Key::Named(Named::End) => DialogAction::Last,
        Key::Named(Named::Tab) if modifiers.shift() => DialogAction::Previous,
        Key::Named(Named::Tab) => DialogAction::Next,
        Key::Named(Named::Enter) if !repeat => DialogAction::Save,
        Key::Named(Named::Escape) if !repeat => DialogAction::Cancel,
        _ => return Some(KeyAction::Consume),
    };
    Some(KeyAction::Dispatch(Control::TabDialog(0, action)))
}

struct FocusName {
    input: Id,
    window: iced_term::WindowFocus,
}
impl Operation<Message> for FocusName {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Message>)) {
        if self.window.is_focused() {
            operate(self);
        }
    }
    fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn operation::Focusable) {
        if id == Some(&self.input) && self.window.is_focused() {
            state.focus();
        }
    }
}
