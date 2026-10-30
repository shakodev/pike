use iced::widget::{button, column, container, row, text};
use iced::{Element, Fill};

use super::{App, Control, Direction, Message, close, split::GUTTER};

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        if let Some(modal) = self.close_view() {
            return modal;
        }
        if let Some(help) = self.help_view() {
            return help;
        }
        if let Some(dialog) = self.tab_dialog_view() {
            return dialog;
        }
        let mut controls = row![text("Pike").size(22)].spacing(12);
        if !self.exiting {
            if let Some(target) = self.active_target() {
                controls = controls
                    .push(button("Split right").on_press(Control::Split(target, Direction::Right)))
                    .push(button("Split down").on_press(Control::Split(target, Direction::Down)))
                    .push(button("New tab").on_press(Control::NewTab));
                if let Some(id) = self.active_session() {
                    controls = controls.push(close::close_button(id));
                }
                controls = controls.push(close::close_tab_button(target.tab));
                if let Some(id) = self.active_session()
                    && self.terminal(id).is_some()
                {
                    controls = controls.push(button("Copy selection").on_press(Control::Copy(id)));
                }
            } else {
                controls = controls.push(button("New terminal").on_press(Control::New));
            }
        }
        let pane: Element<'_, Message> = if self.exiting {
            text("Closing terminals…").into()
        } else if let Some(tab_id) = self.model.workspace().active() {
            self.pane_grid(tab_id)
        } else {
            container(text("No terminal open"))
                .width(Fill)
                .height(Fill)
                .into()
        };
        let constrained = self
            .grid_size
            .and_then(|(tab, area)| {
                let geometry = self.resize_geometry(tab, area)?;
                let node = self.model.workspace().tab(tab)?.grid().layout();
                Some(geometry.canvas(node).is_none_or(|canvas| {
                    canvas.width > area.width
                        || canvas.height > area.height
                        || node
                            .pane_regions(GUTTER, 0.0, canvas)
                            .iter()
                            .any(|(pane, r)| {
                                let minimum = geometry.minimum[pane];
                                r.width < minimum.width || r.height < minimum.height
                            })
                }))
            })
            .unwrap_or(false);
        controls = controls.push(
            text(if constrained {
                "Size constrained · 20×5 minimum · drag scrollbars to pan"
            } else {
                "Resize: Alt+Shift+Arrow · 20×5 minimum"
            })
            .size(12),
        );
        let controls: Element<'_, Control> = controls.wrap().into();
        let mut content = column![controls.map(Message::Control)].spacing(8);
        if !self.exiting && !self.model.workspace().order().is_empty() {
            content = content.push(self.tabs_view().map(Message::Control));
        }
        let mut content = content.push(pane);
        if let Some(notice) = self.notice {
            content = content.push(text(notice));
        }
        super::resize::capture(
            container(content).padding(8).width(Fill).height(Fill),
            self.interaction_blocked(),
            self.active_session().and_then(|id| self.terminal(id)),
            super::commands::on_key,
        )
        .map(move |message| match message {
            Message::Control(Control::Command(command)) => {
                Message::Control(Control::KeyCommand(self.active_session(), command))
            }
            message => message,
        })
    }
}
