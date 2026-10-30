use super::*;
use crate::layout::TabState;
use crate::session::Lifecycle;
use iced::widget::pane_grid::{self, Node, Pane};
use std::collections::BTreeSet;

#[test]
fn guarded_splits_use_exact_half_ratios_and_preserve_the_source() {
    use iced::Size;
    for axis in [Axis::Vertical, Axis::Horizontal] {
        let mut model = Model::default();
        let (source, effect) = model.create_tab().unwrap();
        let original = reservation(effect);
        model.spawn_succeeded(original);
        let geometry = SplitGeometry {
            area: Size::new(408.0, 264.0),
            minimum: Size::new(202.0, 130.0),
            spacing: 4.0,
        };
        let (created, effect) = model.split_with_geometry(source, axis, geometry).unwrap();
        let next = reservation(effect);
        assert_ne!(next.session, original.session);
        assert_ne!(next.request, original.request);
        assert_eq!(
            model.workspace().pane(source).unwrap().session,
            original.session
        );
        assert_eq!(
            model.sessions().get(original.session).unwrap().lifecycle(),
            Lifecycle::Running
        );
        assert_eq!(model.target(next.session), Some(created));
        let Node::Split {
            axis: actual,
            ratio,
            ..
        } = model.workspace().tab(source.tab).unwrap().grid().layout()
        else {
            panic!("missing split")
        };
        assert_eq!((*actual, *ratio), (axis, 0.5));
        invariants(&model);
    }
}

#[test]
fn geometry_refusal_is_atomic_for_rounding_nested_and_invalid_sizes() {
    use iced::Size;
    let mut model = Model::default();
    let (source, _) = model.create_tab().unwrap();
    let mut geometry = SplitGeometry {
        area: Size::new(408.0, 264.0),
        minimum: Size::new(202.0, 130.0),
        spacing: 4.0,
    };
    for area in [
        Size::new(407.0, 264.0),
        Size::new(408.0, 129.0),
        Size::ZERO,
        Size::new(f32::NAN, 500.0),
        Size::new(500.0, f32::INFINITY),
    ] {
        geometry.area = area;
        assert_eq!(
            model.split_with_geometry(source, Axis::Vertical, geometry),
            Err(Error::Geometry)
        );
        assert_eq!(model.sessions().len(), 1);
        assert_eq!(
            model.workspace().tab(source.tab).unwrap().focused(),
            source.pane
        );
    }
    geometry.area = Size::new(408.0, 264.0);
    let (right, _) = model
        .split_with_geometry(source, Axis::Vertical, geometry)
        .unwrap();
    assert_eq!(
        model.split_with_geometry(right, Axis::Vertical, geometry),
        Err(Error::Geometry)
    );
    let (down, _) = model
        .split_with_geometry(right, Axis::Horizontal, geometry)
        .unwrap();
    assert_eq!(
        model.split_with_geometry(down, Axis::Horizontal, geometry),
        Err(Error::Geometry)
    );
    assert_eq!(model.sessions().len(), 3);
    invariants(&model);
}

#[test]
fn retained_budget_is_32_and_in_place_retry_preserves_layout_at_capacity() {
    let mut model = Model::default();
    let (source, first) = model.create_tab().unwrap();
    let first = reservation(first);
    model.spawn_succeeded(first);
    let mut last = source;
    for _ in 1..32 {
        let (target, spawn) = model.split(source, Axis::Vertical).unwrap();
        model.spawn_failed(reservation(spawn), Failure::Spawn);
        last = target;
    }
    assert_eq!(model.sessions().len(), 32);
    assert_eq!(
        model.split(source, Axis::Horizontal),
        Err(Error::SessionLimit)
    );
    let tree = format!(
        "{:?}",
        model.workspace().tab(source.tab).unwrap().grid().layout()
    );
    let old = model.workspace().pane(last).unwrap().session;
    let retry = reservation(model.restart(last).unwrap());
    assert_ne!(retry.session, old);
    assert!(model.sessions().get(old).is_none());
    assert_eq!(model.sessions().len(), 32);
    assert_eq!(
        tree,
        format!(
            "{:?}",
            model.workspace().tab(source.tab).unwrap().grid().layout()
        )
    );
    assert_eq!(model.restart(source), Err(Error::NotRestartable));
    assert_eq!(
        model.workspace().pane(source).unwrap().session,
        first.session
    );
    invariants(&model);
}

fn reservation(effect: Effect) -> Reservation {
    match effect {
        Effect::Spawn(reservation) => reservation,
        Effect::Close(_) => panic!("expected spawn"),
    }
}

fn targets(model: &Model) -> Vec<PaneTarget> {
    model
        .workspace
        .order()
        .iter()
        .flat_map(|tab| {
            model
                .workspace
                .tab(*tab)
                .unwrap()
                .grid()
                .iter()
                .map(|(pane, _)| PaneTarget {
                    tab: *tab,
                    pane: *pane,
                })
        })
        .collect()
}

fn leaves(node: &Node, panes: &mut BTreeSet<Pane>) {
    match node {
        Node::Pane(pane) => assert!(panes.insert(*pane), "duplicated tree leaf"),
        Node::Split { a, b, ratio, .. } => {
            assert!(ratio.is_finite() && (0.1..=0.9).contains(ratio));
            leaves(a, panes);
            leaves(b, panes);
        }
    }
}

/// Independent invariant traversal of both the canonical tree and registry.
fn invariants(model: &Model) {
    let workspace = model.workspace();
    let order: BTreeSet<_> = workspace.order().iter().copied().collect();
    assert_eq!(order.len(), workspace.order().len());
    assert_eq!(order, workspace.tabs.keys().copied().collect());
    assert_eq!(workspace.active().is_none(), order.is_empty());
    if let Some(active) = workspace.active() {
        assert!(order.contains(&active));
    }
    let mut displayed = BTreeSet::new();
    for tab in workspace.tabs.values() {
        assert!(!tab.grid().is_empty());
        let mut tree = BTreeSet::new();
        leaves(tab.grid().layout(), &mut tree);
        assert_eq!(tree, tab.grid().iter().map(|(p, _)| *p).collect());
        assert!(tree.contains(&tab.focused()));
        if let Some(pane) = tab.grid().maximized() {
            assert!(tree.contains(&pane));
            assert_eq!(
                pane,
                tab.focused(),
                "focus must remain visible while zoomed"
            );
        }
        for (_, pane) in tab.grid().iter() {
            assert!(
                displayed.insert(pane.session),
                "duplicated displayed session"
            );
            let entry = model
                .sessions()
                .get(pane.session)
                .expect("missing registry entry");
            assert!(!matches!(entry.lifecycle(), Lifecycle::Closing { .. }));
        }
    }
    for (id, entry) in model.sessions().iter() {
        assert_eq!(id, entry.reservation().session);
        assert_eq!(
            displayed.contains(&id),
            !matches!(entry.lifecycle(), Lifecycle::Closing { .. })
        );
    }
    assert!(model.sessions().len() <= usize::from(model.limit.get()));
}

#[test]
fn focus_hidden_pane_restores_layout_and_refocusing_visible_pane_preserves_zoom() {
    let mut model = Model::default();
    let (a, _) = model.create_tab().unwrap();
    let (b, _) = model.split(a, Axis::Vertical).unwrap();
    model.zoom(a).unwrap();
    invariants(&model);
    model.focus(a).unwrap();
    let tab = model.workspace.tab(a.tab).unwrap();
    assert_eq!(tab.grid().maximized(), Some(a.pane));
    assert_eq!(tab.focused(), a.pane);
    invariants(&model);
    let sessions: Vec<_> = model
        .sessions
        .iter()
        .map(|(id, entry)| (id, entry.lifecycle()))
        .collect();
    model.focus(b).unwrap();
    let tab = model.workspace.tab(a.tab).unwrap();
    assert_eq!(tab.grid().maximized(), None);
    assert_eq!(tab.focused(), b.pane);
    assert_eq!(
        sessions,
        model
            .sessions
            .iter()
            .map(|(id, entry)| (id, entry.lifecycle()))
            .collect::<Vec<_>>()
    );
    invariants(&model);
}

#[test]
fn zoom_nonfocused_pane_synchronizes_focus_when_maximizing_switching_and_restoring() {
    let mut model = Model::default();
    let (a, _) = model.create_tab().unwrap();
    let (b, _) = model.split(a, Axis::Vertical).unwrap();
    model.focus(a).unwrap();
    model.zoom(b).unwrap();
    let tab = model.workspace.tab(a.tab).unwrap();
    assert_eq!(tab.focused(), b.pane);
    assert_eq!(tab.grid().maximized(), Some(b.pane));
    invariants(&model);
    model.zoom(a).unwrap();
    let tab = model.workspace.tab(a.tab).unwrap();
    assert_eq!(tab.focused(), a.pane);
    assert_eq!(tab.grid().maximized(), Some(a.pane));
    invariants(&model);
    model.zoom(a).unwrap();
    let tab = model.workspace.tab(a.tab).unwrap();
    assert_eq!(tab.focused(), a.pane);
    assert_eq!(tab.grid().maximized(), None);
    invariants(&model);
}

#[test]
fn background_zoom_activation_and_focus_keep_the_selected_pane_visible() {
    let mut model = Model::default();
    let (a, _) = model.create_tab().unwrap();
    let (b, _) = model.split(a, Axis::Vertical).unwrap();
    let (foreground, _) = model.create_tab().unwrap();
    model.zoom(a).unwrap();
    assert_eq!(model.workspace.active(), Some(foreground.tab));
    assert_eq!(model.workspace.tab(a.tab).unwrap().focused(), a.pane);
    invariants(&model);
    model.activate(a.tab).unwrap();
    assert_eq!(
        model.workspace.tab(a.tab).unwrap().grid().maximized(),
        Some(a.pane)
    );
    invariants(&model);
    model.activate(foreground.tab).unwrap();
    model.focus(b).unwrap();
    assert_eq!(model.workspace.active(), Some(b.tab));
    let tab = model.workspace.tab(b.tab).unwrap();
    assert_eq!(tab.focused(), b.pane);
    assert_eq!(tab.grid().maximized(), None);
    invariants(&model);
    model.zoom(b).unwrap();
    model.activate(foreground.tab).unwrap();
    model.focus(b).unwrap();
    assert_eq!(model.workspace.active(), Some(b.tab));
    assert_eq!(
        model.workspace.tab(b.tab).unwrap().grid().maximized(),
        Some(b.pane)
    );
    invariants(&model);
}

#[test]
fn split_move_swap_and_remove_while_zoomed_preserve_visible_focus() {
    let mut model = Model::default();
    let (a, effect) = model.create_tab().unwrap();
    let a_ids = reservation(effect);
    let (b, _) = model.split(a, Axis::Vertical).unwrap();
    model.zoom(a).unwrap();
    let (c, _) = model.split(b, Axis::Horizontal).unwrap();
    let tab = model.workspace.tab(a.tab).unwrap();
    assert_eq!(tab.focused(), c.pane);
    assert_eq!(tab.grid().maximized(), None);
    invariants(&model);
    model.zoom(a).unwrap();
    model.swap(a, b).unwrap();
    assert_eq!(
        model.workspace.tab(a.tab).unwrap().grid().maximized(),
        Some(a.pane)
    );
    invariants(&model);
    model.move_to_tab(a, b, Axis::Vertical).unwrap(); // Same-tab positional swap.
    assert_eq!(model.workspace.tab(a.tab).unwrap().focused(), a.pane);
    invariants(&model);

    let (destination, _) = model.create_tab().unwrap();
    model.zoom(destination).unwrap();
    let sessions: Vec<_> = model
        .sessions
        .iter()
        .map(|(id, entry)| (id, entry.lifecycle()))
        .collect();
    let moved_b = model.move_to_tab(b, destination, Axis::Vertical).unwrap();
    assert_eq!(
        model.workspace.tab(a.tab).unwrap().grid().maximized(),
        Some(a.pane)
    );
    let tab = model.workspace.tab(destination.tab).unwrap();
    assert_eq!(tab.focused(), moved_b.pane);
    assert_eq!(tab.grid().maximized(), None);
    invariants(&model);

    model.zoom(destination).unwrap();
    let moved_a = model.move_to_tab(a, destination, Axis::Horizontal).unwrap();
    let source = model.workspace.tab(c.tab).unwrap();
    assert_eq!(source.focused(), c.pane);
    assert_eq!(source.grid().maximized(), None);
    let destination = model.workspace.tab(moved_a.tab).unwrap();
    assert_eq!(destination.focused(), moved_a.pane);
    assert_eq!(destination.grid().maximized(), None);
    assert_eq!(
        sessions,
        model
            .sessions
            .iter()
            .map(|(id, entry)| (id, entry.lifecycle()))
            .collect::<Vec<_>>()
    );
    invariants(&model);
    model.zoom(moved_a).unwrap();
    assert_eq!(model.close_pane(moved_a), Ok(Some(Effect::Close(a_ids))));
    assert_eq!(
        model.workspace.tab(moved_a.tab).unwrap().grid().maximized(),
        None
    );
    invariants(&model);
}

#[test]
fn final_pane_deletes_tab_and_final_tab_leaves_empty_window() {
    let mut model = Model::default();
    let (first, effect) = model.create_tab().unwrap();
    let first_ids = reservation(effect);
    let (second, effect) = model.create_tab().unwrap();
    let second_ids = reservation(effect);
    // Both pane grids begin with Pane(0). Tab scope keeps their identities apart.
    assert_eq!(first.pane, second.pane);
    assert_ne!(first.tab, second.tab);
    assert_ne!(first_ids.session, second_ids.session);
    model.focus(first).unwrap();
    assert_eq!(model.workspace.active(), Some(first.tab));
    assert_eq!(model.close_pane(first), Ok(Some(Effect::Close(first_ids))));
    assert_eq!(model.workspace.active(), Some(second.tab));
    assert_eq!(model.close_pane(first), Err(Error::StaleTab));
    assert_eq!(
        model.close_tab(second.tab),
        Ok(vec![Effect::Close(second_ids)])
    );
    assert_eq!(model.close_tab(second.tab), Err(Error::StaleTab));
    assert!(model.workspace.order().is_empty());
    assert_eq!(model.workspace.active(), None);
    invariants(&model);
    for ids in [first_ids, second_ids] {
        assert!(model.closed(ids.session));
        assert!(!model.closed(ids.session));
        assert_eq!(model.spawn_succeeded(ids), SpawnDisposition::Dispose);
    }
    assert!(model.sessions.is_empty());
    let (next, effect) = model.create_tab().unwrap();
    assert!(next.tab.get() > second.tab.get());
    assert!(reservation(effect).session.get() > second_ids.session.get());
    invariants(&model);
}

#[test]
fn background_exit_targets_session_and_preserves_layout_and_focus() {
    let mut model = Model::default();
    let (background, spawn) = model.create_tab().unwrap();
    let ids = reservation(spawn);
    assert_eq!(model.spawn_succeeded(ids), SpawnDisposition::Attach);
    let (foreground, _) = model.create_tab().unwrap();
    assert!(model.exited(ids.session, Some(37)));
    assert_eq!(model.workspace.active(), Some(foreground.tab));
    assert_eq!(
        model.workspace.pane(background).unwrap().session,
        ids.session
    );
    assert_eq!(
        model.sessions.get(ids.session).unwrap().lifecycle(),
        Lifecycle::Exited { code: Some(37) }
    );
    assert!(!model.exited(ids.session, None));
    assert!(!model.closed(ids.session));
    assert_eq!(model.close_pane(background), Ok(None));
    assert!(model.sessions.get(ids.session).is_none());
    invariants(&model);
}

#[test]
fn failed_split_keeps_source_and_failed_or_canceled_reservations_are_bounded() {
    let mut model = Model::new(NonZeroU16::new(2).unwrap());
    let (source, spawn) = model.create_tab().unwrap();
    let source_ids = reservation(spawn);
    assert_eq!(model.spawn_succeeded(source_ids), SpawnDisposition::Attach);
    let (failed, spawn) = model.split(source, Axis::Vertical).unwrap();
    let failed_ids = reservation(spawn);
    assert!(model.spawn_failed(failed_ids, Failure::Admission));
    assert_eq!(
        model.sessions.get(source_ids.session).unwrap().lifecycle(),
        Lifecycle::Running
    );
    assert_eq!(
        model.split(source, Axis::Horizontal),
        Err(Error::SessionLimit)
    );
    assert_eq!(model.close_pane(failed), Ok(None));
    let (canceled, spawn) = model.split(source, Axis::Horizontal).unwrap();
    let canceled_ids = reservation(spawn);
    assert!(canceled_ids.session.get() > failed_ids.session.get());
    assert_eq!(
        model.close_pane(canceled),
        Ok(Some(Effect::Close(canceled_ids)))
    );
    assert_eq!(model.create_tab(), Err(Error::SessionLimit));
    assert_eq!(
        model.spawn_succeeded(canceled_ids),
        SpawnDisposition::Dispose
    );
    assert!(!model.spawn_failed(canceled_ids, Failure::Canceled));
    assert!(model.closed(canceled_ids.session));
    assert!(!model.spawn_failed(canceled_ids, Failure::Canceled));
    assert_eq!(
        model.spawn_succeeded(canceled_ids),
        SpawnDisposition::Dispose
    );
    assert_eq!(model.sessions.len(), 1);
    let (pending, spawn) = model.split(source, Axis::Vertical).unwrap();
    let pending_ids = reservation(spawn);
    assert_eq!(
        model.close_pane(pending),
        Ok(Some(Effect::Close(pending_ids)))
    );
    assert!(model.spawn_failed(pending_ids, Failure::Canceled));
    assert_eq!(
        model.spawn_succeeded(pending_ids),
        SpawnDisposition::Dispose
    );
    assert_eq!(model.sessions.len(), 1);
    invariants(&model);
}

#[test]
fn late_or_mismatched_results_never_attach_or_remove_another_entry() {
    let mut model = Model::default();
    let (first, effect) = model.create_tab().unwrap();
    let first_ids = reservation(effect);
    let (_, effect) = model.create_tab().unwrap();
    let second_ids = reservation(effect);
    let mismatch = Reservation {
        request: second_ids.request,
        ..first_ids
    };
    assert_eq!(model.spawn_succeeded(mismatch), SpawnDisposition::Dispose);
    assert!(!model.spawn_failed(mismatch, Failure::Spawn));
    assert_eq!(model.spawn_succeeded(first_ids), SpawnDisposition::Attach);
    assert_eq!(model.spawn_succeeded(first_ids), SpawnDisposition::Dispose);
    assert_eq!(model.close_pane(first), Ok(Some(Effect::Close(first_ids))));
    // A duplicate failure must not acknowledge disposal of an attached backend.
    assert!(!model.spawn_failed(first_ids, Failure::Spawn));
    assert!(!model.exited(first_ids.session, Some(0)));
    assert!(model.sessions.get(first_ids.session).is_some());
    assert!(model.closed(first_ids.session));
    assert_eq!(model.spawn_succeeded(first_ids), SpawnDisposition::Dispose);
    assert_eq!(
        model.sessions.get(second_ids.session).unwrap().lifecycle(),
        Lifecycle::Starting
    );
    invariants(&model);
}

#[test]
fn layout_edits_preserve_reservations_and_reject_removed_handles() {
    let mut model = Model::default();
    let (source, effect) = model.create_tab().unwrap();
    let ids = reservation(effect);
    let (destination, _) = model.create_tab().unwrap();
    let moved = model
        .move_to_tab(source, destination, Axis::Horizontal)
        .unwrap();
    assert!(model.workspace.tab(source.tab).is_none());
    assert_eq!(model.focus(source), Err(Error::StaleTab));
    assert_eq!(model.workspace.pane(moved).unwrap().session, ids.session);
    assert_eq!(model.spawn_succeeded(ids), SpawnDisposition::Attach);
    let lifecycle: Vec<_> = model
        .sessions
        .iter()
        .map(|(id, e)| (id, e.lifecycle()))
        .collect();
    model.swap(moved, destination).unwrap();
    model.zoom(moved).unwrap();
    let split = *model
        .workspace
        .tab(moved.tab)
        .unwrap()
        .grid()
        .layout()
        .splits()
        .next()
        .unwrap();
    model.resize(moved.tab, split, 0.0).unwrap();
    assert_eq!(
        model.resize(moved.tab, split, f32::NAN),
        Err(Error::InvalidRatio)
    );
    model.zoom(moved).unwrap();
    assert_eq!(
        lifecycle,
        model
            .sessions
            .iter()
            .map(|(id, e)| (id, e.lifecycle()))
            .collect::<Vec<_>>()
    );
    assert_eq!(model.close_pane(moved), Ok(Some(Effect::Close(ids))));
    assert_eq!(model.focus(moved), Err(Error::StalePane));
    assert_eq!(
        model.resize(destination.tab, split, 0.5),
        Err(Error::StaleSplit)
    );
    assert_eq!(model.zoom(moved), Err(Error::StalePane));
    invariants(&model);
}

fn exhaust_layout(tab: &mut TabState) -> Pane {
    let session = tab.grid.iter().next().unwrap().1.session;
    let mut panes = BTreeMap::new();
    let internal = pane_grid::state::Internal::from_configuration(
        &mut panes,
        pane_grid::Configuration::Pane(PaneState { session }),
        usize::MAX - 1,
    );
    let pane = *panes.keys().next().unwrap();
    tab.grid = pane_grid::State { panes, internal };
    tab.focused = pane;
    pane
}

#[test]
fn layout_overflow_does_not_orphan_a_session_or_remove_move_source() {
    let mut model = Model::default();
    let (source, _) = model.create_tab().unwrap();
    let (mut destination, _) = model.create_tab().unwrap();
    destination.pane = exhaust_layout(model.workspace.tabs.get_mut(&destination.tab).unwrap());
    model.zoom(source).unwrap();
    model.zoom(destination).unwrap();
    assert_eq!(
        model.split(destination, Axis::Vertical),
        Err(Error::LayoutExhausted)
    );
    assert_eq!(
        model.move_to_tab(source, destination, Axis::Vertical),
        Err(Error::LayoutExhausted)
    );
    assert!(model.workspace.pane(source).is_some());
    assert_eq!(model.sessions.len(), 2);
    invariants(&model);
}

#[test]
fn closing_background_tab_does_not_change_active_tab_and_closes_exact_sessions() {
    let mut model = Model::default();
    let (a, spawn) = model.create_tab().unwrap();
    let a_ids = reservation(spawn);
    let (_, spawn) = model.split(a, Axis::Vertical).unwrap();
    let b_ids = reservation(spawn);
    let (active, _) = model.create_tab().unwrap();
    assert_eq!(
        model.close_tab(a.tab),
        Ok(vec![Effect::Close(a_ids), Effect::Close(b_ids)])
    );
    assert_eq!(model.workspace.active(), Some(active.tab));
    assert_eq!(model.close_tab(a.tab), Err(Error::StaleTab));
    invariants(&model);
}

#[test]
fn launch_claim_follows_session_across_moves_and_cannot_be_reissued() {
    let mut model = Model::default();
    let (source, effect) = model.create_tab().unwrap();
    let ids = reservation(effect);
    let (destination, _) = model.create_tab().unwrap();
    let moved = model
        .move_to_tab(source, destination, Axis::Vertical)
        .unwrap();
    let token = model.claim_launch(ids).unwrap();
    assert_eq!(token.reservation(), ids);
    assert!(model.claim_launch(ids).is_none());
    assert!(model.accepts_spawn(ids));
    invariants(&model);
    model.swap(moved, destination).unwrap();
    assert!(model.claim_launch(ids).is_none());
    assert!(model.accepts_spawn(ids));
    invariants(&model);
    model.close_pane(moved).unwrap();
    assert!(!model.accepts_spawn(ids));
    assert!(model.claim_launch(ids).is_none());
    assert!(model.spawn_failed(ids, Failure::Canceled));
    invariants(&model);
}

#[test]
fn deterministic_generated_transactions_preserve_invariants_after_every_step() {
    for seed in 1..=32_u64 {
        let mut model = Model::new(NonZeroU16::new(16).unwrap());
        let mut random = seed;
        let mut history = Vec::new();
        let mut handles = Vec::new();
        let mut issued = BTreeSet::new();
        let mut closed = BTreeSet::new();
        let mut claimed = BTreeSet::new();
        let mut last_id = 0;
        for _ in 0..2048 {
            random = random
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let choice = (random >> 32) as usize;
            let current = targets(&model);
            let target = current.get(choice % current.len().max(1)).copied();
            let ids = history.get((choice / 17) % history.len().max(1)).copied();
            let mut spawn = None;
            let mut effects = Vec::new();
            match (choice % 16, target) {
                (0 | 1, _) => spawn = model.create_tab().ok(),
                (2, Some(t)) => spawn = model.split(t, Axis::Vertical).ok(),
                (3, Some(t)) => {
                    model.focus(t).unwrap();
                }
                (4, Some(t)) => {
                    model.zoom(t).unwrap();
                }
                (5, Some(t)) => {
                    let other = current[(choice / 31) % current.len()];
                    if let Ok(moved) = model.move_to_tab(t, other, Axis::Horizontal) {
                        handles.push(moved);
                    }
                }
                (6, Some(t)) => {
                    let split = model
                        .workspace
                        .tab(t.tab)
                        .unwrap()
                        .grid()
                        .layout()
                        .splits()
                        .next()
                        .copied();
                    if let Some(split) = split {
                        model
                            .resize(t.tab, split, (choice % 101) as f32 / 100.0)
                            .unwrap();
                    }
                }
                (7, Some(t)) => {
                    effects.extend(model.close_pane(t).unwrap());
                }
                (8, Some(t)) => {
                    effects = model.close_tab(t.tab).unwrap();
                }
                (9, _) => {
                    if let Some(ids) = ids {
                        model.spawn_succeeded(ids);
                    }
                }
                (10, _) => {
                    if let Some(ids) = ids {
                        model.spawn_failed(ids, Failure::Spawn);
                    }
                }
                (11, _) => {
                    if let Some(ids) = ids {
                        model.exited(ids.session, None);
                    }
                }
                (12, _) => {
                    if let Some(ids) = ids {
                        model.closed(ids.session);
                    }
                }
                (13, _) => {
                    if let Some(t) = handles.get(choice % handles.len().max(1)).copied() {
                        let _ = model.close_pane(t).map(|effect| effects.extend(effect));
                    }
                }
                (14, _) => {
                    if let (Some(ids), Some(other)) = (ids, history.last()) {
                        let malformed = Reservation {
                            request: other.request,
                            ..ids
                        };
                        model.spawn_succeeded(malformed);
                    }
                }
                (15, _) => {
                    if let Some(ids) = ids
                        && let Some(_token) = model.claim_launch(ids)
                    {
                        assert!(claimed.insert(ids.session));
                        assert!(model.claim_launch(ids).is_none());
                        // Simulate an adapter abandoning the claimed token.
                        assert!(model.spawn_failed(ids, Failure::Canceled));
                    }
                }
                _ => {}
            }
            if let Some((handle, effect)) = spawn {
                let ids = reservation(effect);
                assert!(ids.session.get() > last_id);
                last_id = ids.session.get();
                assert!(issued.insert(ids.session));
                history.push(ids);
                handles.push(handle);
            }
            for effect in effects {
                let Effect::Close(ids) = effect else {
                    panic!("unexpected spawn");
                };
                assert!(issued.contains(&ids.session));
                assert!(closed.insert(ids.session), "duplicate close effect");
            }
            invariants(&model);
        }
    }
}
