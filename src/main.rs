#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pike::app::App;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .executor::<pike::executor::SharedExecutor>()
        .subscription(App::subscription)
        .theme(App::theme)
        .title("Pike")
        .window_size((960.0, 640.0))
        .exit_on_close_request(false)
        .run()
}
