use iced_term::{ColorPalette, settings::FontSettings};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FontFamily {
    #[default]
    Monospace,
    Consolas,
    CascadiaMono,
    Menlo,
    DejaVuSansMono,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Font {
    pub family: FontFamily,
    pub size: f32,
}

impl Default for Font {
    fn default() -> Self {
        Self {
            family: FontFamily::default(),
            size: 14.0,
        }
    }
}

impl Font {
    pub fn terminal(self) -> FontSettings {
        // Pinned Iced Font::with_name requires &'static str. A closed bounded
        // family set avoids leaking a new interned allocation on every reload.
        let font_type = match self.family {
            FontFamily::Monospace if cfg!(windows) => iced::Font::with_name("Consolas"),
            FontFamily::Monospace => iced::Font::MONOSPACE,
            FontFamily::Consolas => iced::Font::with_name("Consolas"),
            FontFamily::CascadiaMono => iced::Font::with_name("Cascadia Mono"),
            FontFamily::Menlo => iced::Font::with_name("Menlo"),
            FontFamily::DejaVuSansMono => iced::Font::with_name("DejaVu Sans Mono"),
        };
        FontSettings {
            size: self.size,
            font_type,
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
}

impl Theme {
    pub fn is_light(self, system: iced::theme::Mode) -> bool {
        self == Self::Light || (self == Self::System && system == iced::theme::Mode::Light)
    }

    pub fn palette(self, system: iced::theme::Mode) -> Box<ColorPalette> {
        if !self.is_light(system) {
            return Box::default();
        }
        Box::new(ColorPalette {
            foreground: "#202020".into(),
            background: "#fafafa".into(),
            black: "#202020".into(),
            red: "#a02626".into(),
            green: "#286428".into(),
            yellow: "#805800".into(),
            blue: "#2050a0".into(),
            magenta: "#804080".into(),
            cyan: "#006870".into(),
            white: "#c0c0c0".into(),
            bright_black: "#606060".into(),
            bright_red: "#b83030".into(),
            bright_green: "#307830".into(),
            bright_yellow: "#906800".into(),
            bright_blue: "#2860b0".into(),
            bright_magenta: "#905090".into(),
            bright_cyan: "#007880".into(),
            bright_white: "#ffffff".into(),
            bright_foreground: Some("#101010".into()),
            dim_foreground: "#606060".into(),
            ..Default::default()
        })
    }
}
