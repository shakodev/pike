use crate::settings::FontSettings;
use iced::{Font, Size};
use iced_core::{
    alignment::Vertical,
    text::{Alignment, LineHeight, Paragraph, Shaping as TextShaping},
    Text,
};
use iced_graphics::text::paragraph;

#[derive(Debug, Clone)]
pub struct TermFont {
    pub(crate) size: f32,
    pub(crate) font_type: Font,
    pub(crate) scale_factor: f32,
    pub(crate) measure: Size<f32>,
    measurements: u64,
}

impl TermFont {
    pub fn new(settings: FontSettings) -> Self {
        let settings = normalize(settings, &FontSettings::default());
        Self {
            size: settings.size,
            font_type: settings.font_type,
            scale_factor: settings.scale_factor,
            measure: font_measure(
                settings.size,
                settings.scale_factor,
                settings.font_type,
            ),
            measurements: 1,
        }
    }

    pub fn update(&mut self, settings: FontSettings) -> bool {
        let settings = normalize(
            settings,
            &FontSettings {
                size: self.size,
                scale_factor: self.scale_factor,
                font_type: self.font_type,
            },
        );
        if self.size == settings.size
            && self.scale_factor == settings.scale_factor
            && self.font_type == settings.font_type
        {
            return false;
        }
        self.size = settings.size;
        self.scale_factor = settings.scale_factor;
        self.font_type = settings.font_type;
        self.measure =
            font_measure(self.size, self.scale_factor, self.font_type);
        self.measurements = self.measurements.saturating_add(1);
        true
    }

    pub(crate) fn measurements(&self) -> u64 {
        self.measurements
    }
}

fn normalize(
    mut settings: FontSettings,
    fallback: &FontSettings,
) -> FontSettings {
    // Reject non-finite/nonpositive inputs and bound shaping work for finite
    // extremes. scale_factor is relative line height, not monitor DPI.
    settings.size = if settings.size.is_finite() && settings.size > 0.0 {
        settings.size.clamp(1.0, 256.0)
    } else {
        fallback.size
    };
    settings.scale_factor =
        if settings.scale_factor.is_finite() && settings.scale_factor > 0.0 {
            settings.scale_factor.clamp(0.1, 8.0)
        } else {
            fallback.scale_factor
        };
    settings
}

fn font_measure(
    font_size: f32,
    scale_factor: f32,
    font_type: Font,
) -> Size<f32> {
    let paragraph = paragraph::Paragraph::with_text(Text {
        content: "m",
        font: font_type,
        size: iced_core::Pixels(font_size),
        align_y: Vertical::Center,
        align_x: Alignment::Center,
        shaping: TextShaping::Advanced,
        line_height: LineHeight::Relative(scale_factor),
        bounds: Size::INFINITE,
        wrapping: iced_core::text::Wrapping::Glyph,
    });

    let bounds = paragraph.min_bounds();
    Size::new(bounds.width.max(0.25), bounds.height.max(0.25))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_includes_size_family_and_relative_line_height() {
        let mut font = TermFont::new(Default::default());
        let mut settings = FontSettings::default();
        for change in 0..3 {
            match change {
                0 => settings.size = 20.0,
                1 => settings.scale_factor = 1.6,
                _ => settings.font_type = Font::DEFAULT,
            }
            assert!(font.update(settings.clone()));
            assert!(!font.update(settings.clone()));
            assert_eq!(font.measurements(), change + 2);
            assert!(crate::geometry::valid_size(font.measure));
        }
    }

    #[test]
    fn invalid_font_settings_keep_valid_metrics_and_extremes_shape_bounded_text(
    ) {
        let mut font = TermFont::new(Default::default());
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.0] {
            assert!(!font.update(FontSettings {
                size: value,
                scale_factor: value,
                ..Default::default()
            }));
        }
        assert_eq!(font.measurements(), 1);
        assert!(font.update(FontSettings {
            size: f32::MAX,
            scale_factor: f32::MAX,
            ..Default::default()
        }));
        assert_eq!((font.size, font.scale_factor), (256.0, 8.0));
        assert!(!font.update(FontSettings {
            size: f32::MAX,
            scale_factor: f32::MAX,
            ..Default::default()
        }));
        assert!(crate::geometry::valid_size(font.measure));
    }
}
