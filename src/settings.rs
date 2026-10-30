//! Bounded version-one configuration. Only startup and explicit reload read it.
//! Launch data and parser source text never appear in diagnostic formatting.
use std::{collections::BTreeMap, path::PathBuf};

use iced_term::settings::{BackendSettings, HistoryLimit};
use serde::{Deserialize, Serialize};

mod appearance;
mod error;
mod io;
pub mod shortcuts;
mod validate;
pub use appearance::{Font, FontFamily, Theme};
pub use error::{Error, Field, Operation};
pub use io::{Store, load, user_path};
pub use shortcuts::Shortcut;

pub const VERSION: u32 = 1;
pub const MAX_FILE_BYTES: usize = 65_536;
pub const MAX_PROFILES: usize = 16;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(String);

impl ProfileId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ProfileId {
    fn default() -> Self {
        Self("default".into())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClosePolicy {
    #[default]
    ConfirmRunning,
    Never,
}

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Profile {
    /// None selects the adapter's platform default; Some is always explicit.
    shell: Option<String>,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    env: BTreeMap<String, String>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    #[serde(default)]
    default_profile: ProfileId,
    #[serde(default = "default_profiles")]
    profiles: BTreeMap<ProfileId, Profile>,
    #[serde(default)]
    font: Font,
    #[serde(default)]
    theme: Theme,
    #[serde(default = "default_history")]
    history: usize,
    #[serde(default)]
    close_policy: ClosePolicy,
    #[serde(default)]
    shortcuts: Vec<Shortcut>,
}

fn default_profiles() -> BTreeMap<ProfileId, Profile> {
    BTreeMap::from([(ProfileId::default(), Profile::default())])
}

fn default_history() -> usize {
    HistoryLimit::default().lines()
}

impl Default for Document {
    fn default() -> Self {
        Self {
            version: VERSION,
            default_profile: ProfileId::default(),
            profiles: default_profiles(),
            font: Font::default(),
            theme: Theme::default(),
            history: default_history(),
            close_policy: ClosePolicy::default(),
            shortcuts: Vec::new(),
        }
    }
}

/// Construction is validated as a whole, so a failed reload cannot partially
/// change appearance, launch data, close policy or the future shortcut schema.
#[derive(Clone, Default, PartialEq)]
pub struct Settings(Document);

impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Settings").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProfileId(..)")
    }
}

impl Settings {
    pub fn parse(source: &str) -> Result<Self, Error> {
        if source.len() > MAX_FILE_BYTES {
            return Err(Error::TooLarge);
        }
        // Inspect the version before deserializing a potentially future schema.
        #[derive(Deserialize)]
        struct Header {
            version: u32,
        }
        let header: Header = toml::from_str(source).map_err(|e| Error::parse(source, &e))?;
        if header.version != VERSION {
            return Err(Error::Version);
        }
        let document: Document = toml::from_str(source).map_err(|e| Error::parse(source, &e))?;
        validate::document(&document)?;
        Ok(Self(document))
    }

    /// Explicit serialization only. Pike never saves on startup, reload or exit.
    pub fn to_toml(&self) -> Result<String, Error> {
        toml::to_string_pretty(&self.0).map_err(|_| Error::Serialize)
    }

    pub fn font(&self) -> Font {
        self.0.font
    }

    pub fn theme(&self) -> Theme {
        self.0.theme
    }

    pub fn history(&self) -> HistoryLimit {
        // Invariant established by construction and private Document fields.
        HistoryLimit::try_from(self.0.history).expect("validated history limit")
    }

    pub fn close_policy(&self) -> ClosePolicy {
        self.0.close_policy
    }

    pub fn default_profile(&self) -> &ProfileId {
        &self.0.default_profile
    }

    pub fn profiles(&self) -> impl Iterator<Item = &ProfileId> {
        self.0.profiles.keys()
    }

    pub fn shortcuts(&self) -> &[Shortcut] {
        &self.0.shortcuts
    }

    /// Each call creates independent process-local overrides. No global env or
    /// cwd mutation, expansion, interpolation or shell command parsing occurs.
    pub fn launch(&self, id: &ProfileId) -> Result<BackendSettings, Error> {
        let profile = self.0.profiles.get(id).ok_or(Error::MissingProfile)?;
        Ok(BackendSettings {
            program: profile
                .shell
                .clone()
                .unwrap_or_else(|| BackendSettings::default().program),
            args: profile.args.clone(),
            working_directory: profile.cwd.clone(),
            env: profile.env.clone().into_iter().collect(),
            history: self.history(),
            ..BackendSettings::default()
        })
    }
}

#[cfg(test)]
mod tests;
