use super::*;
use iced::futures::{FutureExt, StreamExt};

#[test]
fn delayed_editor_focus_cannot_steal_a_newer_rename_selection() {
    struct Focus(bool);
    impl operation::Focusable for Focus {
        fn is_focused(&self) -> bool {
            self.0
        }
        fn focus(&mut self) {
            self.0 = true;
        }
        fn unfocus(&mut self) {
            self.0 = false;
        }
    }
    let mut app = App::default();
    app.window_focus.set_focused(true);
    let (tab, _) = app.model.create_tab().unwrap();
    let task = app.begin_tab_rename(tab.tab);
    let mut stream = iced_runtime::task::into_stream(task).unwrap();
    let iced_runtime::Action::Widget(mut held) = stream.next().now_or_never().flatten().unwrap()
    else {
        panic!("expected editor focus operation");
    };
    let token = app.tab_dialog_token().unwrap();
    drop(app.tab_dialog_action(token, DialogAction::Next));
    let current = app.tab_name_widget_id().unwrap();
    let mut state = Focus(false);
    held.traverse(&mut |effect| effect.focusable(Some(&current), Rectangle::default(), &mut state));
    assert!(!state.0, "queued editor focus stole Cancel's selection");
    assert_eq!(app.tab_dialog_token(), Some(token));
}
