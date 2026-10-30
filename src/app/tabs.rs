//! Tab controls use the same reservation, launch pool and focus path as panes.
use iced::{Element, Task};

use super::{App, Control, Message};
use crate::identity::TabId;
use crate::launch::LaunchRequest;
use crate::session::Effect;
use crate::tab_strip::{self, Item, TabAction};
use crate::tabs::{Selection, Title, display_title};
use crate::workspace::Error;

impl App {
    pub(super) fn begin_new_tab(&mut self) -> Option<LaunchRequest> {
        if self.interaction_blocked() {
            return None;
        }
        // Freeze the source before create_tab changes the active pane. Source
        // profiles are retained for starting, exited and failed panes as well.
        let (profile, name) = self.launch_profile(self.active_session())?;
        let reservation = self.create_tab_reservation()?;
        let request = self.submit_named_profile(reservation, profile, name);
        self.synchronize();
        request
    }

    pub(super) fn create_tab_reservation(&mut self) -> Option<crate::identity::Reservation> {
        let reservation = match self.model.create_tab() {
            Ok((_, Effect::Spawn(reservation))) => reservation,
            Ok(_) => unreachable!("new tab produces a spawn effect"),
            Err(error) => {
                self.notice = Some(match error {
                    Error::SessionLimit => {
                        "The 32-session limit includes starting, closing, exited and error panes. Close a pane, wait for cleanup, and try again."
                    }
                    Error::IdsExhausted => {
                        "No more tab or session identifiers are available. Restart Pike when convenient."
                    }
                    _ => "This tab cannot be opened. Select an available terminal and try again.",
                });
                return None;
            }
        };
        self.focus_on_attach.clear();
        Some(reservation)
    }

    pub(super) fn new_tab(&mut self) -> Task<Message> {
        let launch = self
            .begin_new_tab()
            .map(Self::receive)
            .unwrap_or_else(Task::none);
        // A refused toolbar action leaves the source terminal ready to type.
        Task::batch([launch, self.focus_active()])
    }

    pub(super) fn activate_tab(&mut self, tab: TabId) -> Task<Message> {
        if self.interaction_blocked() || self.model.activate(tab).is_err() {
            return Task::none();
        }
        self.focus_on_attach.clear();
        self.notice = None;
        self.synchronize();
        self.focus_active()
    }

    pub(super) fn select_tab(&mut self, selection: Selection) -> Task<Message> {
        let Some(tab) = self.model.workspace().selection_target(selection) else {
            return self.command_refused(super::commands::Error::MissingTab);
        };
        self.activate_tab(tab)
    }

    /// Labels follow the remembered focused session; reset clears only its OSC
    /// title. The manual override is independent of session exit/restart.
    pub fn tab_title(&self, id: TabId) -> Option<Title> {
        let tab = self.model.workspace().tab(id)?;
        let session = tab.grid().get(tab.focused())?.session;
        let osc = self
            .sessions
            .entries
            .get(&session)
            .and_then(|entry| entry.title.as_ref());
        let profile = self.profiles.get(&session).map_or("", |profile| {
            profile.program.rsplit(['/', '\\']).next().unwrap_or("")
        });
        Some(display_title(tab.manual_name(), osc, profile))
    }

    pub(super) fn tab_items(&self) -> Vec<Item> {
        self.model
            .workspace()
            .order()
            .iter()
            .map(|id| Item {
                id: *id,
                title: self.tab_title(*id).expect("ordered tab"),
                activity: self
                    .model
                    .workspace()
                    .tab(*id)
                    .expect("ordered tab")
                    .activity(),
            })
            .collect()
    }

    pub(super) fn tab_action(&mut self, action: TabAction) -> Task<Message> {
        if self.interaction_blocked() {
            return Task::none();
        }
        match action {
            TabAction::BeginRename(tab) => return self.begin_tab_rename(tab),
            TabAction::OpenOverflow => return self.begin_tab_overflow(),
            TabAction::Select(selection) => return self.select_tab(selection),
            TabAction::ResetName(tab) => {
                if self.model.reset_tab_name(tab).is_err() {
                    self.notice =
                        Some("That tab is no longer open. Select an existing tab to rename it.");
                } else {
                    self.notice = None;
                }
            }
            TabAction::Move {
                tab,
                anchor,
                placement,
            } => {
                if self.model.reorder_tab(tab, anchor, placement).is_err() {
                    self.notice = Some(
                        "Those tabs are no longer available. Try moving the selected tab again.",
                    );
                } else {
                    self.notice = None;
                }
            }
        }
        self.focus_active()
    }

    pub(super) fn tabs_view(&self) -> Element<'_, Control> {
        tab_strip::view(
            self.tab_items(),
            self.model.workspace().active(),
            Control::ActivateTab,
            Control::CloseTab,
            Control::TabAction,
        )
    }
}
