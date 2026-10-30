//! Public widget encoder; dimensions remain owned by the real native host.
use iced::advanced::widget::{Id, Operation, Tree, operation};
use iced::advanced::{Layout, Shell, clipboard, layout, mouse};
use iced::keyboard::{self, Modifiers};
use iced_term::{Terminal, TerminalView};
use pike::app::{App, Control, Message};
use pike::identity::SessionId;

pub fn click_new_tab(app: &App, renderer: &iced::Renderer) -> Message {
    let mut view = app.view();
    let mut tree = Tree::new(view.as_widget());
    let size = iced::Size::new(1100.0, 720.0);
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &layout::Limits::new(size, size));
    let root = Layout::new(&node);
    fn centers(layout: Layout<'_>, found: &mut Vec<iced::Point>) {
        found.push(layout.bounds().center());
        for child in layout.children() {
            centers(child, found);
        }
    }
    let mut points = Vec::new();
    centers(root, &mut points);
    for position in points {
        let mut messages = Vec::new();
        for event in [
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            view.as_widget_mut().update(
                &mut tree,
                &iced::Event::Mouse(event),
                root,
                mouse::Cursor::Available(position),
                renderer,
                &mut clipboard::Null,
                &mut Shell::new(&mut messages),
                &iced::Rectangle::with_size(size),
            );
        }
        if let Some(message) = messages
            .into_iter()
            .find(|m| matches!(m, Message::Control(Control::NewTab)))
        {
            return message;
        }
    }
    panic!("actual App view has no clickable New tab control");
}

/// Walk the actual App widget tree. An absent terminal has no widget to build
/// canvas geometry; this does not inspect or change any terminal contents.
pub fn visible_terminals(app: &App, renderer: &iced::Renderer) -> Vec<SessionId> {
    struct Present {
        ids: Vec<(SessionId, Id)>,
        found: Vec<SessionId>,
    }
    impl Operation<()> for Present {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<()>)) {
            operate(self);
        }
        fn focusable(
            &mut self,
            id: Option<&Id>,
            _bounds: iced::Rectangle,
            _state: &mut dyn operation::Focusable,
        ) {
            if let Some((session, _)) = self.ids.iter().find(|(_, widget)| id == Some(widget)) {
                self.found.push(*session);
            }
        }
    }
    let mut present = Present {
        ids: app
            .model()
            .sessions()
            .iter()
            .filter_map(|(id, _)| app.terminal(id).map(|t| (id, t.widget_id().clone())))
            .collect(),
        found: Vec::new(),
    };
    let mut view = app.view();
    let mut tree = Tree::new(view.as_widget());
    let size = iced::Size::new(1100.0, 720.0);
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &layout::Limits::new(size, size));
    view.as_widget_mut()
        .operate(&mut tree, Layout::new(&node), renderer, &mut present);
    present.found
}

pub fn encode(terminal: &Terminal, renderer: &iced::Renderer, text: &str) -> Vec<iced_term::Event> {
    let mut view = TerminalView::show(terminal);
    let mut tree = Tree::new(view.as_widget());
    let size = iced::Size::new(400.0, 240.0);
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &layout::Limits::new(size, size));
    view.as_widget_mut().operate(
        &mut tree,
        Layout::new(&node),
        renderer,
        &mut operation::focusable::focus::<()>(terminal.widget_id().clone()),
    );
    let mut messages = vec![];
    for character in text.chars().map(Some).chain([None]) {
        let key = character
            .map(|c| keyboard::Key::Character(c.to_string().into()))
            .unwrap_or(keyboard::Key::Named(keyboard::key::Named::Enter));
        let event = iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
            text: character.map(|c| c.to_string().into()),
            repeat: false,
        });
        view.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut clipboard::Null,
            &mut Shell::new(&mut messages),
            &iced::Rectangle::with_size(size),
        );
    }
    messages.retain(|event| {
        !matches!(
            event,
            iced_term::Event::BackendCall(_, iced_term::BackendCommand::Resize(..))
        )
    });
    messages
}
