use super::*;
use crate::identity::Reservation;
use crate::session::{Effect, Lifecycle};
use crate::workspace::{Error, Model};
use iced::widget::pane_grid::{Axis, Node};

fn owner(effect: Effect) -> Reservation {
    let Effect::Spawn(owner) = effect else {
        panic!("expected launch")
    };
    owner
}

#[test]
fn ordered_tabs_remember_focus_and_nested_ratios_without_lifecycle_effects() {
    let mut model = Model::default();
    let (first, launch) = model.create_tab().unwrap();
    let initial = owner(launch);
    model.spawn_succeeded(initial);
    let (right, launch) = model.split(first, Axis::Vertical).unwrap();
    model.spawn_succeeded(owner(launch));
    let (down, launch) = model.split(right, Axis::Horizontal).unwrap();
    let down_owner = owner(launch);
    model.spawn_succeeded(down_owner);
    let Node::Split { id, .. } = model.workspace().tab(first.tab).unwrap().grid().layout() else {
        panic!("missing split")
    };
    model.resize(first.tab, *id, 0.37).unwrap();
    model.focus(right).unwrap();
    let tree = format!(
        "{:?}",
        model.workspace().tab(first.tab).unwrap().grid().layout()
    );
    let (second, launch) = model.create_tab().unwrap();
    model.spawn_succeeded(owner(launch));
    let (third, launch) = model.create_tab().unwrap();
    model.spawn_succeeded(owner(launch));
    let order = vec![first.tab, second.tab, third.tab];
    for _ in 0..20 {
        for target in [first, second, third, second, first] {
            model.activate(target.tab).unwrap();
            assert_eq!(model.workspace().order(), order);
            assert_eq!(model.workspace().active(), Some(target.tab));
            assert_eq!(
                model.workspace().tab(first.tab).unwrap().focused(),
                right.pane
            );
            assert_eq!(
                format!(
                    "{:?}",
                    model.workspace().tab(first.tab).unwrap().grid().layout()
                ),
                tree
            );
            assert_eq!(model.target(initial.session), Some(first));
            assert_eq!(model.target(down_owner.session), Some(down));
            assert!(
                model
                    .sessions()
                    .iter()
                    .all(|(_, e)| e.lifecycle() == Lifecycle::Running)
            );
        }
    }
    model.close_tab(second.tab).unwrap();
    assert_eq!(model.workspace().order(), [first.tab, third.tab]);
    assert_eq!(model.workspace().active(), Some(first.tab));
    assert_eq!(model.activate(second.tab), Err(Error::StaleTab));
    assert_eq!(model.workspace().active(), Some(first.tab));
}

#[test]
fn hidden_activity_coalesces_and_selection_clears_only_that_tab() {
    let mut model = Model::default();
    let (a, launch) = model.create_tab().unwrap();
    let a_owner = owner(launch);
    model.spawn_succeeded(a_owner);
    let (b, launch) = model.create_tab().unwrap();
    let b_owner = owner(launch);
    model.spawn_succeeded(b_owner);
    assert!(model.record_activity(a_owner, ActivityEvent::Output));
    assert!(model.record_activity(a_owner, ActivityEvent::Bell));
    assert!(model.exited(a_owner.session, Some(7)));
    for _ in 0..1000 {
        assert!(!model.record_activity(a_owner, ActivityEvent::Output));
        assert!(!model.record_activity(a_owner, ActivityEvent::Bell));
    }
    assert_eq!(model.workspace().active(), Some(b.tab));
    assert_eq!(
        model.workspace().tab(a.tab).unwrap().activity(),
        Activity {
            unread: true,
            bell: true,
            exited: true
        }
    );
    assert!(!model.record_activity(b_owner, ActivityEvent::Output));
    model.activate(a.tab).unwrap();
    assert_eq!(
        model.workspace().tab(a.tab).unwrap().activity(),
        Activity::default()
    );
    model.record_activity(b_owner, ActivityEvent::Output);
    model.focus(b).unwrap();
    assert_eq!(
        model.workspace().tab(b.tab).unwrap().activity(),
        Activity::default()
    );
    assert_eq!(
        model.sessions().get(a_owner.session).unwrap().lifecycle(),
        Lifecycle::Exited { code: Some(7) }
    );
}

#[test]
fn stale_closed_starting_and_restarted_owners_cannot_mark_activity() {
    let mut model = Model::default();
    let (a, launch) = model.create_tab().unwrap();
    let old = owner(launch);
    let (b, launch) = model.create_tab().unwrap();
    let current = owner(launch);
    model.spawn_succeeded(current);
    assert!(!model.record_activity(old, ActivityEvent::Bell)); // not attached
    model.spawn_succeeded(old);
    let mismatched = Reservation {
        session: old.session,
        request: current.request,
    };
    assert!(!model.record_activity(mismatched, ActivityEvent::Output));
    model.exited(old.session, Some(0));
    let next = owner(model.restart(a).unwrap());
    model.spawn_succeeded(next);
    // Discard the earlier exit indicator before testing the stale delivery.
    model.activate(a.tab).unwrap();
    model.activate(b.tab).unwrap();
    for event in [
        ActivityEvent::Output,
        ActivityEvent::Bell,
        ActivityEvent::Exit,
    ] {
        assert!(!model.record_activity(old, event));
        assert_eq!(
            model.workspace().tab(a.tab).unwrap().activity(),
            Activity::default()
        );
    }
    assert!(model.record_activity(next, ActivityEvent::Output));
    model.close_tab(a.tab).unwrap();
    assert!(!model.record_activity(next, ActivityEvent::Bell));
    assert_eq!(model.workspace().active(), Some(b.tab));
    assert_eq!(
        model.workspace().tab(b.tab).unwrap().activity(),
        Activity::default()
    );
}

#[test]
fn closing_active_tab_acknowledges_fallback_but_hidden_exit_never_selects_it() {
    let mut model = Model::default();
    let (a, launch) = model.create_tab().unwrap();
    let original = owner(launch);
    model.spawn_succeeded(original);
    let (b, _) = model.create_tab().unwrap();
    model.exited(original.session, Some(9));
    assert_eq!(model.workspace().active(), Some(b.tab));
    assert!(model.workspace().tab(a.tab).unwrap().activity().exited);
    model.close_tab(b.tab).unwrap();
    assert_eq!(model.workspace().active(), Some(a.tab));
    assert_eq!(
        model.workspace().tab(a.tab).unwrap().activity(),
        Activity::default()
    );
    model.close_tab(a.tab).unwrap();
    assert!(model.workspace().order().is_empty());
    assert_eq!(model.workspace().active(), None);
}
