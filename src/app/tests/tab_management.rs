use super::*;
use crate::tab_strip::TabAction;
use crate::tabs::{Placement, Selection, TITLE_LIMIT};

fn model_app() -> App {
    let mut app = App::default();
    app.window_focus.set_focused(true);
    for _ in 0..12 {
        app.model.create_tab().unwrap();
    }
    app
}

#[test]
fn rename_cancel_and_stale_dialog_actions_preserve_active_owner_and_pending_intent() {
    let mut app = model_app();
    let tab = app.model.workspace().order()[0];
    let active = app.active_session();
    let pending = app
        .model
        .workspace()
        .tab(tab)
        .unwrap()
        .grid()
        .iter()
        .next()
        .unwrap()
        .1
        .session;
    app.focus_on_attach.insert(pending);
    let owners: Vec<_> = app
        .model
        .sessions()
        .iter()
        .map(|(id, e)| (id, e.reservation(), e.lifecycle()))
        .collect();
    drop(app.update(Message::Control(Control::TabAction(
        TabAction::BeginRename(tab),
    ))));
    let token = app.tab_dialog_token().unwrap();
    drop(app.update(Message::TabName(
        token,
        format!("  My project\n\u{202e}{}", "🦀".repeat(400)),
    )));
    drop(app.update(Message::Control(Control::TabDialog(
        token,
        DialogAction::Save,
    ))));
    let name = app
        .model
        .workspace()
        .tab(tab)
        .unwrap()
        .manual_name()
        .unwrap()
        .as_str()
        .to_owned();
    // The editable 256-scalar draft includes its two leading spaces; save
    // trims those spaces rather than inventing two replacement characters.
    assert_eq!(name.chars().count(), TITLE_LIMIT - 2);
    assert!(name.starts_with("My project"));
    assert_eq!(app.active_session(), active);
    assert!(app.focus_on_attach.contains(&pending));
    drop(app.update(Message::Control(Control::TabAction(
        TabAction::BeginRename(tab),
    ))));
    let next = app.tab_dialog_token().unwrap();
    assert_ne!(next, token);
    drop(app.update(Message::TabName(token, "stale".into())));
    drop(app.update(Message::Control(Control::TabDialog(
        token,
        DialogAction::Save,
    ))));
    drop(app.update(Message::Control(Control::TabDialog(
        token,
        DialogAction::Cancel,
    ))));
    drop(app.update(Message::Control(Control::NewTab)));
    drop(app.update(Message::Control(Control::ActivateTab(tab))));
    assert_eq!(app.tab_dialog_token(), Some(next));
    drop(app.update(Message::TabName(next, "discard me".into())));
    drop(app.update(Message::Control(Control::TabDialog(
        next,
        DialogAction::Cancel,
    ))));
    assert_eq!(
        app.model
            .workspace()
            .tab(tab)
            .unwrap()
            .manual_name()
            .unwrap()
            .as_str(),
        name
    );
    assert_eq!(app.active_session(), active);
    assert!(app.focus_on_attach.contains(&pending));
    assert_eq!(
        app.model
            .sessions()
            .iter()
            .map(|(id, e)| (id, e.reservation(), e.lifecycle()))
            .collect::<Vec<_>>(),
        owners
    );
}

#[test]
fn all_tab_selection_consumers_follow_reordered_current_positions_and_stale_targets_are_harmless() {
    let mut app = model_app();
    let tabs = app.model.workspace().order().to_vec();
    drop(
        app.update(Message::Control(Control::TabAction(TabAction::Move {
            tab: tabs[11],
            anchor: tabs[0],
            placement: Placement::Before,
        }))),
    );
    for index in 0..12 {
        drop(
            app.update(Message::Control(Control::TabAction(TabAction::Select(
                Selection::Index(index),
            )))),
        );
        assert_eq!(
            app.model.workspace().active(),
            Some(app.model.workspace().order()[index])
        );
    }
    drop(app.update(Message::Control(Control::Command(
        commands::Command::NextTab,
    ))));
    assert_eq!(app.model.workspace().active(), Some(tabs[11]));
    drop(app.update(Message::Control(Control::Command(
        commands::Command::PreviousTab,
    ))));
    assert_eq!(app.model.workspace().active(), Some(tabs[10]));
    let before = app.active_session();
    app.model.close_tab(tabs[1]).unwrap();
    drop(app.update(Message::Control(Control::TabAction(
        TabAction::BeginRename(tabs[1]),
    ))));
    assert!(app.tab_dialog_token().is_none());
    assert!(app.notice.is_some());
    assert_eq!(app.active_session(), before);
}

#[test]
fn app_blur_without_widget_events_retires_editor_receipts_and_preserves_the_draft() {
    let mut app = model_app();
    let tab = app.model.workspace().active().unwrap();
    drop(app.update(Message::Control(Control::TabAction(
        TabAction::BeginRename(tab),
    ))));
    let token = app.tab_dialog_token().unwrap();
    let input = app.tab_name_widget_id().unwrap();
    drop(app.update(Message::TabName(token, " Draft before blur ".into())));
    drop(app.update(Message::Control(Control::TabDialog(
        token,
        DialogAction::Composing(true),
    ))));
    drop(app.update(Message::WindowFocused(false)));
    let next = app.tab_dialog_token().unwrap();
    assert_ne!(next, token);
    assert_ne!(app.tab_name_widget_id().unwrap(), input);
    drop(app.update(Message::WindowFocused(true)));
    drop(app.update(Message::TabName(token, "stale commit".into())));
    drop(app.update(Message::Control(Control::TabDialog(
        token,
        DialogAction::Save,
    ))));
    assert_eq!(app.tab_dialog_token(), Some(next));
    drop(app.update(Message::Control(Control::TabDialog(
        next,
        DialogAction::Save,
    ))));
    assert_eq!(
        app.model
            .workspace()
            .tab(tab)
            .unwrap()
            .manual_name()
            .unwrap()
            .as_str(),
        "Draft before blur"
    );
}
