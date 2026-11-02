//! Width-aware plain-text ellipsis. The pinned renderer's Wrapping::None can
//! produce infinite paragraph widths; measure a bounded, normally shaped line.
use iced::advanced::text::Paragraph as _;
use iced::advanced::{
    Layout, Widget, layout, renderer, text,
    widget::{Operation, Tree, tree},
};
use iced::{Element, Length, Rectangle, Size};

use super::{plain_title, tokens};

type Paragraph = <iced::Renderer as text::Renderer>::Paragraph;

#[derive(Default)]
struct State {
    source: String,
    display: String,
    width: f32,
    font: Option<iced::Font>,
    paragraph: text::paragraph::Plain<Paragraph>,
}

pub fn label<'a, M: 'a>(source: impl Into<String>) -> Element<'a, M> {
    Element::new(Label(plain_title(&source.into(), tokens::TITLE_LIMIT)))
}

struct Label(String);

impl<M> Widget<M, iced::Theme, iced::Renderer> for Label {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        let font = tokens::FONT;
        let width = limits.max().width.min(10000.0);
        if state.source != self.0 || state.width != width || state.font != Some(font) {
            let measure = |value: &str| {
                Paragraph::with_text(text::Text {
                    content: value,
                    bounds: Size::new(100000.0, 10000.0),
                    size: iced::Pixels(tokens::BODY),
                    line_height: text::LineHeight::default(),
                    font,
                    align_x: text::Alignment::Default,
                    align_y: iced::alignment::Vertical::Top,
                    shaping: text::Shaping::Advanced,
                    wrapping: text::Wrapping::WordOrGlyph,
                })
                .min_bounds()
                .width
            };
            state.display = if measure(&self.0) <= width {
                self.0.clone()
            } else if measure("…") > width {
                String::new()
            } else {
                let chars: Vec<_> = self.0.chars().collect();
                let mut low = 0;
                let mut high = chars.len();
                while low < high {
                    let middle = (low + high).div_ceil(2);
                    let trial: String = chars[..middle]
                        .iter()
                        .chain(std::iter::once(&'…'))
                        .collect();
                    if measure(&trial) <= width {
                        low = middle;
                    } else {
                        high = middle - 1;
                    }
                }
                chars[..low].iter().chain(std::iter::once(&'…')).collect()
            };
            state.source.clone_from(&self.0);
            state.width = width;
            state.font = Some(font);
        }
        iced::widget::text::layout(
            &mut state.paragraph,
            renderer,
            limits,
            &state.display,
            iced::widget::text::Format {
                size: Some(iced::Pixels(tokens::BODY)),
                font: Some(font),
                shaping: text::Shaping::Advanced,
                ..Default::default()
            },
        )
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        _: iced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        iced::widget::text::draw(
            renderer,
            style,
            layout.bounds(),
            state.paragraph.raw(),
            Default::default(),
            viewport,
        );
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.text(
            None,
            layout.bounds(),
            &tree.state.downcast_ref::<State>().display,
        );
    }
}
