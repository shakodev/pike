use super::*;
use iced::widget::pane_grid::{Axis, Node};

fn owner(effect: Effect) -> Reservation {
    let Effect::Spawn(owner) = effect else {
        panic!("expected launch")
    };
    owner
}

#[test]
fn twelve_reordered_tabs_keep_exact_panes_reservations_lifecycle_focus_and_names() {
    let mut model = Model::default();
    let mut panes = Vec::new();
    for index in 0..12 {
        let (pane, launch) = model.create_tab().unwrap();
        let owner = owner(launch);
        model.spawn_succeeded(owner);
        model
            .rename_tab(pane.tab, &format!("Fixture {index} 🦀"))
            .unwrap();
        panes.push(pane);
    }
    let (sibling, launch) = model.split(panes[4], Axis::Vertical).unwrap();
    model.spawn_succeeded(owner(launch));
    let Node::Split { id, .. } = model.workspace().tab(sibling.tab).unwrap().grid().layout() else {
        panic!("missing split")
    };
    model.resize(sibling.tab, *id, 0.37).unwrap();
    model.zoom(sibling).unwrap();
    model.record_activity(
        model
            .sessions()
            .get(model.workspace().pane(panes[2]).unwrap().session)
            .unwrap()
            .reservation(),
        ActivityEvent::Bell,
    );
    let active = model.workspace().active();
    let registry: Vec<_> = model
        .sessions()
        .iter()
        .map(|(id, e)| (id, e.reservation(), e.lifecycle()))
        .collect();
    let states: Vec<_> = panes
        .iter()
        .map(|pane| {
            let tab = model.workspace().tab(pane.tab).unwrap();
            (
                pane.tab,
                format!("{:?}", tab.grid()),
                tab.focused(),
                tab.activity(),
                tab.manual_name().unwrap().as_str().to_owned(),
            )
        })
        .collect();

    // Move the last past the first, the split tab past the last, then reverse
    // the remaining anchors repeatedly. Positions never become owner IDs.
    model
        .reorder_tab(panes[11].tab, panes[0].tab, Placement::Before)
        .unwrap();
    model
        .reorder_tab(panes[4].tab, panes[10].tab, Placement::After)
        .unwrap();
    assert_eq!(model.workspace().order()[0], panes[11].tab);
    assert_eq!(model.workspace().order()[11], panes[4].tab);
    for _ in 0..10 {
        for pair in panes.windows(2) {
            model
                .reorder_tab(pair[0].tab, pair[1].tab, Placement::After)
                .unwrap();
        }
        model
            .reorder_tab(panes[0].tab, panes[0].tab, Placement::Before)
            .unwrap();
        assert_eq!(model.workspace().order().len(), 12);
        assert_eq!(model.workspace().active(), active);
        assert_eq!(
            model
                .sessions()
                .iter()
                .map(|(id, e)| (id, e.reservation(), e.lifecycle()))
                .collect::<Vec<_>>(),
            registry
        );
        for (id, grid, focused, activity, title) in &states {
            let tab = model.workspace().tab(*id).unwrap();
            assert_eq!(format!("{:?}", tab.grid()), *grid);
            assert_eq!(tab.focused(), *focused);
            assert_eq!(tab.activity(), *activity);
            assert_eq!(tab.manual_name().unwrap().as_str(), title);
            for (pane, state) in tab.grid().iter() {
                assert_eq!(
                    model.target(state.session),
                    Some(PaneTarget {
                        tab: *id,
                        pane: *pane
                    })
                );
            }
        }
    }
}

#[test]
fn stale_name_and_either_reorder_owner_are_atomic_after_removal() {
    let mut model = Model::default();
    let (a, _) = model.create_tab().unwrap();
    let (b, _) = model.create_tab().unwrap();
    let (stale, _) = model.create_tab().unwrap();
    model.rename_tab(a.tab, "first").unwrap();
    model.close_tab(stale.tab).unwrap();
    let before = format!("{model:?}");
    assert_eq!(
        model.rename_tab(stale.tab, "new neighbor"),
        Err(Error::StaleTab)
    );
    assert_eq!(model.reset_tab_name(stale.tab), Err(Error::StaleTab));
    for placement in [Placement::Before, Placement::After] {
        assert_eq!(
            model.reorder_tab(a.tab, stale.tab, placement),
            Err(Error::StaleTab)
        );
        assert_eq!(
            model.reorder_tab(stale.tab, b.tab, placement),
            Err(Error::StaleTab)
        );
        assert_eq!(
            model.reorder_tab(stale.tab, stale.tab, placement),
            Err(Error::StaleTab)
        );
    }
    assert_eq!(format!("{model:?}"), before);
    assert_eq!(
        model
            .workspace()
            .tab(a.tab)
            .unwrap()
            .manual_name()
            .unwrap()
            .as_str(),
        "first"
    );
    model.rename_tab(a.tab, " \n\u{202e}\t").unwrap();
    assert!(
        model
            .workspace()
            .tab(a.tab)
            .unwrap()
            .manual_name()
            .is_none()
    );
    model.rename_tab(a.tab, "again").unwrap();
    model.reset_tab_name(a.tab).unwrap();
    assert!(
        model
            .workspace()
            .tab(a.tab)
            .unwrap()
            .manual_name()
            .is_none()
    );
}

#[test]
fn direct_and_cyclic_navigation_cover_twelve_tabs_and_follow_reordered_positions() {
    let mut model = Model::default();
    for selection in [Selection::Index(0), Selection::Next, Selection::Previous] {
        assert_eq!(model.workspace().selection_target(selection), None);
    }
    for _ in 0..12 {
        model.create_tab().unwrap();
    }
    let original = model.workspace().order().to_vec();
    model
        .reorder_tab(original[11], original[0], Placement::Before)
        .unwrap();
    let order = model.workspace().order().to_vec();
    assert_eq!(order[0], original[11]);
    for (index, id) in order.iter().enumerate() {
        assert_eq!(
            model.workspace().selection_target(Selection::Index(index)),
            Some(*id)
        );
        model.activate(*id).unwrap();
        assert_eq!(
            model.workspace().selection_target(Selection::Next),
            Some(order[(index + 1) % 12])
        );
        assert_eq!(
            model.workspace().selection_target(Selection::Previous),
            Some(order[(index + 11) % 12])
        );
    }
    assert_eq!(
        model.workspace().selection_target(Selection::Index(12)),
        None
    );
    assert_eq!(
        model
            .workspace()
            .selection_target(Selection::Index(usize::MAX)),
        None
    );
}

#[test]
fn removal_selects_right_neighbor_then_left_using_current_order_and_preserves_background_focus() {
    let mut model = Model::default();
    let (a, _) = model.create_tab().unwrap();
    let (b, _) = model.create_tab().unwrap();
    let (c, _) = model.create_tab().unwrap();
    let (d, _) = model.create_tab().unwrap();
    model.reorder_tab(d.tab, b.tab, Placement::Before).unwrap();
    assert_eq!(model.workspace().order(), [a.tab, d.tab, b.tab, c.tab]);
    model.close_tab(d.tab).unwrap();
    assert_eq!(model.workspace().active(), Some(b.tab));
    model.close_tab(c.tab).unwrap(); // background removal preserves b
    assert_eq!(model.workspace().active(), Some(b.tab));
    model.close_tab(b.tab).unwrap(); // final position selects left
    assert_eq!(model.workspace().active(), Some(a.tab));
    model.close_tab(a.tab).unwrap();
    assert_eq!(model.workspace().active(), None);
}
