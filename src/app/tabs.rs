//! Tab controls use the same reservation, launch pool and focus path as panes.
use iced::widget::{button, row};
use iced::{Element, Task};

use super::{App, Control, Message};
use crate::identity::TabId;
use crate::launch::LaunchRequest;
use crate::session::Effect;
use crate::workspace::Error;

impl App {
    pub(super) fn begin_new_tab(&mut self) -> Option<LaunchRequest> {
        if self.interaction_blocked() {
            return None;
        }
        // Freeze the source before create_tab changes the active pane. Source
        // profiles are retained for starting, exited and failed panes as well.
        let profile = match self.active_session() {
            Some(id) => match self.profiles.get(&id).cloned() {
                Some(profile) => profile,
                None => {
                    self.notice = Some("Wait for the source terminal to start, then try again.");
                    return None;
                }
            },
            None => self.profile.clone(),
        };
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
        let request = self.submit_profile(reservation, profile);
        self.synchronize();
        request
    }

    pub(super) fn new_tab(&mut self) -> Task<Message> {
        let launch = self.begin_new_tab()
            .map(Self::receive)
            .unwrap_or_else(Task::none);
        // A refused toolbar action leaves the source terminal ready to type.
        Task::batch([launch, self.focus_active()])
    }

    pub(super) fn activate_tab(&mut self, tab: TabId) -> Task<Message> {
        if self.interaction_blocked() || self.model.activate(tab).is_err() {
            return Task::none();
        }
        self.notice = None;
        self.synchronize();
        self.focus_active()
    }

    pub(super) fn tabs_view(&self) -> Element<'_, Control> {
        let workspace = self.model.workspace();
        let tabs = workspace
            .order()
            .iter()
            .enumerate()
            .fold(row![], |tabs, (index, id)| {
                let activity = workspace.tab(*id).expect("ordered tab").activity();
                let mut label = format!("Tab {}", index + 1);
                for (set, indicator) in [
                    (activity.unread, " · unread"),
                    (activity.bell, " · bell"),
                    (activity.exited, " · exited"),
                ] {
                    if set {
                        label.push_str(indicator);
                    }
                }
                tabs.push(
                    button(iced::widget::text(label))
                        .on_press(Control::ActivateTab(*id))
                        .style(if workspace.active() == Some(*id) {
                            button::primary
                        } else {
                            button::secondary
                        }),
                )
            });
        tabs.spacing(4).wrap().into()
    }
}
