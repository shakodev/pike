//! Restrained surfaces and one accent, resolved from the existing app theme.
use iced::widget::{button, container, pane_grid};
use iced::{Background, Border, Color, Theme};

use super::tokens;

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Color,
    pub surface: Color,
    pub raised: Color,
    pub text: Color,
    pub muted: Color,
    pub line: Color,
    pub accent: Color,
}

pub fn palette(theme: &Theme) -> Palette {
    let light = !theme.extended_palette().is_dark;
    let rgb = Color::from_rgb8;
    if light {
        Palette {
            background: rgb(250, 250, 250),
            surface: rgb(240, 241, 243),
            raised: rgb(225, 228, 233),
            text: rgb(32, 32, 32),
            muted: rgb(89, 94, 104),
            line: rgb(202, 206, 214),
            accent: rgb(32, 80, 160),
        }
    } else {
        Palette {
            background: rgb(24, 25, 28),
            surface: rgb(32, 34, 38),
            raised: rgb(47, 50, 57),
            text: rgb(225, 227, 232),
            muted: rgb(158, 164, 176),
            line: rgb(65, 69, 78),
            accent: rgb(126, 167, 230),
        }
    }
}

pub fn workspace(theme: &Theme) -> container::Style {
    let p = palette(theme);
    container::Style {
        background: Some(p.background.into()),
        text_color: Some(p.text),
        ..Default::default()
    }
}

pub fn surface(theme: &Theme) -> container::Style {
    let p = palette(theme);
    container::Style {
        background: Some(p.surface.into()),
        ..workspace(theme)
    }
}

pub fn tooltip(theme: &Theme) -> container::Style {
    let p = palette(theme);
    container::Style {
        border: Border {
            color: p.line,
            width: 1.0,
            radius: 3.0.into(),
        },
        ..surface(theme)
    }
}

pub fn header(theme: &Theme, focused: bool) -> container::Style {
    let p = palette(theme);
    container::Style {
        text_color: Some(if focused { p.text } else { p.muted }),
        // PaneGrid paints its title after the pane background/border. Repeat
        // the same 1-DIP outline so the header cannot cover the focus edge.
        border: Border {
            width: tokens::BORDER,
            color: if focused { p.accent } else { p.line },
            ..Default::default()
        },
        ..surface(theme)
    }
}

pub fn pane(theme: &Theme, focused: bool) -> container::Style {
    let p = palette(theme);
    container::Style {
        border: Border {
            width: tokens::BORDER,
            color: if focused { p.accent } else { p.line },
            ..Default::default()
        },
        ..workspace(theme)
    }
}

pub fn quiet(theme: &Theme, status: button::Status) -> button::Style {
    let p = palette(theme);
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hovered.then_some(Background::Color(p.raised)),
        text_color: if status == button::Status::Disabled {
            p.muted
        } else {
            p.text
        },
        border: Border {
            radius: 3.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn tab(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let p = palette(theme);
    let mut style = quiet(theme, status);
    if selected {
        style.background = Some(p.background.into());
        style.text_color = p.accent;
    } else if status == button::Status::Active {
        style.text_color = p.muted;
    }
    style
}

pub fn dividers(theme: &Theme) -> pane_grid::Style {
    let p = palette(theme);
    let mut style = pane_grid::default(theme);
    style.hovered_split = pane_grid::Line {
        color: p.muted,
        width: tokens::GUTTER,
    };
    style.picked_split = pane_grid::Line {
        color: p.accent,
        width: tokens::GUTTER,
    };
    style
}
