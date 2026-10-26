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
    /// Normal-screen scrollback; validated independently of viewport geometry.
    pub history: HistoryLimit,
    /// Unquoted executable path/name. Empty on Windows selects PowerShell 7,
    /// Windows PowerShell, then cmd. WSL requires an explicit program.
    /// Windows drive-relative executable paths (C:tool.exe) are rejected.
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
            history: HistoryLimit::default(),
            program: DEFAULT_SHELL.to_string(),
            args: vec![],
            #[cfg(windows)]
            escape_args: true,
            env: HashMap::new(),
            working_directory: None,
        }
    }
}

/// A line limit, not a byte budget: combining characters and hyperlinks can
/// allocate additional storage within a cell. Zero disables normal history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryLimit(usize);

impl HistoryLimit {
    pub const MAX: usize = 10_000;

    pub fn lines(self) -> usize {
        self.0
    }
}

impl Default for HistoryLimit {
    fn default() -> Self {
        Self(2_000)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidHistoryLimit;

impl std::fmt::Display for InvalidHistoryLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("history must be between 0 and 10000 lines")
    }
}

impl std::error::Error for InvalidHistoryLimit {}

impl TryFrom<usize> for HistoryLimit {
    type Error = InvalidHistoryLimit;

    fn try_from(lines: usize) -> Result<Self, Self::Error> {
        if lines <= Self::MAX {
            Ok(Self(lines))
        } else {
            Err(InvalidHistoryLimit)
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
