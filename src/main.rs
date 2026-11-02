#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pike::app::App;
use std::process::ExitCode;

fn main() -> ExitCode {
    let diagnostics = std::env::args_os().any(|argument| argument == "--diagnostics");
    let result = iced::application(
        move || {
            let (mut app, task) = App::boot();
            app.set_diagnostics_enabled(diagnostics);
            (app, task)
        },
        App::update,
        App::view,
    )
    .executor::<pike::executor::SharedExecutor>()
    .subscription(App::subscription)
    .theme(App::theme)
    .title("Pike")
    .window_size((960.0, 640.0))
    .exit_on_close_request(false)
    .run();
    if result.is_err() {
        pike::startup_report::renderer_failure();
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
