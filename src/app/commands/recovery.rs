//! The command menu is the keyboard route to passive recovery controls.
//! Freeze stable error IDs for one opening; asynchronous error eviction cannot
//! turn a selected row into an action on a different terminal.
use super::{App, CommandReference, HelpAction, Message};
use crate::errors::{Action, Error, Owner, Recovery};
use iced::Task;

#[cfg(test)]
#[path = "recovery/tests.rs"]
mod tests;

impl App {
    pub(super) fn menu_actions(&self) -> Vec<HelpAction> {
        let mut actions: Vec<_> = super::Id::ALL
            .iter()
            .copied()
            .map(|id| HelpAction::Execute(super::keymap::command(id)))
            .collect();
        for notice in self.errors.notices().rev() {
            actions.push(HelpAction::Recovery(notice.id, Action::Dismiss));
            if matches!(
                notice.error.recovery(),
                Recovery::Restart
                    | Recovery::PasteAgain
                    | Recovery::CopyAgain
                    | Recovery::ReloadSettings
                    | Recovery::Resize
            ) {
                actions.push(HelpAction::Recovery(notice.id, Action::Retry));
            }
            if matches!(notice.error, Error::Launch(_)) {
                actions.push(HelpAction::Recovery(notice.id, Action::UseDefault));
            }
        }
        if self.diagnostics.enabled() {
            actions.extend([HelpAction::CopyDiagnostics, HelpAction::ClearDiagnostics]);
        }
        actions
    }

    pub(super) fn menu_item(
        &self,
        action: HelpAction,
        references: &[CommandReference],
    ) -> (String, String, bool) {
        match action {
            HelpAction::Execute(command) => {
                let reference = references
                    .iter()
                    .find(|r| r.command == command)
                    .expect("catalog command");
                let bindings = if reference.bindings.is_empty() {
                    "Unbound · assign a shortcut in settings.toml".into()
                } else {
                    reference.bindings.join(" / ")
                };
                (reference.label.clone(), bindings, reference.enabled)
            }
            HelpAction::Recovery(id, action) => {
                let Some(notice) = self.errors.get(id) else {
                    return ("Previous recovery action".into(), "This notice was dismissed or replaced. Reopen Commands to refresh recovery actions.".into(), false);
                };
                let retry = self.error_retry_label(notice, true);
                let (label, enabled) = match action {
                    Action::Dismiss => ("Dismiss message", true),
                    Action::Retry => (retry.unwrap_or("Retry operation"), retry.is_some()),
                    Action::UseDefault => (
                        "Use default shell",
                        retry.is_some() && matches!(notice.error, Error::Launch(_)),
                    ),
                };
                let owner = match notice.owner {
                    Owner::Application => "Pike application".into(),
                    Owner::Session(id) => format!("Terminal {}", id.get()),
                };
                (
                    format!("{owner} · {label}"),
                    notice.error.message(),
                    enabled,
                )
            }
            HelpAction::CopyDiagnostics => (
                "Copy diagnostics".into(),
                "Opt-in metadata only; copies to the clipboard.".into(),
                self.diagnostics.enabled(),
            ),
            HelpAction::ClearDiagnostics => (
                "Clear diagnostics".into(),
                "Clear the bounded in-memory diagnostic records.".into(),
                self.diagnostics.enabled(),
            ),
            _ => unreachable!("navigation is not a menu row"),
        }
    }

    pub(super) fn run_menu_action(&mut self, action: HelpAction) -> Task<Message> {
        if !self.command_state.help_actions.contains(&action)
            || !self.menu_item(action, &self.command_reference()).2
        {
            return Task::none();
        }
        // Retire this opening before any clipboard receipt or launch is
        // created. The normal owner checks run again in the existing adapter.
        let restore = self.dismiss_help();
        let task = match action {
            HelpAction::Execute(command) => self.run_command(command),
            HelpAction::Recovery(id, Action::Retry)
                if self.errors.get(id).is_some_and(|n| {
                    matches!(n.owner, Owner::Session(_))
                        && matches!(
                            n.error.recovery(),
                            Recovery::CopyAgain | Recovery::PasteAgain
                        )
                }) =>
            {
                self.recovery_request(id)
                    .map(|request| self.retry_error_request(request))
                    .unwrap_or_else(Task::none)
            }
            HelpAction::Recovery(id, action) => self.error_action(id, action),
            HelpAction::CopyDiagnostics => self.copy_diagnostics(),
            HelpAction::ClearDiagnostics => {
                self.diagnostics.clear();
                Task::none()
            }
            _ => unreachable!("navigation is not executable"),
        };
        Task::batch([restore, task])
    }
}
