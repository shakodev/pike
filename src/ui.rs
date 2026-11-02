//! Static native-workspace presentation; no session, command or timer ownership.
mod label;
pub mod style;
pub mod tokens;
pub use label::label;

use iced::Element;
use iced::widget::{button, container, text, tooltip};

/// A bounded, single-line label, even when its source is untrusted OSC text.
pub fn plain_title(source: &str, limit: usize) -> String {
    let mut chars = source.chars().filter(|c| {
        !c.is_control()
            && !matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    });
    let mut title: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        title.pop();
        if limit > 0 {
            title.push('…');
        }
    }
    title
}

/// Fixed logical hit target. Tooltips receive their effective shortcut from
/// the caller's command resolver; presentation never invents a binding.
pub fn icon<'a, M: Clone + 'a>(
    label: &'a str,
    description: impl Into<String>,
    message: M,
) -> Element<'a, M> {
    reveal_icon(label, description, message, true)
}

/// Reserve the same hit target while an inactive close glyph is at rest.
/// Hover reveal changes paint only and never changes terminal geometry.
pub fn reveal_icon<'a, M: Clone + 'a>(
    label: &'a str,
    description: impl Into<String>,
    message: M,
    visible: bool,
) -> Element<'a, M> {
    tooltip(
        button(
            text(label)
                .font(tokens::FONT)
                .size(tokens::ICON)
                .width(tokens::CONTROL)
                .height(tokens::CONTROL)
                .center(),
        )
        .width(tokens::CONTROL)
        .height(tokens::CONTROL)
        .padding(0)
        .style(move |theme, status| {
            let mut style = style::quiet(theme, status);
            if !visible && status == button::Status::Active {
                style.text_color = iced::Color::TRANSPARENT;
            }
            style
        })
        .on_press(message),
        container(
            text(description.into())
                .font(tokens::FONT)
                .size(tokens::SMALL),
        )
        .padding([4, 8])
        .style(style::tooltip),
        tooltip::Position::Bottom,
    )
    .gap(4)
    .into()
}

#[cfg(test)]
mod tests {
    use super::plain_title;

    #[test]
    fn labels_are_bounded_plain_text_at_unicode_boundaries() {
        assert_eq!(plain_title("one\n\r\0two", 20), "onetwo");
        assert_eq!(plain_title("界é🦀long", 4), "界é🦀…");
        assert_eq!(plain_title("abc", 3), "abc");
        assert_eq!(plain_title("abc", 0), "");
        assert_eq!(plain_title("abc", 1), "…");
    }
}
