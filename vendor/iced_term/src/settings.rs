use std::{collections::HashMap, path::PathBuf};

use crate::ColorPalette;
use iced::Font;

#[cfg(target_os = "windows")]
const DEFAULT_SHELL: &str = "";

#[cfg(not(target_os = "windows"))]
const DEFAULT_SHELL: &str = "/bin/bash";

#[derive(Default, Clone)]
pub struct Settings {
    pub font: FontSettings,
    pub theme: ThemeSettings,
    pub backend: BackendSettings,
}

#[derive(Clone)]
pub struct BackendSettings {
    /// Unquoted executable path/name. Empty on Windows selects PowerShell 7,
    /// Windows PowerShell, then cmd. WSL requires an explicit program.
    pub program: String,
    /// Individual unquoted arguments, unless `escape_args` is explicitly false.
    pub args: Vec<String>,
    /// Windows CRT argument encoding by default. Set false only for an explicit
    /// shell command line (for example cmd /D /S /C), whose syntax you own.
    #[cfg(windows)]
    pub escape_args: bool,
    pub env: HashMap<String, String>,
    pub working_directory: Option<PathBuf>,
}

impl Default for BackendSettings {
    fn default() -> Self {
        Self {
            program: DEFAULT_SHELL.to_string(),
            args: vec![],
            #[cfg(windows)]
            escape_args: true,
            env: HashMap::new(),
            working_directory: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FontSettings {
    pub size: f32,
    pub scale_factor: f32,
    pub font_type: Font,
}

impl Default for FontSettings {
    fn default() -> Self {
        Self {
            size: 14.0,
            scale_factor: 1.3,
            font_type: Font::MONOSPACE,
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct ThemeSettings {
    pub color_pallete: Box<ColorPalette>,
}

impl ThemeSettings {
    pub fn new(color_pallete: Box<ColorPalette>) -> Self {
        Self { color_pallete }
    }
}

// Launch settings may contain credentials in any field, including paths.
impl std::fmt::Debug for BackendSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackendSettings").finish_non_exhaustive()
    }
}
