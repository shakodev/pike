//! Execute the model's single-use reservation and UI-thread handoff contract.
use super::{App, Message};
use crate::identity::Reservation;
use crate::launch::{LaunchError, LaunchRequest, PreparedSession};
use crate::session::{Effect, Failure, Lifecycle};
use crate::terminal::{Entry, Owner};
use iced::Task;
use iced_term::settings::BackendSettings;

impl App {
    pub(super) fn begin_open(&mut self) -> Option<LaunchRequest> {
        if self.exiting || !self.model.workspace().order().is_empty() {
            return None;
        }
        let Ok((_, Effect::Spawn(reservation))) = self.model.create_tab() else {
            self.notice =
                Some("A terminal cannot be opened yet. Wait for closing sessions to finish.");
            return None;
        };
        self.submit(reservation)
    }

    pub(super) fn submit(&mut self, reservation: Reservation) -> Option<LaunchRequest> {
        self.submit_profile(reservation, self.profile.clone())
    }

    pub(super) fn submit_profile(
        &mut self,
        reservation: Reservation,
        mut profile: BackendSettings,
    ) -> Option<LaunchRequest> {
        let token = self.model.claim_launch(reservation)?;
        self.notice = None;
        // Freeze the launch cwd. Splits inherit this directory even after a
        // process changes its own cwd; there is no shell integration here.
        profile.working_directory = profile
            .working_directory
            .map(|path| std::path::absolute(&path).unwrap_or(path))
            .or_else(|| std::env::current_dir().ok());
        self.profiles.insert(reservation.session, profile.clone());
        match self.launches.start_reserved(token, profile) {
            Ok(launch) => {
                self.sessions.entries.insert(
                    reservation.session,
                    Entry::new(reservation, Owner::Starting(launch.cancellation())),
                );
                Some(launch)
            }
            Err(error) => {
                self.model.spawn_failed(reservation, Failure::Admission);
                self.sessions.entries.insert(
                    reservation.session,
                    Entry::new(reservation, Owner::Failed(error)),
                );
                None
            }
        }
    }

    pub(super) fn receive(launch: LaunchRequest) -> Task<Message> {
        let reservation = Reservation {
            session: launch.session,
            request: launch.request,
        };
        Task::perform(launch.receive(), move |result| {
            Message::Launched(reservation, result)
        })
    }

    pub(super) fn open(&mut self) -> Task<Message> {
        self.begin_open()
            .map(Self::receive)
            .unwrap_or_else(Task::none)
    }

    pub(super) fn launched(
        &mut self,
        reservation: Reservation,
        result: Result<Box<PreparedSession>, LaunchError>,
    ) -> Task<Message> {
        match result {
            Ok(ready)
                if !self.exiting
                    && self.model.accepts_spawn(reservation)
                    && ready.session == reservation.session
                    && ready.request == reservation.request =>
            {
                let active = self.active_session() == Some(reservation.session);
                // Font/cache attachment occurs here, on Iced's update thread.
                match ready.attach_hidden(Default::default(), Default::default()) {
                    Ok(mut terminal) => {
                        terminal.set_window_focus(self.window_focus.clone());
                        self.model.spawn_succeeded(reservation);
                        self.sessions.entries.insert(
                            reservation.session,
                            Entry::new(reservation, Owner::Attached(Box::new(terminal))),
                        );
                        if (self.focus_on_attach.remove(&reservation.session) || active)
                            && let Some(target) = self.model.target(reservation.session)
                        {
                            return self.select(target);
                        }
                        self.synchronize();
                    }
                    Err(error) => self.failed(reservation, error),
                }
            }
            Ok(ready) => {
                let handle = ready.shutdown_handle();
                // This disposition describes only the supplied owner. It must
                // never close an already attached duplicate/current session.
                if self.model.accepts_spawn(reservation) {
                    self.failed(reservation, LaunchError::WorkerFailed);
                } else {
                    self.model.spawn_succeeded(reservation);
                }
                if matches!(self.model.sessions().get(reservation.session).map(|e| (e.reservation(), e.lifecycle())),
                    Some((ids, Lifecycle::Closing { .. })) if ids == reservation)
                    && self
                        .sessions
                        .entries
                        .get(&reservation.session)
                        .is_some_and(|entry| matches!(entry.owner, Owner::Starting(_)))
                {
                    self.sessions.entries.insert(
                        reservation.session,
                        Entry::new(reservation, Owner::Disposing(handle.clone())),
                    );
                    drop(ready);
                    return Task::perform(
                        async move {
                            handle.wait().await;
                            reservation
                        },
                        Message::Disposed,
                    );
                }
                drop(ready);
                // An unrelated/duplicate result has its own pool-owned disposal;
                // never let its eventual completion acknowledge this ID's live owner.
            }
            Err(error) => self.failed(reservation, error),
        }
        Task::none()
    }

    fn failed(&mut self, reservation: Reservation, error: LaunchError) {
        let failure = match error {
            LaunchError::Canceled => Failure::Canceled,
            LaunchError::WorkerFailed => Failure::Worker,
            _ => Failure::Spawn,
        };
        if self.model.spawn_failed(reservation, failure) {
            self.focus_on_attach.remove(&reservation.session);
            if let Some(entry) = self.sessions.entries.get(&reservation.session)
                && let Owner::Starting(cancel) = &entry.owner
            {
                cancel.cancel();
            }
            if self.model.sessions().get(reservation.session).is_some() {
                self.sessions.entries.insert(
                    reservation.session,
                    Entry::new(reservation, Owner::Failed(error)),
                );
            } else {
                self.sessions.entries.remove(&reservation.session);
                self.profiles.remove(&reservation.session);
            }
        }
    }
}
