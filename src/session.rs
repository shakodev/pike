//! Pure lifecycle registry. Backend owners live outside this module.
use std::collections::BTreeMap;

pub use crate::identity::{LaunchToken, RequestId, Reservation, SessionId};

/// Bounded, redacted failure metadata; never a command or terminal transcript.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Admission,
    Canceled,
    Spawn,
    Worker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    Starting,
    Running,
    Closing { awaiting_spawn: bool },
    Exited { code: Option<i32> },
    Failed(Failure),
}

#[derive(Debug)]
pub struct SessionEntry {
    pub(crate) reservation: Reservation,
    pub(crate) lifecycle: Lifecycle,
    launch_claimed: bool,
}

impl SessionEntry {
    pub fn reservation(&self) -> Reservation {
        self.reservation
    }

    pub fn lifecycle(&self) -> Lifecycle {
        self.lifecycle
    }
}

/// Commands for a future adapter, not executable tasks. Close is issued once
/// per entry and targets precisely this attempt (cancel startup or close live).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Spawn(Reservation),
    Close(Reservation),
}

/// Applies to the supplied prepared result only. Dispose must never close an
/// already-attached session, even for a duplicate completion with identical IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnDisposition {
    Attach,
    Dispose,
}

#[derive(Debug, Default)]
pub struct SessionRegistry {
    entries: BTreeMap<SessionId, SessionEntry>,
}

impl SessionRegistry {
    pub fn get(&self, id: SessionId) -> Option<&SessionEntry> {
        self.entries.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (SessionId, &SessionEntry)> {
        self.entries.iter().map(|(id, entry)| (*id, entry))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn insert(&mut self, reservation: Reservation) {
        let previous = self.entries.insert(
            reservation.session,
            SessionEntry {
                reservation,
                lifecycle: Lifecycle::Starting,
                launch_claimed: false,
            },
        );
        debug_assert!(previous.is_none());
    }

    pub(crate) fn accepts_spawn(&self, reservation: Reservation) -> bool {
        self.get(reservation.session).is_some_and(|entry| {
            entry.reservation == reservation && entry.lifecycle == Lifecycle::Starting
        })
    }

    pub(crate) fn claim_launch(&mut self, reservation: Reservation) -> Option<LaunchToken> {
        if !self.accepts_spawn(reservation) {
            return None;
        }
        let entry = self.entries.get_mut(&reservation.session)?;
        if entry.launch_claimed {
            return None;
        }
        entry.launch_claimed = true;
        Some(LaunchToken::new(reservation))
    }

    /// Commit only a matching live reservation. Caller must dispose every
    /// rejected result, including results arriving after close acknowledgement.
    pub(crate) fn spawn_succeeded(&mut self, reservation: Reservation) -> SpawnDisposition {
        if let Some(entry) = self.entries.get_mut(&reservation.session)
            && entry.reservation == reservation
        {
            match entry.lifecycle {
                Lifecycle::Starting => {
                    entry.lifecycle = Lifecycle::Running;
                    return SpawnDisposition::Attach;
                }
                Lifecycle::Closing {
                    awaiting_spawn: true,
                } => {
                    // This result now needs disposal. A contradictory late
                    // failure cannot release its budget before closed().
                    entry.lifecycle = Lifecycle::Closing {
                        awaiting_spawn: false,
                    };
                }
                _ => {}
            }
        }
        SpawnDisposition::Dispose
    }

    pub(crate) fn spawn_failed(&mut self, reservation: Reservation, failure: Failure) -> bool {
        let Some(entry) = self.entries.get_mut(&reservation.session) else {
            return false;
        };
        if entry.reservation != reservation {
            return false;
        }
        match entry.lifecycle {
            Lifecycle::Starting => entry.lifecycle = Lifecycle::Failed(failure),
            // A failed launch owns no prepared resource, so this also settles
            // a canceled reservation. A late success is still disposed.
            Lifecycle::Closing {
                awaiting_spawn: true,
            } => {
                self.entries.remove(&reservation.session);
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn close(&mut self, id: SessionId) -> Option<Effect> {
        let entry = self.entries.get_mut(&id)?;
        match entry.lifecycle {
            Lifecycle::Starting | Lifecycle::Running => {
                entry.lifecycle = Lifecycle::Closing {
                    awaiting_spawn: entry.lifecycle == Lifecycle::Starting,
                };
                Some(Effect::Close(entry.reservation))
            }
            Lifecycle::Closing { .. } => None,
            Lifecycle::Exited { .. } | Lifecycle::Failed(_) => {
                self.entries.remove(&id);
                None
            }
        }
    }

    pub(crate) fn exited(&mut self, id: SessionId, code: Option<i32>) -> bool {
        let Some(entry) = self.entries.get_mut(&id) else {
            return false;
        };
        if entry.lifecycle != Lifecycle::Running {
            return false;
        }
        entry.lifecycle = Lifecycle::Exited { code };
        true
    }

    /// Acknowledges complete backend disposal, not merely a child-exit event.
    pub(crate) fn closed(&mut self, id: SessionId) -> bool {
        if self
            .get(id)
            .is_some_and(|e| matches!(e.lifecycle, Lifecycle::Closing { .. }))
        {
            self.entries.remove(&id);
            true
        } else {
            false
        }
    }
}
