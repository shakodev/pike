use crate::backend::{Backend, Command, LinkAction, MouseButton};
use crate::bindings::{BindingAction, InputKind};
use crate::terminal::{Event, Terminal};
use crate::theme::TerminalStyle;
use crate::RenderableContent;
use alacritty_terminal::index::Point as TerminalGridPoint;
use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::{cell, TermMode};
use alacritty_terminal::vte::ansi::{self as ansi, NamedColor};
use iced::alignment::Vertical;
use iced::mouse::{Cursor, ScrollDelta};
use iced::widget::canvas::{Path, Text};
use iced::widget::container;
use iced::{Color, Element, Length, Point, Rectangle, Size, Theme, Vector};
use iced_core::clipboard::Kind as ClipboardKind;
use iced_core::keyboard::{Key, Modifiers};
use iced_core::mouse::{self, Click};
use iced_core::text::{Alignment, LineHeight, Shaping};

mod cursor;
mod glyphs;
mod pointer;
#[cfg(test)]
mod paste_tests;
use iced_core::widget::operation::{self, Focusable};
use iced_graphics::core::widget::{tree, Tree};
use iced_graphics::core::Widget;
use iced_graphics::geometry::Stroke;

pub struct TerminalView<'a> {
    term: &'a Terminal,
}

impl<'a> TerminalView<'a> {
    pub fn show(term: &'a Terminal) -> Element<'a, Event> {
        container(Self { term })
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| term.backend.theme.container_style())
            .into()
    }

    pub fn focus<Message: 'static>(
        id: iced::widget::Id,
    ) -> iced::Task<Message> {
        iced::widget::operation::focus(id)
    }

    fn is_cursor_in_layout(
        &self,
        cursor: Cursor,
        layout: iced_graphics::core::Layout<'_>,
    ) -> bool {
        if let Some(cursor_position) = cursor.position() {
            let layout_position = layout.position();
            let layout_size = layout.bounds();
            let is_triggered = cursor_position.x >= layout_position.x
                && cursor_position.y >= layout_position.y
                && cursor_position.x < (layout_position.x + layout_size.width)
                && cursor_position.y < (layout_position.y + layout_size.height);

            return is_triggered;
        }

        false
    }

    fn is_cursor_hovered_hyperlink(&self, state: &TerminalViewState) -> bool {
        let content = self.term.backend.renderable_content();
        if let Some(hyperlink_range) = &content.hovered_hyperlink {
            return hyperlink_range.contains(&state.mouse_position_on_grid);
        }

        false
    }

    fn cell_colors(
        &self,
        content: &RenderableContent,
        cell: &cell::Cell,
        point: TerminalGridPoint,
    ) -> (Color, Color) {
        let mut fg = self.term.backend.theme.get_color(cell.fg);
        let mut bg = self.term.backend.theme.get_color(cell.bg);
        if cell
            .flags
            .intersects(cell::Flags::DIM | cell::Flags::DIM_BOLD)
        {
            fg.a *= 0.7;
        }
        if cell.flags.contains(cell::Flags::INVERSE)
            || content
                .selectable_range
                .is_some_and(|range| range.contains(point))
        {
            std::mem::swap(&mut fg, &mut bg);
        }
        (fg, bg)
    }

    fn handle_resize(
        &mut self,
        state: &mut TerminalViewState,
        layout: iced_graphics::core::Layout<'_>,
        shell: &mut iced_graphics::core::Shell<'_, Event>,
    ) {
        let bounds = layout.bounds();
        let previous = state.bounds.replace(bounds);
        if previous != Some(bounds) {
            // A moved pane must not continue a drag/double-click at its old
            // screen position. Selection itself remains owned by the session.
            for command in Self::cancel_pointer(state) {
                shell.publish(Event::BackendCall(self.term.id, command));
            }
        }
        if previous.map(|bounds| bounds.size()) != Some(bounds.size()) {
            // Every session needs its first layout, including equal-sized tab
            // replacements. The backend deduplicates unchanged cell geometry.
            shell.publish(Event::BackendCall(
                self.term.id,
                Command::Resize(Some(bounds.size()), None),
            ));
        }
    }

    fn handle_focus(
        &self,
        event: &iced_core::Event,
        state: &mut TerminalViewState,
        is_cursor_in_layout: bool,
    ) {
        use iced::Event::Mouse;
        use iced_core::mouse::{Button::Left, Event::ButtonPressed};

        if let Mouse(ButtonPressed(Left)) = event {
            if is_cursor_in_layout {
                state.focus();
            } else {
                state.unfocus();
            }
        }
    }

    fn handle_keyboard_event(
        &self,
        state: &mut TerminalViewState,
        clipboard: &mut dyn iced_graphics::core::Clipboard,
        event: &iced::keyboard::Event,
        shell: &mut iced_core::Shell<'_, Event>,
    ) -> Option<Command> {
        let mut binding_action = BindingAction::Ignore;
        let last_content = self.term.backend.renderable_content();
        match event {
            iced::keyboard::Event::ModifiersChanged(m) => {
                state.keyboard_modifiers = *m;
                let action = if state.keyboard_modifiers == Modifiers::COMMAND {
                    LinkAction::Hover
                } else {
                    LinkAction::Clear
                };
                return Some(Command::ProcessLink(
                    action,
                    state.mouse_position_on_grid,
                ));
            },
            iced::keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            } => match &key {
                // Use the physical character key for bindings even when text is None (e.g., Ctrl/Cmd combos)
                Key::Character(k) => {
                    let lower = k.to_ascii_lowercase();
                    binding_action = self.term.bindings.get_action(
                        InputKind::Char(lower),
                        *modifiers,
                        last_content.terminal_mode,
                    );

                    // If no binding matched, only write printable text (when provided)
                    if binding_action == BindingAction::Ignore {
                        if let Some(c) = text {
                            return Some(Command::Write(c.as_bytes().to_vec()));
                        }
                    }
                },
                Key::Named(code) => {
                    binding_action = self.term.bindings.get_action(
                        InputKind::KeyCode(*code),
                        *modifiers,
                        last_content.terminal_mode,
                    );
                },
                _ => {},
            },
            _ => {},
        }

        match binding_action {
            BindingAction::Char(c) => {
                let mut buf = [0, 0, 0, 0];
                let str = c.encode_utf8(&mut buf);
                return Some(Command::Write(str.as_bytes().to_vec()));
            },
            BindingAction::Esc(seq) => {
                return Some(Command::Write(seq.as_bytes().to_vec()));
            },
            BindingAction::Paste => {
                let data = clipboard.read(ClipboardKind::Standard);
                return Some(self.term.backend.queue_paste(data));
            },
            BindingAction::ScrollHistory(scroll) => {
                shell.capture_event();
                return Some(Command::ScrollHistory(scroll));
            },
            BindingAction::Copy => {
                self.term.copy_selection(|text| {
                    clipboard.write(ClipboardKind::Standard, text);
                });
                // Copy is handled even with no selection or an unavailable
                // clipboard; it must never reach an ancestor or terminal text.
                shell.capture_event();
            },
            _ => {},
        };

        None
    }
}

impl Widget<Event, Theme, iced::Renderer> for TerminalView<'_> {
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<TerminalViewState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(TerminalViewState::new(self.term.id))
    }

    fn diff(&self, tree: &mut Tree) {
        // Iced reconciles by widget type/position, not by terminal session.
        // Backport upstream PR 70: never reuse another session's input state.
        if tree.state.downcast_ref::<TerminalViewState>().terminal_id
            != self.term.id
        {
            tree.state = self.state();
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &iced_core::layout::Limits,
    ) -> iced_core::layout::Node {
        let size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
        iced::advanced::layout::Node::new(size)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: iced_core::Layout<'_>,
        _renderer: &iced::Renderer,
        operation: &mut dyn operation::Operation,
    ) {
        let state = tree.state.downcast_mut::<TerminalViewState>();
        let wid = self.term.widget_id();
        operation.focusable(Some(wid), layout.bounds(), state);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        layout: iced::advanced::Layout,
        _cursor: Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if !self.term.is_visible() {
            return;
        }
        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return;
        }
        let state = tree.state.downcast_ref::<TerminalViewState>();
        let content = self.term.backend.renderable_content();
        let term_size = content.terminal_size;
        let cell_width = term_size.cell_width;
        let cell_height = term_size.cell_height;
        let font_size = self.term.font.size;
        let font_scale_factor = self.term.font.scale_factor;
        let focused =
            content.focused && self.term.backend.window_focus.is_focused();
        if self
            .term
            .cursor_focused
            .replace(Some(focused))
            .is_some_and(|previous| previous != focused)
        {
            self.term.redraw();
        }
        let cursor = cursor::Cursor::new(
            content,
            &self.term.backend.theme,
            focused,
            |cell, point| self.cell_colors(content, cell, point),
        );
        // Cache only pane-local geometry. Repositioning reuses the same cache;
        // translation and clipping always come from the current layout.
        let geom = self.term.cache.draw(renderer, bounds.size(), |frame| {
            // Precompute constants used in the inner loop
            let display_offset = content.viewport.display_offset() as f32;
            let cell_size = Size::new(cell_width, cell_height);
            let half_h = cell_height * 0.5;
            // We use the background pallete color as a default
            // because the widget global background color must be the same
            let default_bg = self
                .term
                .backend
                .theme
                .get_color(ansi::Color::Named(NamedColor::Background));

            let mut last_line: Option<i32> = None;
            let mut bg_batch_rect = BackgroundRect::default();

            for indexed in content.viewport.display_iter() {
                // Compute per-cell geometry cheaply
                let line = indexed.point.line.0;
                let col = indexed.point.column.0 as f32;

                // Resolve position point for this cell
                let x = col * cell_width;
                let (_, bg) =
                    self.cell_colors(content, indexed.cell, indexed.point);

                // If the new line was detected,
                // need to flush pending background rect and init the new one
                if last_line != Some(line) {
                    if bg_batch_rect.can_flush() {
                        let line = last_line.unwrap_or(line);
                        frame.fill(
                            &bg_batch_rect.build(line),
                            bg_batch_rect.color,
                        );
                    }

                    last_line = Some(line);
                    bg_batch_rect = BackgroundRect::default()
                        .with_cell_height(cell_height)
                        .with_display_offset(display_offset);
                }

                // Batch draw backgrounds: skip default background (container already paints it)
                if bg != default_bg {
                    if bg_batch_rect.can_extend(bg, x) {
                        // Same color and contiguous: extend current run
                        bg_batch_rect.extend(cell_width);
                    } else {
                        // New colored run (or non-contiguous): flush previous run if any
                        if bg_batch_rect.can_flush() {
                            frame.fill(
                                &bg_batch_rect.build(line),
                                bg_batch_rect.color,
                            );
                        }

                        // Start a new run but do not draw yet; wait for potential extensions
                        bg_batch_rect = BackgroundRect::default()
                            .with_cell_height(cell_height)
                            .with_display_offset(display_offset)
                            .activate()
                            .with_color(bg)
                            .with_start_x(x)
                            .with_width(cell_width);
                    }
                } else if bg_batch_rect.can_flush() {
                    // Background returns to default, flush current background rect and init the new one
                    frame.fill(&bg_batch_rect.build(line), bg_batch_rect.color);

                    bg_batch_rect = BackgroundRect::default()
                        .with_cell_height(cell_height)
                        .with_display_offset(display_offset);
                }
            }

            // Flush any remaining background run at the end
            if bg_batch_rect.can_flush() {
                frame.fill(
                    &bg_batch_rect.build(last_line.unwrap_or(0)),
                    bg_batch_rect.color,
                );
            }

            // Backgrounds precede all glyphs, including wide spans and cursor.
            for indexed in content.viewport.display_iter() {
                let x = indexed.point.column.0 as f32 * cell_width;
                let y = (indexed.point.line.0 as f32 + display_offset)
                    * cell_height;
                let cell_center_y = y + half_h;
                let (fg, _) =
                    self.cell_colors(content, indexed.cell, indexed.point);
                let columns = glyphs::columns(
                    indexed.cell,
                    content.viewport.columns() - indexed.point.column.0,
                );
                // Draw hovered hyperlink underline (rare; keep per-cell for correctness)
                if content.hovered_hyperlink.as_ref().is_some_and(|range| {
                    range.contains(&indexed.point)
                        && range.contains(&state.mouse_position_on_grid)
                }) || indexed.cell.flags.contains(cell::Flags::UNDERLINE)
                {
                    let underline_height = y + cell_size.height;
                    let underline = Path::line(
                        Point::new(x, underline_height),
                        Point::new(x + cell_size.width, underline_height),
                    );
                    frame.stroke(
                        &underline,
                        Stroke::default()
                            .with_width(font_size * 0.15)
                            .with_color(fg),
                    );
                }

                // Handle cursor rendering
                if let Some(cursor) =
                    cursor.filter(|cursor| cursor.point == indexed.point)
                {
                    cursor.draw(frame);
                }

                // ASCII retains the existing pane geometry cache. Unicode uses
                // the core text renderer below: tiny-skia canvas text ignores
                // per-text clip bounds in the pinned Iced implementation.
                if columns > 0
                    && glyphs::visible(indexed.cell)
                    && !glyphs::clipped(indexed.cell)
                {
                    let fg = cursor.map_or(fg, |cursor| {
                        cursor.glyph_color(indexed.point, fg)
                    });
                    frame.fill_text(Text {
                        content: glyphs::text(indexed.cell),
                        position: Point::new(
                            x + cell_width * 0.5,
                            cell_center_y,
                        ),
                        font: glyphs::font(
                            self.term.font.font_type,
                            indexed.flags,
                        ),
                        size: iced_core::Pixels(font_size),
                        color: fg,
                        align_x: Alignment::Center,
                        align_y: Vertical::Center,
                        shaping: Shaping::Advanced,
                        line_height: LineHeight::Relative(font_scale_factor),
                        ..Default::default()
                    });
                }
            }
        });

        use iced::advanced::graphics::geometry::Renderer as _;
        use iced_core::Renderer as _;
        renderer.with_layer(clip, |renderer| {
            renderer.with_translation(
                Vector::new(bounds.x, bounds.y),
                |renderer| {
                    renderer.draw_geometry(geom);
                    use iced_graphics::text::Renderer as _;
                    let mut cache = state.glyphs.borrow_mut();
                    let generation = (
                        self.term.snapshot_stats().snapshots,
                        self.term.work_stats().font_measurements,
                    );
                    cache.update(generation, |glyphs| {
                        glyphs::build(content, &self.term.font, glyphs);
                    });
                    // Iced's layer stack replaces, rather than intersects,
                    // the parent clip. Preserve the current pane/ancestor
                    // viewport when applying each cached local cell clip.
                    let ancestor = Rectangle::new(
                        Point::new(clip.x - bounds.x, clip.y - bounds.y),
                        clip.size(),
                    );
                    for glyph in &cache.glyphs {
                        let Some(cell) =
                            content.viewport.cells().get(glyph.cell_index)
                        else {
                            continue;
                        };
                        let point = TerminalGridPoint::new(
                            alacritty_terminal::index::Line(
                                (glyph.cell_index / content.viewport.columns())
                                    as i32
                                    - content.viewport.display_offset() as i32,
                            ),
                            alacritty_terminal::index::Column(
                                glyph.cell_index % content.viewport.columns(),
                            ),
                        );
                        let (fg, _) = self.cell_colors(content, cell, point);
                        let fg = cursor
                            .map_or(fg, |cursor| cursor.glyph_color(point, fg));
                        let Some(clip) = glyph.clip.intersection(&ancestor)
                        else {
                            continue;
                        };
                        renderer.with_layer(clip, |renderer| {
                            renderer.fill_raw(iced_graphics::text::Raw {
                                buffer: std::sync::Arc::downgrade(
                                    &glyph.buffer,
                                ),
                                position: glyph.position,
                                color: fg,
                                clip_bounds: clip,
                            });
                        });
                    }
                },
            );
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced_core::Event,
        layout: iced_graphics::core::Layout<'_>,
        cursor: Cursor,
        _renderer: &iced::Renderer,
        clipboard: &mut dyn iced_graphics::core::Clipboard,
        shell: &mut iced_graphics::core::Shell<'_, Event>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<TerminalViewState>();
        self.handle_resize(state, layout, shell);
        let mut cleanup = Vec::new();
        match event {
            iced::Event::Window(iced::window::Event::Focused) => {
                self.term.backend.window_focus.set_focused(true);
            },
            iced::Event::Window(iced::window::Event::Rescaled(_)) => {
                // Iced layout/cells stay in logical pixels; force the renderer
                // to recreate geometry for the new physical scale once.
                self.term.redraw();
                shell.request_redraw();
            },
            iced::Event::Window(iced::window::Event::Unfocused) => {
                self.term.backend.window_focus.set_focused(false);
                cleanup.extend(Self::cancel_pointer(state));
                state.keyboard_modifiers = Modifiers::empty();
            },
            iced::Event::Mouse(mouse::Event::CursorLeft) => {
                cleanup.extend(Self::cancel_pointer(state));
            },
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                modifiers,
            )) if !state.is_focused() => {
                // Wheel and Shift-selection target hover, independently of
                // keyboard focus. Track global modifiers without sending input.
                state.keyboard_modifiers = *modifiers;
            },
            _ => {},
        }

        let is_cursor_in_layout = self.is_cursor_in_layout(cursor, layout);
        let captured = Self::has_pointer_capture(state) || !cleanup.is_empty();
        if matches!(event, iced::Event::Mouse(_))
            && shell.is_event_captured()
            && !captured
        {
            return;
        }
        if matches!(
            event,
            iced::Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Left
            ))
        ) && !is_cursor_in_layout
        {
            cleanup.extend(Self::cancel_pointer(state));
        }
        self.handle_focus(event, state, is_cursor_in_layout);
        let focused =
            state.is_focused() && self.term.backend.window_focus.is_focused();
        if focused != self.term.backend.is_focused() {
            shell.publish(Event::BackendCall(
                self.term.id,
                Command::FocusChanged(focused),
            ));
        }

        let commands = match event {
            iced::Event::Mouse(mouse_event)
                if is_cursor_in_layout
                    || (captured
                        && matches!(
                            mouse_event,
                            mouse::Event::CursorMoved { .. }
                                | mouse::Event::ButtonReleased(_)
                        )) =>
            {
                self.handle_mouse_event(
                    state,
                    layout.position(),
                    cursor.position().unwrap_or_else(|| {
                        Self::fallback_cursor(state, layout.position())
                    }),
                    mouse_event,
                )
            },
            iced::Event::Keyboard(keyboard_event) => {
                if !state.is_focused() {
                    return;
                }

                self.handle_keyboard_event(
                    state,
                    clipboard,
                    keyboard_event,
                    shell,
                )
                .into_iter()
                .collect()
            },
            _ => Vec::new(),
        };

        if !commands.is_empty()
            || !cleanup.is_empty()
            || (is_cursor_in_layout
                && matches!(
                    event,
                    iced::Event::Mouse(mouse::Event::WheelScrolled { .. })
                ))
        {
            shell.capture_event();
        }

        for cmd in cleanup.into_iter().chain(commands) {
            shell.publish(Event::BackendCall(self.term.id, cmd));
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: iced_core::Layout<'_>,
        cursor: iced_core::mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> iced_core::mouse::Interaction {
        let state = tree.state.downcast_ref::<TerminalViewState>();
        let mut cursor_mode = iced_core::mouse::Interaction::Idle;
        let terminal_mode =
            self.term.backend.renderable_content().terminal_mode;
        if self.is_cursor_in_layout(cursor, layout)
            && (!terminal_mode.intersects(TermMode::MOUSE_MODE)
                || state.keyboard_modifiers.shift())
        {
            cursor_mode = iced_core::mouse::Interaction::Text;
        }

        if self.is_cursor_hovered_hyperlink(state) {
            cursor_mode = iced_core::mouse::Interaction::Pointer;
        }

        cursor_mode
    }
}

impl<'a> From<TerminalView<'a>> for Element<'a, Event, Theme, iced::Renderer> {
    fn from(widget: TerminalView<'a>) -> Self {
        Self::new(widget)
    }
}

#[derive(Debug, Clone)]
struct TerminalViewState {
    terminal_id: u64,
    glyphs: std::cell::RefCell<glyphs::Cache>,
    focus: bool,
    is_dragged: bool,
    last_click: Option<mouse::Click>,
    pointer: pointer::State,
    keyboard_modifiers: Modifiers,
    bounds: Option<Rectangle>,
    mouse_position_on_grid: TerminalGridPoint,
}

impl TerminalViewState {
    fn new(terminal_id: u64) -> Self {
        Self {
            terminal_id,
            glyphs: Default::default(),
            focus: false,
            is_dragged: false,
            last_click: None,
            pointer: pointer::State::default(),
            keyboard_modifiers: Modifiers::empty(),
            bounds: None,
            mouse_position_on_grid: TerminalGridPoint::default(),
        }
    }

    fn reset_pointer(&mut self) {
        self.is_dragged = false;
        self.last_click = None;
        self.pointer = pointer::State::default();
        self.mouse_position_on_grid = TerminalGridPoint::default();
    }
}

impl operation::Focusable for TerminalViewState {
    fn is_focused(&self) -> bool {
        self.focus
    }

    fn focus(&mut self) {
        self.focus = true;
    }

    fn unfocus(&mut self) {
        self.focus = false;
        self.reset_pointer();
        self.keyboard_modifiers = Modifiers::empty();
    }
}

#[derive(Default)]
struct BackgroundRect {
    display_offset: f32,
    cell_height: f32,
    is_active: bool,
    color: Color,
    start_x: f32,
    width: f32,
}

impl BackgroundRect {
    fn with_display_offset(mut self, value: f32) -> Self {
        self.display_offset = value;
        self
    }

    fn with_cell_height(mut self, value: f32) -> Self {
        self.cell_height = value;
        self
    }

    fn with_width(mut self, value: f32) -> Self {
        self.width = value;
        self
    }

    fn with_start_x(mut self, value: f32) -> Self {
        self.start_x = value;
        self
    }

    fn with_color(mut self, value: Color) -> Self {
        self.color = value;
        self
    }

    fn activate(mut self) -> Self {
        self.is_active = true;
        self
    }

    fn build(&self, line: i32) -> Path {
        let flush_y = (line as f32 + self.display_offset) * self.cell_height;
        Path::rectangle(
            Point::new(self.start_x, flush_y),
            Size::new(self.width, self.cell_height),
        )
    }

    fn can_flush(&self) -> bool {
        self.is_active && self.width > 0.0
    }

    fn can_extend(&self, bg: Color, x: f32) -> bool {
        self.is_active
            && bg == self.color
            && (self.start_x + self.width - x).abs() < f32::EPSILON
    }

    fn extend(&mut self, value: f32) {
        self.width += value;
    }
}

#[cfg(test)]
mod state_tests;

#[cfg(test)]
mod pointer_tests;
