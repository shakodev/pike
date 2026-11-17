use super::*;

fn luminance(color: Color) -> f32 {
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}
fn contrast(a: Color, b: Color) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn accessibility_chrome_text_and_focus_meet_contrast_including_disabled_choices() {
    // Iced's built-in Light also uses a black/white base. Only the explicit
    // custom themes may replace Pike's existing restrained chrome palette.
    assert_eq!(
        palette(&Theme::Light).background,
        Color::from_rgb8(250, 250, 250)
    );
    assert_eq!(palette(&Theme::Light).accent, Color::from_rgb8(32, 80, 160));
    for theme in [
        Theme::Dark,
        Theme::Light,
        high_contrast(false),
        high_contrast(true),
    ] {
        let p = palette(&theme);
        let minimum = if p.background == Color::BLACK || p.background == Color::WHITE {
            7.0
        } else {
            4.5
        };
        for background in [p.background, p.surface, p.raised] {
            for foreground in [p.text, p.muted] {
                assert!(
                    contrast(foreground, background) >= minimum,
                    "{theme:?}: {foreground:?} on {background:?}"
                );
            }
            assert!(
                contrast(p.accent, background) >= 3.0,
                "focus border in {theme:?}"
            );
        }
        for status in [
            button::Status::Active,
            button::Status::Hovered,
            button::Status::Disabled,
        ] {
            let selected = choice(&theme, status, true);
            let ordinary = choice(&theme, status, false);
            assert_eq!(selected.border.width, ordinary.border.width);
            assert!(selected.border.width >= 2.0);
            assert!(contrast(selected.border.color, p.surface) >= 3.0);
            assert!(contrast(selected.text_color, p.surface) >= minimum);
        }
    }
}
