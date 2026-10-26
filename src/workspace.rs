//! Atomic, event-driven workspace transactions. All mutation stays on the caller.
use std::collections::BTreeMap;
use std::num::NonZeroU16;

use iced::widget::pane_grid::{Axis, Split};

pub use crate::identity::TabId;
use crate::identity::{Reservation, SessionId};
use crate::layout::{PaneState, PaneTarget, TabState, contains_split};
use crate::session::{Effect, Failure, SessionRegistry, SpawnDisposition};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    StaleTab,
    StalePane,
    StaleSplit,
    InvalidRatio,
    SessionLimit,
    IdsExhausted,
    LayoutExhausted,
}

#[derive(Debug, Default)]
pub struct Workspace {
    order: Vec<TabId>,
    tabs: BTreeMap<TabId, TabState>,
    active: Option<TabId>,
}

impl Workspace {
    pub fn order(&self) -> &[TabId] {
        &self.order
    }
    pub fn tab(&self, tab: TabId) -> Option<&TabState> {
        self.tabs.get(&tab)
    }
    pub fn active(&self) -> Option<TabId> {
        self.active
    }
    pub fn pane(&self, target: PaneTarget) -> Option<&PaneState> {
        self.tab(target.tab)?.grid.get(target.pane)
    }

    fn remove_pane(&mut self, target: PaneTarget) {
        let tab = self.tabs.get_mut(&target.tab).expect("validated tab");
        if tab.grid.len() > 1 {
            tab.remove(target.pane);
        } else {
            self.remove_tab(target.tab);
        }
    }

    fn remove_tab(&mut self, id: TabId) {
        self.tabs.remove(&id);
        let index = self
            .order
            .iter()
            .position(|tab| *tab == id)
            .expect("tab order");
        self.order.remove(index);
        if self.active == Some(id) {
            self.active = self.order.get(index).or_else(|| self.order.last()).copied();
        }
    }
}

/// Transaction boundary for independent layout and lifecycle owners. Read-only
/// access prevents callers bypassing the one-pane/one-entry invariant.
#[derive(Debug)]
pub struct Model {
    workspace: Workspace,
    sessions: SessionRegistry,
    limit: NonZeroU16,
}

impl Default for Model {
    fn default() -> Self {
        Self::new(NonZeroU16::new(16).unwrap())
    }
}

impl Model {
    /// All retained entries count toward this budget, including closing,
    /// exited and failed entries. Tabs/panes cannot grow beyond the same bound.
    pub fn new(limit: NonZeroU16) -> Self {
        Self {
            workspace: Workspace::default(),
            sessions: SessionRegistry::default(),
            limit,
        }
    }
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }
    pub fn sessions(&self) -> &SessionRegistry {
        &self.sessions
    }

    fn reserve(&self) -> Result<Reservation, Error> {
        if self.sessions.len() >= usize::from(self.limit.get()) {
            return Err(Error::SessionLimit);
        }
        Reservation::allocate().map_err(|_| Error::IdsExhausted)
    }

    pub fn create_tab(&mut self) -> Result<(PaneTarget, Effect), Error> {
        let reservation = self.reserve()?;
        let id = TabId::allocate().map_err(|_| Error::IdsExhausted)?;
        let tab = TabState::new(reservation.session);
        let target = PaneTarget {
            tab: id,
            pane: tab.focused(),
        };
        self.sessions.insert(reservation);
        self.workspace.tabs.insert(id, tab);
        self.workspace.order.push(id);
        self.workspace.active = Some(id);
        Ok((target, Effect::Spawn(reservation)))
    }

    pub fn split(&mut self, target: PaneTarget, axis: Axis) -> Result<(PaneTarget, Effect), Error> {
        self.validate(target)?;
        let reservation = self.reserve()?;
        let tab = self.workspace.tabs.get_mut(&target.tab).unwrap();
        let pane = tab
            .split(
                target.pane,
                axis,
                PaneState {
                    session: reservation.session,
                },
            )
            .ok_or(Error::LayoutExhausted)?;
        self.sessions.insert(reservation);
        self.workspace.active = Some(target.tab);
        Ok((
            PaneTarget {
                tab: target.tab,
                pane,
            },
            Effect::Spawn(reservation),
        ))
    }

    fn validate(&self, target: PaneTarget) -> Result<(), Error> {
        self.workspace
            .tab(target.tab)
            .ok_or(Error::StaleTab)?
            .grid
            .get(target.pane)
            .ok_or(Error::StalePane)?;
        Ok(())
    }

    pub fn activate(&mut self, tab: TabId) -> Result<(), Error> {
        self.workspace.tab(tab).ok_or(Error::StaleTab)?;
        self.workspace.active = Some(tab);
        Ok(())
    }

    pub fn focus(&mut self, target: PaneTarget) -> Result<(), Error> {
        self.validate(target)?;
        self.workspace.tabs.get_mut(&target.tab).unwrap().focused = target.pane;
        self.workspace.active = Some(target.tab);
        Ok(())
    }

    /// Swap positions within one tab, keeping pane handles and sessions intact.
    /// Cross-tab moves use move_to_tab, which returns the new scoped handle.
    pub fn swap(&mut self, a: PaneTarget, b: PaneTarget) -> Result<(), Error> {
        self.validate(a)?;
        self.validate(b)?;
        if a.tab != b.tab {
            return Err(Error::StalePane);
        }
        self.workspace
            .tabs
            .get_mut(&a.tab)
            .unwrap()
            .grid
            .swap(a.pane, b.pane);
        Ok(())
    }

    /// Split the destination with an existing session. Complete the fallible
    /// insertion before removing the source. A moved Starting pane still uses
    /// the same reservation; completion never depends on its former location.
    pub fn move_to_tab(
        &mut self,
        source: PaneTarget,
        destination: PaneTarget,
        axis: Axis,
    ) -> Result<PaneTarget, Error> {
        self.validate(source)?;
        self.validate(destination)?;
        if source.tab == destination.tab {
            self.swap(source, destination)?;
            return Ok(source);
        }
        let state = *self.workspace.pane(source).unwrap();
        let tab = self.workspace.tabs.get_mut(&destination.tab).unwrap();
        let pane = tab
            .split(destination.pane, axis, state)
            .ok_or(Error::LayoutExhausted)?;
        self.workspace.remove_pane(source);
        self.workspace.active = Some(destination.tab);
        Ok(PaneTarget {
            tab: destination.tab,
            pane,
        })
    }

    pub fn resize(&mut self, tab: TabId, split: Split, ratio: f32) -> Result<(), Error> {
        let tab = self.workspace.tabs.get_mut(&tab).ok_or(Error::StaleTab)?;
        if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
            return Err(Error::InvalidRatio);
        }
        if !contains_split(tab.grid.layout(), split) {
            return Err(Error::StaleSplit);
        }
        tab.grid.resize(split, ratio.clamp(0.1, 0.9));
        Ok(())
    }

    pub fn zoom(&mut self, target: PaneTarget) -> Result<(), Error> {
        self.validate(target)?;
        let tab = self.workspace.tabs.get_mut(&target.tab).unwrap();
        if tab.grid.maximized() == Some(target.pane) {
            tab.grid.restore();
        } else {
            tab.grid.maximize(target.pane);
        }
        tab.focused = target.pane;
        Ok(())
    }

    pub fn close_pane(&mut self, target: PaneTarget) -> Result<Option<Effect>, Error> {
        self.validate(target)?;
        let session = self.workspace.pane(target).unwrap().session;
        self.workspace.remove_pane(target);
        Ok(self.sessions.close(session))
    }

    pub fn close_tab(&mut self, id: TabId) -> Result<Vec<Effect>, Error> {
        let tab = self.workspace.tab(id).ok_or(Error::StaleTab)?;
        let mut effects = Vec::new();
        for (_, pane) in tab.grid.iter() {
            if let Some(effect) = self.sessions.close(pane.session) {
                effects.push(effect);
            }
        }
        self.workspace.remove_tab(id);
        Ok(effects)
    }

    pub fn spawn_succeeded(&mut self, reservation: Reservation) -> SpawnDisposition {
        self.sessions.spawn_succeeded(reservation)
    }
    pub fn spawn_failed(&mut self, reservation: Reservation, failure: Failure) -> bool {
        self.sessions.spawn_failed(reservation, failure)
    }
    pub fn exited(&mut self, session: SessionId, code: Option<i32>) -> bool {
        self.sessions.exited(session, code)
    }
    pub fn closed(&mut self, session: SessionId) -> bool {
        self.sessions.closed(session)
    }
}

#[cfg(test)]
mod tests;
