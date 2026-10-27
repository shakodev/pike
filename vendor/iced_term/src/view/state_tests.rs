use super::*;
use crate::settings::BackendSettings;
use crate::PreparedTerminal;
use std::time::Duration;

struct Session(Terminal);

impl Session {
    fn new(id: u64) -> Self {
        #[cfg(windows)]
        let (program, args) =
            ("cmd.exe", vec!["/D", "/Q", "/C", "set /p PIKE_WAIT="]);
        #[cfg(not(windows))]
        let (program, args) = ("/bin/sh", vec!["-c", "read pike_wait"]);
        Self(
            PreparedTerminal::new(
                id,
                BackendSettings {
                    program: program.into(),
                    args: args.into_iter().map(str::to_owned).collect(),
                    #[cfg(windows)]
                    escape_args: false,
                    ..Default::default()
                },
            )
            .unwrap()
            .attach(Default::default(), Default::default()),
        )
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.0.request_close();
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                tokio::time::timeout(
                    Duration::from_secs(5),
                    self.0.shutdown_handle().wait(),
                )
                .await
                .unwrap();
            });
    }
}

fn tree(view: &TerminalView<'_>) -> Tree {
    Tree::new(view as &dyn Widget<Event, Theme, iced::Renderer>)
}

fn resize(
    view: &mut TerminalView<'_>,
    tree: &mut Tree,
    bounds: Rectangle,
) -> Vec<Event> {
    let node =
        iced_core::layout::Node::new(bounds.size()).move_to(bounds.position());
    let mut events = Vec::new();
    view.handle_resize(
        tree.state.downcast_mut(),
        iced_core::Layout::new(&node),
        &mut iced_core::Shell::new(&mut events),
    );
    events
}

#[test]
fn same_sized_tab_replacements_reset_all_input_state_and_receive_their_own_layout(
) {
    let sessions = [Session::new(13001), Session::new(13002)];
    let bounds =
        Rectangle::new(Point::new(8.25, 31.5), Size::new(400.0, 240.0));
    let mut tree = tree(&TerminalView {
        term: &sessions[0].0,
    });
    for index in 0..32 {
        let terminal = &sessions[index % 2].0;
        let mut view = TerminalView { term: terminal };
        view.diff(&mut tree);
        let state = tree.state.downcast_ref::<TerminalViewState>();
        assert_eq!(state.terminal_id, terminal.id);
        assert!(!state.is_focused());
        assert!(!state.is_dragged);
        assert!(state.last_click.is_none());
        assert_eq!(state.scroll_pixels, 0.0);
        assert_eq!(state.keyboard_modifiers, Modifiers::empty());
        assert_eq!(state.mouse_position_on_grid, TerminalGridPoint::default());
        assert!(matches!(
            resize(&mut view, &mut tree, bounds).as_slice(),
            [Event::BackendCall(id, Command::Resize(Some(size), None))]
                if *id == terminal.id && *size == bounds.size()
        ));
        assert!(resize(&mut view, &mut tree, bounds).is_empty());
        let state = tree.state.downcast_mut::<TerminalViewState>();
        state.focus();
        state.is_dragged = true;
        state.last_click =
            Some(Click::new(Point::ORIGIN, mouse::Button::Left, None));
        state.scroll_pixels = 0.75;
        state.keyboard_modifiers = Modifiers::CTRL;
        state.mouse_position_on_grid = TerminalGridPoint::new(
            alacritty_terminal::index::Line(2),
            alacritty_terminal::index::Column(3),
        );
    }
}

#[test]
fn same_session_reconciliation_preserves_focus_and_move_resets_only_pointer_state(
) {
    let session = Session::new(13003);
    let mut view = TerminalView { term: &session.0 };
    let mut tree = tree(&view);
    let bounds =
        Rectangle::new(Point::new(4.25, 32.5), Size::new(400.0, 240.0));
    assert_eq!(resize(&mut view, &mut tree, bounds).len(), 1);
    let state = tree.state.downcast_mut::<TerminalViewState>();
    state.focus();
    state.is_dragged = true;
    state.scroll_pixels = 0.5;
    view.diff(&mut tree);
    assert!(resize(&mut view, &mut tree, bounds).is_empty());
    let state = tree.state.downcast_ref::<TerminalViewState>();
    assert!(state.is_focused() && state.is_dragged);
    let before = session.0.work_stats();
    let moved = Rectangle::new(Point::new(300.75, 90.25), bounds.size());
    assert!(resize(&mut view, &mut tree, moved).is_empty());
    let state = tree.state.downcast_ref::<TerminalViewState>();
    assert!(state.is_focused());
    assert!(!state.is_dragged);
    assert_eq!(state.scroll_pixels, 0.0);
    assert_eq!(session.0.work_stats(), before);
    assert!(session.0.shutdown_handle().completion().is_none());
}

#[test]
fn tab_replacement_does_not_discard_session_selection_or_viewport() {
    let mut first = Session::new(13004);
    let second = Session::new(13005);
    first
        .0
        .handle(crate::Command::ProxyToBackend(Command::SelectStart(
            SelectionType::Simple,
            (0.0, 0.0),
        )));
    first
        .0
        .handle(crate::Command::ProxyToBackend(Command::SelectUpdate((
            80.0, 0.0,
        ))));
    let selection = first.0.backend.renderable_content().selectable_range;
    assert!(selection.is_some());
    let before = first.0.snapshot_stats();
    let mut tree = tree(&TerminalView { term: &first.0 });
    TerminalView { term: &second.0 }.diff(&mut tree);
    TerminalView { term: &first.0 }.diff(&mut tree);
    assert_eq!(
        first.0.backend.renderable_content().selectable_range,
        selection
    );
    let after = first.0.snapshot_stats();
    assert_eq!(after.snapshots, before.snapshots);
    assert_eq!(after.cells_copied, before.cells_copied);
    assert_eq!(after.history_lines, before.history_lines);
    assert!(first.0.shutdown_handle().completion().is_none());
}

#[test]
fn focus_loss_cannot_leave_a_drag_or_modifiers_for_later_reentry() {
    let mut state = TerminalViewState::new(13006);
    state.focus();
    state.is_dragged = true;
    state.keyboard_modifiers = Modifiers::COMMAND;
    state.scroll_pixels = 0.5;
    state.unfocus();
    state.focus();
    assert!(!state.is_dragged);
    assert_eq!(state.keyboard_modifiers, Modifiers::empty());
    assert_eq!(state.scroll_pixels, 0.0);
}
