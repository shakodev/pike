//! Header moves use the pinned grid's drop regions, without touching sessions.
use iced::widget::pane_grid::Target;

use super::{Error, Model};
use crate::layout::PaneTarget;

impl Model {
    /// A target pane belongs to the source tab; grid-edge targets apply to that
    /// same tab. Validate before State::drop, whose edge path removes its source
    /// before inserting it. Commit only a complete ownership-preserving tree.
    pub fn drop_pane(&mut self, source: PaneTarget, target: Target) -> Result<(), Error> {
        self.validate(source)?;
        if let Target::Pane(pane, _) = target {
            self.validate(PaneTarget {
                tab: source.tab,
                pane,
            })?;
            if pane == source.pane {
                return Err(Error::StalePane);
            }
        }
        let tab = self.workspace.tabs.get_mut(&source.tab).unwrap();
        if tab.grid.len() < 2 || tab.grid.maximized().is_some() {
            return Err(Error::Geometry);
        }
        let mut next = tab.grid.clone();
        next.drop(source.pane, target);
        if next.panes != tab.grid.panes {
            return Err(Error::LayoutExhausted);
        }
        tab.grid = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::pane_grid::{Axis, Edge, Region};

    fn fixture() -> (Model, [PaneTarget; 3]) {
        let mut model = Model::default();
        let (a, _) = model.create_tab().unwrap();
        let (b, _) = model.split(a, Axis::Vertical).unwrap();
        let (c, _) = model.split(a, Axis::Horizontal).unwrap();
        (model, [a, b, c])
    }

    #[test]
    fn every_pinned_region_preserves_handles_reservations_focus_and_lifecycles() {
        for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
            for whole_grid in [false, true] {
                let (mut model, [a, b, c]) = fixture();
                let before = model.workspace.tab(a.tab).unwrap().grid.clone();
                let owners: Vec<_> = model
                    .sessions
                    .iter()
                    .map(|(id, entry)| (id, entry.reservation(), entry.lifecycle()))
                    .collect();
                let target = if whole_grid {
                    Target::Edge(edge)
                } else {
                    Target::Pane(b.pane, Region::Edge(edge))
                };
                let mut pinned = before.clone();
                pinned.drop(a.pane, target);
                model.drop_pane(a, target).unwrap();
                let tab = model.workspace.tab(a.tab).unwrap();
                assert_eq!(tab.grid.panes, before.panes);
                assert_eq!(
                    format!("{:?}", tab.grid.layout()),
                    format!("{:?}", pinned.layout())
                );
                assert_eq!(tab.focused(), c.pane);
                assert_eq!(
                    owners,
                    model
                        .sessions
                        .iter()
                        .map(|(id, entry)| (id, entry.reservation(), entry.lifecycle()))
                        .collect::<Vec<_>>()
                );
                for pane in [a, b, c] {
                    let session = model.workspace.pane(pane).unwrap().session;
                    assert_eq!(model.target(session), Some(pane));
                }
            }
        }
    }

    #[test]
    fn center_drop_is_the_pinned_swap_and_zoom_restores_the_moved_tree() {
        let (mut model, [a, b, c]) = fixture();
        let before = model.workspace.tab(a.tab).unwrap().grid.clone();
        let mut swapped = before.clone();
        swapped.swap(a.pane, b.pane);
        model
            .drop_pane(a, Target::Pane(b.pane, Region::Center))
            .unwrap();
        let moved = format!("{:?}", swapped.layout());
        assert_eq!(
            format!("{:?}", model.workspace.tab(a.tab).unwrap().grid.layout()),
            moved
        );
        model.focus(c).unwrap();
        model.zoom(a).unwrap();
        assert_eq!(model.workspace.tab(a.tab).unwrap().focused(), a.pane);
        model.zoom(a).unwrap();
        let tab = model.workspace.tab(a.tab).unwrap();
        assert_eq!(tab.grid.maximized(), None);
        assert_eq!(tab.focused(), a.pane);
        assert_eq!(format!("{:?}", tab.grid.layout()), moved);
        assert_eq!(tab.grid.panes, before.panes);
    }

    #[test]
    fn invalid_stale_self_and_zoomed_targets_are_atomic() {
        let (mut model, [a, b, c]) = fixture();
        model.close_pane(c).unwrap();
        let (other, _) = model.create_tab().unwrap();
        model.activate(a.tab).unwrap();
        let before = format!("{:?}", model);
        assert_eq!(
            model.drop_pane(c, Target::Edge(Edge::Top)),
            Err(Error::StalePane)
        );
        assert_eq!(
            model.drop_pane(a, Target::Pane(c.pane, Region::Center)),
            Err(Error::StalePane)
        );
        assert_eq!(
            model.drop_pane(a, Target::Pane(a.pane, Region::Edge(Edge::Left))),
            Err(Error::StalePane)
        );
        assert_eq!(before, format!("{:?}", model));
        model.zoom(a).unwrap();
        let before = format!("{:?}", model);
        assert_eq!(
            model.drop_pane(a, Target::Pane(b.pane, Region::Center)),
            Err(Error::Geometry)
        );
        assert_eq!(before, format!("{:?}", model));
        model.close_tab(other.tab).unwrap();
        let before = format!("{:?}", model);
        assert_eq!(
            model.drop_pane(other, Target::Edge(Edge::Left)),
            Err(Error::StaleTab)
        );
        assert_eq!(before, format!("{:?}", model));
    }
}
