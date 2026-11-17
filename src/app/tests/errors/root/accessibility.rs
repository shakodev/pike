//! Synthetic keyboard events through the actual App view and pinned Iced
//! widgets, with real local PTYs. These are not physical desktop/Narrator tests.
use super::*;
use crate::app::commands::{Command, FontChange, HelpAction};
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::pane_grid;

fn host() -> Host {
    let executor = SharedExecutor::new().unwrap();
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .expect("local graphics device");
    let (app, _, _) = pair(&executor);
    let tree = Tree::new(app.view().as_widget());
    let mut host = Host {
        app,
        executor,
        renderer,
        tree,
        size: Size::new(960.0, 700.0),
        clipboard: None,
        clipboard_reads: 0,
        clipboard_writes: Vec::new(),
    };
    host.settle();
    host.task(host.app.focus_active());
    host
}

impl Host {
    fn assert_in_viewport(&self, bounds: Rectangle) {
        // Layout arithmetic is f32; allow representation error below 0.001
        // DIP, without accepting a clipped physical pixel at any tested scale.
        assert!(
            bounds.x >= -0.001
                && bounds.y >= -0.001
                && bounds.x + bounds.width <= self.size.width + 0.001
                && bounds.y + bounds.height <= self.size.height + 0.001,
            "control is outside the viewport: {bounds:?}"
        );
    }
    fn assert_dialog_selection_visible(&mut self) {
        let id = self.app.tab_dialog_focus_id().expect("dialog selection");
        let bounds = self.bounds(vec![id]).pop().expect("selected control").1;
        self.assert_in_viewport(bounds);
    }
    fn key(&mut self, key: impl Into<Key>, modifiers: Modifiers, text: Option<&str>) {
        let key = key.into();
        self.dispatch(
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)),
            Point::ORIGIN,
        );
        let pressed = iced::keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key.clone(),
            physical_key: iced::keyboard::key::Physical::Unidentified(
                iced::keyboard::key::NativeCode::Unidentified,
            ),
            location: iced::keyboard::Location::Standard,
            modifiers,
            text: text.map(Into::into),
            repeat: false,
        };
        // Close dialogs use the production keyboard subscription. Their view
        // has no terminal child; deliver the same ignored runtime interaction.
        let close = into_recipes(self.app.close_subscription()).pop();
        self.dispatch(iced::Event::Keyboard(pressed.clone()), Point::ORIGIN);
        if let Some(recipe) = close {
            let mut events = recipe.stream(
                stream::iter([iced::advanced::subscription::Event::Interaction {
                    window: iced::window::Id::unique(),
                    event: iced::Event::Keyboard(pressed),
                    status: iced::event::Status::Ignored,
                }])
                .boxed(),
            );
            if let Some(message) = self.executor.block_on(events.next()) {
                self.apply(message);
            }
        }
        self.dispatch(
            iced::Event::Keyboard(iced::keyboard::Event::KeyReleased {
                modified_key: key.clone(),
                physical_key: iced::keyboard::key::Physical::Unidentified(
                    iced::keyboard::key::NativeCode::Unidentified,
                ),
                key,
                location: iced::keyboard::Location::Standard,
                modifiers,
            }),
            Point::ORIGIN,
        );
        self.dispatch(
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(Modifiers::empty())),
            Point::ORIGIN,
        );
    }

    fn choose(&mut self, action: HelpAction) {
        self.key(Named::F1, Modifiers::empty(), None);
        assert!(self.app.command_state.help.is_some());
        let index = self
            .app
            .command_state
            .help_actions
            .iter()
            .position(|a| *a == action)
            .unwrap_or_else(|| panic!("menu does not contain {action:?}"));
        self.key(Named::Home, Modifiers::empty(), None);
        for _ in 0..index / 8 {
            self.key(Named::PageDown, Modifiers::empty(), None);
        }
        for _ in 0..index % 8 {
            self.key(Named::ArrowDown, Modifiers::empty(), None);
        }
        assert_eq!(self.app.command_state.help_selected, index);
        self.key(Named::Enter, Modifiers::empty(), None);
        self.settle();
    }

    fn finish(mut self) {
        // Complete controlled reads before teardown; no partial-line Unix
        // cancellation timing is included in this keyboard acceptance fixture.
        let handles: Vec<_> = self
            .app
            .model
            .sessions()
            .iter()
            .filter_map(|(id, _)| self.app.terminal(id).map(|t| (id, t.shutdown_handle())))
            .collect();
        for (id, _) in &handles {
            self.apply(Message::Terminal((
                *id,
                Event::BackendCall(id.get(), B::Write(b"\n".to_vec())),
            )));
        }
        shutdown(&mut self.app, &self.executor);
        for (_, handle) in handles {
            self.executor.block_on(async {
                tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap();
            });
        }
    }

    fn raster(&mut self, name: &str, scale: f32) {
        let Some(output) = std::env::var_os("PIKE_ACCESSIBILITY_ARTIFACTS") else {
            return;
        };
        use iced::advanced::{Renderer, renderer::Style};
        use std::io::Write;
        self.settle();
        let selected = self.app.command_state.help.is_some().then(|| {
            let id = self.app.command_state.help_rows[self.app.command_state.help_selected].clone();
            self.bounds(vec![id]).pop().expect("selected menu row").1
        });
        let mut view = self.app.view();
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.size, self.size),
        );
        let viewport = Rectangle::with_size(self.size);
        // Iced button/input visual status is set during the same view's update
        // before drawing; a freshly constructed view otherwise paints Disabled.
        view.as_widget_mut().update(
            &mut self.tree,
            &iced::Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &self.renderer,
            &mut clipboard::Null,
            &mut Shell::new(&mut Vec::new()),
            &viewport,
        );
        self.renderer.reset(viewport);
        view.as_widget().draw(
            &self.tree,
            &mut self.renderer,
            &self.app.theme(),
            &Style::default(),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &viewport,
        );
        let size = Size::new(
            (self.size.width * scale) as u32,
            (self.size.height * scale) as u32,
        );
        let pixels = Headless::screenshot(&mut self.renderer, size, scale, iced::Color::BLACK);
        let [r, g, b, _] = crate::ui::style::palette(&self.app.theme())
            .accent
            .into_rgba8();
        assert!(
            pixels
                .chunks_exact(4)
                .filter(|p| p[0].abs_diff(r) < 3 && p[1].abs_diff(g) < 3 && p[2].abs_diff(b) < 3)
                .count()
                >= 20,
            "selected control has no visible accent outline in {name}"
        );
        let output = std::path::Path::new(&output);
        std::fs::create_dir_all(output).unwrap();
        let mut file = std::io::BufWriter::new(
            std::fs::File::create(output.join(format!("{name}.ppm"))).unwrap(),
        );
        write!(file, "P6\n{} {}\n255\n", size.width, size.height).unwrap();
        for pixel in pixels.chunks_exact(4) {
            file.write_all(&pixel[..3]).unwrap();
        }
        file.flush().unwrap();
        if let Some(bounds) = selected {
            let [r, g, b, _] = crate::ui::style::palette(&self.app.theme())
                .text
                .into_rgba8();
            let ink = pixels
                .chunks_exact(4)
                .enumerate()
                .filter(|(index, p)| {
                    let x = ((*index % size.width as usize) as f32 + 0.5) / scale;
                    let y = ((*index / size.width as usize) as f32 + 0.5) / scale;
                    x >= bounds.x + 10.0
                        && x < bounds.x + bounds.width - 10.0
                        && y >= bounds.y + 5.0
                        && y < bounds.y + bounds.height - 5.0
                        && p[0].abs_diff(r) < 12
                        && p[1].abs_diff(g) < 12
                        && p[2].abs_diff(b) < 12
                })
                .count();
            assert!(
                ink >= 20,
                "selected menu text is missing in {name}: {ink} ink pixels in {bounds:?}"
            );
        }
    }
}

#[test]
#[ignore = "requires a local graphics device; actual App keyboard journey with real PTYs"]
fn accessibility_keyboard_journey_preserves_sessions_and_restores_input() {
    let mut host = host();
    let original = host.app.active_session().unwrap();
    let widget = host.app.terminal(original).unwrap().widget_id().clone();
    host.choose(HelpAction::Execute(Command::NewTab));
    assert_eq!(host.app.model.sessions().len(), 3);
    host.choose(HelpAction::Execute(Command::Split(
        crate::app::Direction::Right,
    )));
    assert_eq!(host.app.model.sessions().len(), 4);
    let split = host.app.active_session().unwrap();
    let split_widget = host.app.terminal(split).unwrap().widget_id().clone();
    let tab = host.app.active_target().unwrap().tab;
    let layout = |host: &Host| {
        format!(
            "{:?}",
            host.app.model.workspace().tab(tab).unwrap().grid().layout()
        )
    };
    let before = layout(&host);
    host.choose(HelpAction::Execute(Command::Resize(
        crate::app::ResizeStep::Left,
    )));
    assert_ne!(layout(&host), before);
    host.choose(HelpAction::Execute(Command::MovePane(
        pane_grid::Direction::Left,
    )));
    assert_eq!(host.app.active_session(), Some(split));
    host.choose(HelpAction::Execute(Command::ToggleZoom));
    assert!(
        host.app
            .model
            .workspace()
            .tab(tab)
            .unwrap()
            .grid()
            .maximized()
            .is_some()
    );
    host.choose(HelpAction::Execute(Command::ToggleZoom));
    host.choose(HelpAction::Execute(Command::Font(FontChange::Larger)));
    for _ in 0..35 {
        host.key(
            Key::Character("+".into()),
            Modifiers::CTRL | Modifiers::SHIFT,
            None,
        );
    }
    assert_eq!(host.app.command_state.fonts[&split].size, 28.0);
    for _ in 0..35 {
        host.key(
            Key::Character("-".into()),
            Modifiers::CTRL | Modifiers::SHIFT,
            None,
        );
    }
    assert_eq!(host.app.command_state.fonts[&split].size, 10.0);
    host.choose(HelpAction::Execute(Command::Font(FontChange::Reset)));

    host.choose(HelpAction::Execute(Command::RenameTab));
    host.key(Key::Character("a".into()), Modifiers::COMMAND, None);
    for character in "Keyboard35".chars() {
        let text = character.to_string();
        host.key(
            Key::Character(text.clone().into()),
            Modifiers::empty(),
            Some(&text),
        );
    }
    host.key(Named::Tab, Modifiers::empty(), None); // Cancel
    host.key(Named::Tab, Modifiers::SHIFT, None); // Name
    host.key(Named::Tab, Modifiers::empty(), None); // Cancel
    host.key(Named::Tab, Modifiers::empty(), None); // Save
    host.key(Named::Enter, Modifiers::empty(), None);
    assert_eq!(host.app.tab_title(tab).unwrap().as_str(), "Keyboard35");
    assert!(host.app.tab_dialog_token().is_none());

    // A menu owns Tab/text; returning restores the same widget's input.
    let before = accepted(&host.app, split);
    host.key(Named::F1, Modifiers::empty(), None);
    host.type_one();
    host.key(Named::Tab, Modifiers::SHIFT, None);
    assert_eq!(
        host.app.command_state.help_selected,
        host.app.command_state.help_actions.len() - 1
    );
    assert_eq!(accepted(&host.app, split), before);
    host.key(Named::Escape, Modifiers::empty(), None);
    host.type_one();
    assert_eq!(accepted(&host.app, split), before + 1);
    host.choose(HelpAction::Execute(Command::Paste));
    let paste = host.app.latest_error(Owner::Session(split)).unwrap();
    host.clipboard = Some("SYNTHETIC_PASTE35".into());
    let before = accepted(&host.app, split);
    host.choose(HelpAction::Recovery(paste.id, Action::Retry));
    assert_eq!(
        accepted(&host.app, split),
        before + "SYNTHETIC_PASTE35".len()
    );
    // Authoritative selection and the same menu route handle copy.
    host.apply(Message::Terminal((
        split,
        Event::BackendCall(
            split.get(),
            B::SelectStart(iced_term::SelectionType::Lines, (1.0, 1.0)),
        ),
    )));
    host.choose(HelpAction::Execute(Command::Copy));
    assert_eq!(host.clipboard_writes.len(), 1);
    assert!(
        host.app.errors.get(paste.id).is_none(),
        "successful paste must resolve its notice"
    );
    host.app.report_error(
        Owner::Application,
        Error::Config(crate::settings::Error::Location),
    );
    let dismiss = host.app.latest_error(Owner::Application).unwrap().id;
    host.choose(HelpAction::Recovery(dismiss, Action::Dismiss));
    assert!(host.app.errors.get(dismiss).is_none());

    host.choose(HelpAction::Execute(Command::ListTabs));
    host.key(Named::Home, Modifiers::empty(), None);
    host.key(Named::Delete, Modifiers::empty(), None);
    assert!(host.app.close_confirmation().is_some());
    host.key(Named::Escape, Modifiers::empty(), None);
    assert!(host.app.close_confirmation().is_none());
    assert_eq!(host.app.active_session(), Some(split));
    assert_eq!(host.app.terminal(original).unwrap().widget_id(), &widget);
    assert_eq!(host.app.terminal(split).unwrap().widget_id(), &split_widget);
    assert_eq!(host.app.model.sessions().len(), 4);
    host.apply(Message::Terminal((
        split,
        Event::BackendCall(split.get(), B::Write(b"\n".to_vec())),
    )));
    host.choose(HelpAction::Execute(Command::ClosePane));
    assert!(host.app.close_confirmation().is_some());
    host.key(Named::Tab, Modifiers::empty(), None);
    host.key(Named::Enter, Modifiers::empty(), None);
    assert!(matches!(
        host.app.model.sessions().get(split).unwrap().lifecycle(),
        Lifecycle::Closing { .. }
    ));
    assert!(host.app.terminal(original).is_some());
    host.finish();
}

#[test]
#[ignore = "requires a local graphics device; high contrast and narrow-window App rasters"]
fn accessibility_high_contrast_dialogs_keep_selected_controls_visible() {
    let mut host = host();
    let original: Vec<_> = host.app.model.sessions().iter().map(|(id, _)| id).collect();
    for name in ["high-contrast-dark", "high-contrast-light"] {
        host.app.settings.pending = true;
        host.apply(Message::SettingsLoaded(crate::settings::Settings::parse(
            &format!("version = 1\ntheme = '{name}'"),
        )));
        for scale in [1.0, 1.25, 1.5, 2.0] {
            host.size = Size::new(640.0, 480.0);
            host.key(Named::F1, Modifiers::empty(), None);
            host.key(Named::Tab, Modifiers::empty(), None);
            host.raster(&format!("{name}-menu-{scale}"), scale);
            host.key(Named::Escape, Modifiers::empty(), None);
        }
        host.size = Size::new(320.0, 240.0);
        host.choose(HelpAction::Execute(Command::RenameTab));
        host.key(Named::Tab, Modifiers::SHIFT, None); // Save
        host.assert_dialog_selection_visible();
        host.raster(&format!("{name}-rename-narrow"), 1.5);
        host.key(Named::Escape, Modifiers::empty(), None);
        host.choose(HelpAction::Execute(Command::ListTabs));
        host.key(Named::End, Modifiers::empty(), None);
        host.assert_dialog_selection_visible();
        host.raster(&format!("{name}-overflow-narrow"), 1.5);
        host.key(Named::Delete, Modifiers::empty(), None);
        assert!(host.app.close_confirmation().is_some());
        host.key(Named::Tab, Modifiers::empty(), None);
        let selected = host
            .bounds(vec![Id::from("close-confirm")])
            .pop()
            .unwrap()
            .1;
        host.assert_in_viewport(selected);
        host.raster(&format!("{name}-close-narrow"), 1.5);
        host.key(Named::Escape, Modifiers::empty(), None);
        assert_eq!(
            host.app
                .model
                .sessions()
                .iter()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            original
        );
    }
    host.finish();
}
