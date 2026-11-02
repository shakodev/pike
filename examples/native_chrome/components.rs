//! Focused native component and activity checks; no App/session model mutation.
use crate::report;
use iced::advanced::{
    Layout, layout, mouse,
    renderer::{Headless, Renderer as _, Style},
    widget::Tree,
};
use iced::{Color, Rectangle, Size};
use pike::app::Control;
use std::{fs, io, path::Path, time::Instant};

pub fn verify(renderer: &mut iced::Renderer, output: &Path, activity: bool) -> io::Result<()> {
    use iced::advanced::{
        Shell, clipboard,
        widget::{Id, Operation},
    };
    use pike::{
        tab_strip::{self, Item},
        tabs::{Activity, Title},
    };
    #[derive(Default)]
    struct Bounds {
        strip: Option<Rectangle>,
        overflow: Option<Rectangle>,
        ellipses: usize,
        indicators: usize,
        expected: Option<&'static str>,
    }
    impl Operation for Bounds {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
            if id == Some(&Id::from("tab-strip")) {
                self.strip = Some(bounds);
            }
            if id == Some(&Id::from("tab-overflow-control")) {
                self.overflow = Some(bounds);
            }
        }
        fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
            assert!(
                bounds.x.abs() < 10000.0
                    && bounds.y.abs() < 10000.0
                    && bounds.width < 10000.0
                    && bounds.height < 10000.0,
                "nonfinite component text layout: {bounds:?} controlled_label={text}"
            );
            if text.ends_with('…') {
                assert!(bounds.width <= 134.01);
                self.ellipses += 1;
            }
            if self.expected == Some(text) {
                assert!(bounds.width > 0.0 && bounds.width < 60.0);
                self.indicators += 1;
            }
        }
    }
    let mut model = pike::workspace::Model::default();
    for _ in 0..12 {
        model
            .create_tab()
            .map_err(|_| io::Error::other("controlled tab setup"))?;
    }
    let states = if activity {
        vec![
            (
                "unread",
                Activity {
                    unread: true,
                    ..Activity::default()
                },
                Some("●"),
            ),
            (
                "bell",
                Activity {
                    bell: true,
                    ..Activity::default()
                },
                Some("!"),
            ),
            (
                "exited",
                Activity {
                    exited: true,
                    ..Activity::default()
                },
                Some("■"),
            ),
            (
                "combined",
                Activity {
                    unread: true,
                    bell: true,
                    exited: true,
                },
                Some("● ! ■"),
            ),
        ]
    } else {
        vec![("quiet", Activity::default(), None)]
    };
    for (state, flags, expected) in states {
        for (script, selected_index) in [("ascii", 11), ("cjk", 10)] {
            for (name, theme) in [("dark", iced::Theme::Dark), ("light", iced::Theme::Light)] {
                for width in [200.0, 320.0, 1200.0] {
                    let size = Size::new(width, pike::ui::tokens::TAB_HEIGHT);
                    let items = model
                        .workspace()
                        .order()
                        .iter()
                        .enumerate()
                        .map(|(index, id)| Item {
                            id: *id,
                            title: Title::new(if index.is_multiple_of(2) {
                                "界界界界界界界界界界界界界界界界界界界界界界界界界界界界"
                            } else {
                                "Controlled long tab name with Unicode and truncation"
                            })
                            .unwrap(),
                            activity: flags,
                        })
                        .collect();
                    let mut view = tab_strip::view(
                        items,
                        Some(model.workspace().order()[selected_index]),
                        Control::ActivateTab,
                        Control::CloseTab,
                        Control::TabAction,
                    );
                    let mut tree = Tree::new(view.as_widget());
                    let node = view.as_widget_mut().layout(
                        &mut tree,
                        renderer,
                        &layout::Limits::new(size, size),
                    );
                    let mut bounds = Bounds {
                        expected,
                        ..Default::default()
                    };
                    view.as_widget_mut().operate(
                        &mut tree,
                        Layout::new(&node),
                        renderer,
                        &mut bounds,
                    );
                    let strip = bounds
                        .strip
                        .ok_or_else(|| io::Error::other("missing actual strip"))?;
                    let overflow = bounds
                        .overflow
                        .ok_or_else(|| io::Error::other("missing overflow"))?;
                    assert_eq!(strip.height, 34.0);
                    assert_eq!(bounds.ellipses, if width < 600.0 { 1 } else { 5 });
                    assert_eq!(
                        bounds.indicators,
                        if activity { bounds.ellipses } else { 0 }
                    );
                    assert_eq!(overflow.width, 26.0);
                    assert_eq!(overflow.height, 26.0);
                    assert_eq!(
                        Rectangle::with_size(size).intersection(&overflow),
                        Some(overflow)
                    );
                    let mut messages = Vec::new();
                    for event in [
                        iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),
                        iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left),
                    ] {
                        view.as_widget_mut().update(
                            &mut tree,
                            &iced::Event::Mouse(event),
                            Layout::new(&node),
                            mouse::Cursor::Available(overflow.center()),
                            renderer,
                            &mut clipboard::Null,
                            &mut Shell::new(&mut messages),
                            &Rectangle::with_size(size),
                        );
                    }
                    assert!(messages.iter().any(|m| matches!(
                        m,
                        Control::TabAction(tab_strip::TabAction::OpenOverflow)
                    )));
                    for _ in 0..2 {
                        let mut messages = Vec::new();
                        let mut shell = Shell::new(&mut messages);
                        view.as_widget_mut().update(
                            &mut tree,
                            &iced::Event::Window(iced::window::Event::RedrawRequested(
                                Instant::now(),
                            )),
                            Layout::new(&node),
                            mouse::Cursor::Unavailable,
                            renderer,
                            &mut clipboard::Null,
                            &mut shell,
                            &Rectangle::with_size(size),
                        );
                        assert_eq!(shell.redraw_request(), iced::window::RedrawRequest::Wait);
                    }
                    for scale in [1.0, 1.25, 1.5, 2.0] {
                        renderer.reset(Rectangle::with_size(size));
                        view.as_widget().draw(
                            &tree,
                            renderer,
                            &theme,
                            &Style::default(),
                            Layout::new(&node),
                            mouse::Cursor::Unavailable,
                            &Rectangle::with_size(size),
                        );
                        let dimensions =
                            Size::new((size.width * scale) as u32, (size.height * scale) as u32);
                        let rgba = Headless::screenshot(renderer, dimensions, scale, Color::BLACK);
                        report::screenshot(
                            &output.join(format!(
                                "tabs-{state}-{script}-{name}-{width}-{}.ppm",
                                (scale * 100.0) as u32
                            )),
                            iced::window::Screenshot::new(rgba, dimensions, scale),
                        )?;
                    }
                }
            }
        }
    }
    if activity {
        overflow_activity(renderer, output)?;
    }
    fs::write(
        output.join("completed.txt"),
        "Actual tab strip: 34 DIP; overflow 26x26 DIP and real hit-tested at 200/320/1200 DIP in dark/light at 100/125/150/200%.\n",
    )?;
    Ok(())
}

pub fn verify_overflow(renderer: &mut iced::Renderer, output: &Path) -> io::Result<()> {
    overflow_activity(renderer, output)?;
    fs::write(
        output.join("completed.txt"),
        "All individual/combined activity markers fit actual overflow viewports with long ASCII/CJK at four scales.\n",
    )
}

fn overflow_activity(renderer: &mut iced::Renderer, output: &Path) -> io::Result<()> {
    use iced::advanced::{
        Shell, clipboard,
        widget::{Id, Operation},
    };
    use pike::{
        tab_strip::{self, Item},
        tabs::{Activity, Title},
    };
    let flags = [
        Activity {
            unread: true,
            ..Activity::default()
        },
        Activity {
            bell: true,
            ..Activity::default()
        },
        Activity {
            exited: true,
            ..Activity::default()
        },
        Activity {
            unread: true,
            bell: true,
            exited: true,
        },
    ];
    let mut model = pike::workspace::Model::default();
    for _ in 0..8 {
        model
            .create_tab()
            .map_err(|_| io::Error::other("activity list setup"))?;
    }
    for (name, theme) in [("dark", iced::Theme::Dark), ("light", iced::Theme::Light)] {
        for width in [200.0, 320.0] {
            let size = Size::new(width, 440.0);
            let viewport = Rectangle::with_size(size);
            let items = model
                .workspace()
                .order()
                .iter()
                .enumerate()
                .map(|(index, id)| Item {
                    id: *id,
                    title: Title::new(if index.is_multiple_of(2) {
                        "Controlled long ASCII activity title to exercise measured truncation"
                    } else {
                        "界界界界界界界界界界界界界界界界界界界界界界界界界界界界"
                    })
                    .unwrap(),
                    activity: flags[index / 2],
                })
                .collect();
            let mut view = tab_strip::overflow(
                items,
                model.workspace().active(),
                Id::unique(),
                Control::ActivateTab,
                Control::CloseTab,
                Control::DismissHelp,
            );
            let mut tree = Tree::new(view.as_widget());
            let node =
                view.as_widget_mut()
                    .layout(&mut tree, renderer, &layout::Limits::new(size, size));
            struct Markers {
                counts: [usize; 4],
                viewport: Rectangle,
                translation: iced::Vector,
                pending: Option<(iced::Vector, Rectangle)>,
            }
            impl Operation for Markers {
                fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                    let saved = (self.translation, self.viewport);
                    if let Some((translation, bounds)) = self.pending.take() {
                        self.viewport = self.viewport.intersection(&bounds).unwrap_or_default();
                        self.translation += translation;
                    }
                    operate(self);
                    (self.translation, self.viewport) = saved;
                }
                fn scrollable(
                    &mut self,
                    _: Option<&Id>,
                    bounds: Rectangle,
                    _: Rectangle,
                    translation: iced::Vector,
                    _: &mut dyn iced::advanced::widget::operation::Scrollable,
                ) {
                    self.pending = Some((translation, bounds - self.translation));
                }
                fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
                    if let Some(index) = ["●", "!", "■", "● ! ■"]
                        .iter()
                        .position(|marker| *marker == text)
                    {
                        let bounds = bounds - self.translation;
                        let allowed = self.viewport.expand(0.001);
                        assert!(
                            allowed.contains(bounds.position())
                                && allowed.contains(iced::Point::new(
                                    bounds.x + bounds.width,
                                    bounds.y + bounds.height
                                )),
                            "activity marker clipped out of actual overflow viewport"
                        );
                        self.counts[index] += 1;
                    }
                }
            }
            let mut markers = Markers {
                counts: [0; 4],
                viewport,
                translation: iced::Vector::ZERO,
                pending: None,
            };
            view.as_widget_mut()
                .operate(&mut tree, Layout::new(&node), renderer, &mut markers);
            assert_eq!(markers.counts, [2; 4]);
            for _ in 0..2 {
                let mut messages = Vec::new();
                let mut shell = Shell::new(&mut messages);
                view.as_widget_mut().update(
                    &mut tree,
                    &iced::Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
                    Layout::new(&node),
                    mouse::Cursor::Unavailable,
                    renderer,
                    &mut clipboard::Null,
                    &mut shell,
                    &viewport,
                );
                assert_eq!(shell.redraw_request(), iced::window::RedrawRequest::Wait);
            }
            for scale in [1.0, 1.25, 1.5, 2.0] {
                renderer.reset(viewport);
                view.as_widget().draw(
                    &tree,
                    renderer,
                    &theme,
                    &Style::default(),
                    Layout::new(&node),
                    mouse::Cursor::Unavailable,
                    &viewport,
                );
                let dimensions =
                    Size::new((size.width * scale) as u32, (size.height * scale) as u32);
                let rgba = Headless::screenshot(renderer, dimensions, scale, Color::BLACK);
                report::screenshot(
                    &output.join(format!(
                        "activity-overflow-{name}-{width}-{}.ppm",
                        (scale * 100.0) as u32
                    )),
                    iced::window::Screenshot::new(rgba, dimensions, scale),
                )?;
            }
        }
    }
    Ok(())
}
