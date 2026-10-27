use alacritty_terminal::term::TermMode;
use iced_core::{
    keyboard::{key::Named, Key, Modifiers},
    mouse::Button,
};

#[derive(Clone, Hash, Debug, PartialEq, Eq)]
pub enum BindingAction {
    Copy,
    Paste,
    Char(char),
    Esc(String),
    /// Reserved; external link opening is disabled in Pike's MVP.
    LinkOpen,
    /// Consume the key without terminal bytes or text fallback.
    Ignore,
}

#[derive(Clone, Hash, Debug, PartialEq, Eq)]
pub enum InputKind {
    Char(String),
    KeyCode(Named),
    Mouse(Button),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding<T> {
    pub target: T,
    pub modifiers: Modifiers,
    pub terminal_mode_include: TermMode,
    pub terminal_mode_exclude: TermMode,
}

pub type KeyboardBinding = Binding<InputKind>;
pub type MouseBinding = Binding<InputKind>;

#[macro_export]
macro_rules! generate_bindings {
    (
        $binding_type:ident;
        $(
            $input_kind:tt$(::$button:ident)?
            $(,$input_modifiers:expr)*
            $(,+$terminal_mode_include:expr)*
            $(,~$terminal_mode_exclude:expr)*
            ;$action:expr
        );*
        $(;)*
    ) => {{
        macro_rules! input_kind_match {
            (KeyboardBinding, $key:ident) => {{
                InputKind::KeyCode(Named::$key)
            }};
            (MouseBinding, $key:ident) => {{
                InputKind::Mouse(Button::$key)
            }};
            (KeyboardBinding, $expr:expr) => {{
                InputKind::Char($expr.to_string())
            }};
        }

        let mut v = Vec::new();

        $(
            let mut _input_modifiers = Modifiers::empty();
            $(_input_modifiers = $input_modifiers;)*
            let mut _terminal_mode_include = TermMode::empty();
            $(_terminal_mode_include.insert($terminal_mode_include);)*
            let mut _terminal_mode_exclude = TermMode::empty();
            $(_terminal_mode_exclude.insert($terminal_mode_exclude);)*

            let binding = $binding_type {
                target: input_kind_match!($binding_type, $input_kind),
                modifiers: _input_modifiers,
                terminal_mode_include: _terminal_mode_include,
                terminal_mode_exclude: _terminal_mode_exclude,
            };

            v.push((binding, $action.into()));
        )*

        v
    }};
}

#[derive(Clone, Debug)]
pub(crate) struct BindingsLayout {
    layout: Vec<(Binding<InputKind>, BindingAction)>,
}

impl Default for BindingsLayout {
    fn default() -> Self {
        BindingsLayout::new()
    }
}

impl BindingsLayout {
    pub(crate) fn new() -> Self {
        Self {
            layout: platform_keyboard_bindings(),
        }
    }

    pub(crate) fn add_bindings(
        &mut self,
        bindings: Vec<(Binding<InputKind>, BindingAction)>,
    ) {
        for (binding, action) in bindings {
            if let Some(position) = self
                .layout
                .iter()
                .position(|(existing, _)| existing == &binding)
            {
                self.layout.remove(position);
            }
            // Most recently configured matching override wins, including a
            // mode-specific override of a broader platform shortcut.
            self.layout.push((binding, action));
        }
    }

    fn find_action(
        &self,
        target: impl Fn(&InputKind) -> bool,
        modifiers: Modifiers,
        terminal_mode: TermMode,
    ) -> Option<&BindingAction> {
        self.layout.iter().rev().find_map(|(binding, action)| {
            (target(&binding.target)
                && binding.modifiers == modifiers
                && terminal_mode.contains(binding.terminal_mode_include)
                && !terminal_mode.intersects(binding.terminal_mode_exclude))
            .then_some(action)
        })
    }

    pub(crate) fn get_action(
        &self,
        input: InputKind,
        modifiers: Modifiers,
        terminal_mode: TermMode,
    ) -> Option<&BindingAction> {
        self.find_action(|target| *target == input, modifiers, terminal_mode)
    }

    pub(crate) fn key_action(
        &self,
        key: &Key,
        modifiers: Modifiers,
        terminal_mode: TermMode,
    ) -> Option<&BindingAction> {
        self.find_action(
            |target| match (target, key) {
                (InputKind::Char(binding), Key::Character(key)) => {
                    binding.eq_ignore_ascii_case(key.as_str())
                },
                (InputKind::KeyCode(binding), Key::Named(key)) => {
                    binding == key
                },
                _ => false,
            },
            modifiers,
            terminal_mode,
        )
    }
}

#[cfg(target_os = "macos")]
fn platform_keyboard_bindings() -> Vec<(Binding<InputKind>, BindingAction)> {
    generate_bindings!(
        KeyboardBinding;
        "c", Modifiers::COMMAND; BindingAction::Copy;
        "v", Modifiers::COMMAND; BindingAction::Paste;
        "h", Modifiers::COMMAND; BindingAction::Char('\x7f');
    )
}

#[cfg(not(target_os = "macos"))]
fn platform_keyboard_bindings() -> Vec<(Binding<InputKind>, BindingAction)> {
    generate_bindings!(
        KeyboardBinding;
        "c", Modifiers::SHIFT | Modifiers::COMMAND; BindingAction::Copy;
        "v", Modifiers::SHIFT | Modifiers::COMMAND; BindingAction::Paste;
    )
}

#[cfg(test)]
mod tests {
    use crate::bindings::MouseBinding;

    use super::{BindingAction, BindingsLayout, InputKind, KeyboardBinding};
    use alacritty_terminal::term::TermMode;
    use iced_core::{
        keyboard::{key::Named, Modifiers},
        mouse::Button,
    };

    #[test]
    fn add_new_custom_keyboard_binding() {
        let mut current_layout = BindingsLayout::default();
        let custom_bindings = generate_bindings!(
            KeyboardBinding;
            "c", Modifiers::SHIFT | Modifiers::ALT; BindingAction::Copy;
        );
        let current_layout_length = current_layout.layout.len();
        let custom_bindings_length = custom_bindings.len();
        current_layout.add_bindings(custom_bindings.clone());
        assert_eq!(
            current_layout.layout.len(),
            current_layout_length + custom_bindings_length
        );
        let found_binding =
            current_layout.layout.iter().find(|(bind, action)| {
                bind == &custom_bindings[0].0 && action == &custom_bindings[0].1
            });
        assert!(found_binding.is_some());
    }

    #[test]
    fn add_many_new_custom_keyboard_bindings() {
        let mut current_layout: BindingsLayout = BindingsLayout::default();
        let custom_bindings = generate_bindings!(
            KeyboardBinding;
            ArrowDown, Modifiers::ALT, +TermMode::SGR_MOUSE; BindingAction::LinkOpen;
            "c",       Modifiers::SHIFT, +TermMode::ALT_SCREEN;             BindingAction::Paste;
            "c",       Modifiers::SHIFT | Modifiers::ALT;                   BindingAction::Copy;
            "w",       Modifiers::ALT;                                      BindingAction::Char('W');
            "q",       Modifiers::SHIFT | Modifiers::CTRL | Modifiers::ALT; BindingAction::Esc("\x1b[1;7C".into());
        );
        let current_layout_length = current_layout.layout.len();
        let custom_bindings_length = custom_bindings.len();
        current_layout.add_bindings(custom_bindings.clone());
        assert_eq!(
            current_layout.layout.len(),
            current_layout_length + custom_bindings_length
        );
        for (custom_bind, custom_action) in custom_bindings {
            let found_binding =
                current_layout.layout.iter().find(|(bind, action)| {
                    bind == &custom_bind && action == &custom_action
                });
            assert!(found_binding.is_some());
        }
    }

    #[test]
    fn add_custom_keyboard_bindings_that_replace_current() {
        let mut current_layout = BindingsLayout::default();
        let custom_bindings = generate_bindings!(
            KeyboardBinding;
            "c", Modifiers::SHIFT, +TermMode::ALT_SCREEN; BindingAction::Paste;
            "a", Modifiers::SHIFT | Modifiers::CTRL;      BindingAction::Char('A');
            "b", Modifiers::SHIFT | Modifiers::CTRL;      BindingAction::Char('B');
            "c", Modifiers::SHIFT | Modifiers::CTRL;      BindingAction::Copy;
        );
        current_layout.add_bindings(custom_bindings.clone());

        for (custom_bind, custom_action) in custom_bindings {
            let found_binding =
                current_layout.layout.iter().find(|(bind, action)| {
                    bind == &custom_bind && action == &custom_action
                });
            assert!(found_binding.is_some());
        }
    }

    #[test]
    fn add_mouse_binding() {
        let mut current_layout = BindingsLayout::default();
        let custom_bindings = generate_bindings!(
            MouseBinding;
            Left,  Modifiers::SHIFT, +TermMode::ALT_SCREEN; BindingAction::Paste;
            Right, Modifiers::SHIFT | Modifiers::CTRL;      BindingAction::Char('A');
        );
        let current_layout_length = current_layout.layout.len();
        current_layout.add_bindings(custom_bindings.clone());
        assert_eq!(current_layout.layout.len(), current_layout_length + 2);
        for (custom_bind, custom_action) in custom_bindings {
            let found_binding =
                current_layout.layout.iter().find(|(bind, action)| {
                    bind == &custom_bind && action == &custom_action
                });
            assert!(found_binding.is_some());
        }
    }

    #[test]
    fn get_action() {
        let current_layout = BindingsLayout::default();
        for (bind, action) in &current_layout.layout {
            let found_action = current_layout.get_action(
                bind.target.clone(),
                bind.modifiers,
                bind.terminal_mode_include,
            );
            assert_eq!(Some(action), found_action);
        }
    }

    #[test]
    fn get_action_with_custom_bindings() {
        let mut current_layout = BindingsLayout::default();
        let custom_bindings = generate_bindings!(
            KeyboardBinding;
            "c", Modifiers::SHIFT, +TermMode::ALT_SCREEN; BindingAction::Paste;
            "a", Modifiers::SHIFT | Modifiers::CTRL;      BindingAction::Char('A');
            "b", Modifiers::SHIFT | Modifiers::CTRL;      BindingAction::Char('B');
            "c", Modifiers::SHIFT | Modifiers::CTRL;      BindingAction::Copy;
        );
        current_layout.add_bindings(custom_bindings.clone());
        for (bind, action) in &current_layout.layout {
            let found_action = current_layout.get_action(
                bind.target.clone(),
                bind.modifiers,
                bind.terminal_mode_include,
            );
            assert_eq!(Some(action), found_action);
        }
    }
}
