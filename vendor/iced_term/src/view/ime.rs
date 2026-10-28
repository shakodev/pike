use super::*;
use iced::advanced::Layout;
use iced_core::input_method::{self, InputMethod, Purpose};

impl TerminalView<'_> {
    fn ime_eligible(&self, state: &TerminalViewState) -> bool {
        state.is_focused()
            && self.term.backend.window_focus.is_focused()
            && self.term.is_visible()
            && !self.term.input_status().closed
            && self.term.backend.ime.enabled()
    }

    pub(super) fn update_ime(
        &self,
        state: &TerminalViewState,
        event: &iced_core::Event,
        layout: Layout<'_>,
        viewport: &Rectangle,
        shell: &mut iced_core::Shell<'_, Event>,
    ) -> Option<Command> {
        let session = &self.term.backend.ime;
        if !self.ime_eligible(state) {
            self.term.backend.window_focus.release_ime(session);
            return None;
        }
        self.term.backend.window_focus.claim_ime(session);
        if matches!(
            event,
            iced::Event::Window(iced::window::Event::RedrawRequested(_))
        ) {
            let content = self.term.backend.renderable_content();
            let cursor = anchor(content, layout.bounds(), *viewport);
            if self.term.backend.window_focus.reset_ime_on_redraw() {
                // Force a native disable boundary before a new owner is enabled.
                // Exactly one follow-up paint, never a timer or idle redraw loop.
                session.requested(false);
                shell.request_redraw();
            } else if let Some(cursor) = cursor {
                session.requested(true);
                let mut preedit = session.preedit();
                if let Some(preedit) = &mut preedit {
                    preedit.text_size =
                        Some(iced_core::Pixels(self.term.font.size));
                }
                shell.request_input_method(&InputMethod::Enabled {
                    cursor,
                    purpose: Purpose::Terminal,
                    preedit: preedit
                        .as_ref()
                        .map(input_method::Preedit::as_ref),
                });
            } else {
                session.cancel();
            }
        }
        if let iced::Event::InputMethod(event) = event {
            shell.capture_event();
            shell.request_redraw();
            return match session.event(event) {
                Ok(commit) => commit.map(Command::ImeCommit),
                Err(error) => Some(Command::ImeRejected(error)),
            };
        }
        None
    }
}

/// Iced forwards logical coordinates and winit applies current monitor DPI.
/// Keep the anchor within pane/ancestor clips even while scrolled above input.
fn anchor(
    content: &RenderableContent,
    bounds: Rectangle,
    viewport: Rectangle,
) -> Option<Rectangle> {
    let visible = bounds.intersection(&viewport)?;
    if visible.width <= 0.0 || visible.height <= 0.0 {
        return None;
    }
    let cell = content.terminal_size;
    let span = if content
        .cursor
        .flags
        .intersects(cell::Flags::WIDE_CHAR | cell::Flags::WIDE_CHAR_SPACER)
    {
        2.0
    } else {
        1.0
    };
    let width = (cell.cell_width * span).min(visible.width);
    let height = cell.cell_height.min(visible.height);
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
    {
        return None;
    }
    let point = content.cursor_point;
    let column = point.column.0.saturating_sub(usize::from(
        content.cursor.flags.contains(cell::Flags::WIDE_CHAR_SPACER),
    ));
    let row = point.line.0 as f32 + content.viewport.display_offset() as f32;
    Some(Rectangle::new(
        Point::new(
            (bounds.x + column as f32 * cell.cell_width)
                .clamp(visible.x, visible.x + visible.width - width),
            (bounds.y + row * cell.cell_height)
                .clamp(visible.y, visible.y + visible.height - height),
        ),
        Size::new(width, height),
    ))
}

#[cfg(test)]
mod tests;
