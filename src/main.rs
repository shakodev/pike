#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::convert::Infallible;

use iced::widget::{column, container, text};
use iced::{Element, Fill};

fn main( )->iced::Result{
    iced::application(|| (), update, view)
        .title("Pike")
        .window_size((960.0, 640.0))
        .run()
}

fn update(_state: &mut (), message: Infallible) {
    match message {}
}

fn view(_state: &()) -> Element<'_, Infallible> {
    container(column![
        text("Pike").size(32),
        text("Windows build baseline. Terminal sessions are coming next."),
    ])
    .center_x(Fill)
    .center_y(Fill)
    .into()
}
