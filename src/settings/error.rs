//! Errors contain only fixed field labels, source locations and OS codes.
use std::{fmt, io};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    ProfileId,
    Profiles,
    DefaultProfile,
    Shell,
    Args,
    Cwd,
    Environment,
    Font,
    FontSize,
    History,
    Shortcuts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Read,
    CreateDirectory,
    CreateTemporary,
    Write,
    Flush,
    Replace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Location,
    TooLarge,
    NotFile,
    Utf8,
    Version,
    Parse {
        line: usize,
        column: usize,
    },
    Invalid(Field),
    MissingProfile,
    BrokenTarget,
    Serialize,
    Io {
        operation: Operation,
        kind: io::ErrorKind,
        code: Option<i32>,
    },
}

impl Error {
    pub(super) fn parse(source: &str, error: &toml::de::Error) -> Self {
        let start = error.span().map_or(0, |span| span.start.min(source.len()));
        let prefix = &source.as_bytes()[..start];
        let line = prefix.iter().filter(|byte| **byte == b'\n').count() + 1;
        let column = prefix
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(start + 1, |p| start - p);
        Self::Parse { line, column }
    }

    pub(super) fn io(operation: Operation, error: io::Error) -> Self {
        Self::Io {
            operation,
            kind: error.kind(),
            code: error.raw_os_error(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Location => f.write_str("The per-user settings directory is unavailable; check LOCALAPPDATA (HOME on Unix)."),
            Self::TooLarge => f.write_str("Settings exceed the 64 KiB limit."),
            Self::NotFile => f.write_str("The settings path must be a regular file, not a directory or link."),
            Self::Utf8 => f.write_str("Settings must be UTF-8 text."),
            Self::Version => f.write_str("Unsupported settings version; Pike accepts version = 1."),
            Self::Parse { line, column } => write!(f, "Invalid TOML, unknown field or wrong field type at line {line}, byte column {column}; check the version-one schema."),
            Self::Invalid(field) => f.write_str(match field {
                Field::ProfileId => "Profile identifiers must use 1–32 ASCII letters, digits, underscores or hyphens.",
                Field::Profiles => "profiles must contain 1–16 launch profiles.",
                Field::DefaultProfile => "default_profile must name a configured profile.",
                Field::Shell => "profiles.shell must be an unquoted executable name or absolute path, at most 4096 bytes, without controls; use no shell field for the platform default.",
                Field::Args => "profiles.args accepts at most 64 literal arguments, at most 4096 bytes each / 16384 bytes total, without NUL; explicit shell is required for arguments.",
                Field::Cwd => "profiles.cwd must name an existing absolute directory, at most 4096 bytes, without controls.",
                Field::Environment => "profiles.env accepts at most 64 overrides (16384 bytes total); names must be nonempty without '=' or NUL, and values must not contain NUL; names are case-insensitively unique.",
                Field::Font => "font.family is unsupported; choose a documented monospace family.",
                Field::FontSize => "font.size must be finite and between 10 and 28 DIP.",
                Field::History => "history must be between 0 and 10000 lines.",
                Field::Shortcuts => "shortcuts must use the typed version-one schema, at most 64 unique commands/chords; unsafe, terminal-reserved or conflicting chords are refused.",
            }),
            Self::MissingProfile => f.write_str("This terminal's launch profile was removed. Restore it and reload, or explicitly choose a configured profile for a new terminal."),
            Self::BrokenTarget => f.write_str("The existing settings file is invalid; correct or move it before saving. Its contents were preserved."),
            Self::Serialize => f.write_str("Settings could not be encoded as TOML."),
            Self::Io { operation, kind, code } => write!(f, "Settings {operation:?} failed ({kind:?}, OS code {code:?}); active settings and the previous file were preserved."),
        }
    }
}

impl std::error::Error for Error {}
