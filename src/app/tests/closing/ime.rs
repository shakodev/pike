//! Hold real public IME tokens across modal focus changes and window teardown.
use super::*;
use crate::app::tests::ime::request;
use iced::advanced::renderer::Headless;
use iced_term::actions::Action;

fn admitted(terminal: &Terminal) -> usize {
    let input = terminal.input_status();
    input.written_bytes + input.outstanding_bytes + input.cancelled_bytes
}

#[test]
#[ignore = "requires native graphics; explicit modal/teardown deferred-IME regression"]
fn modal_and_teardown_cancel_all_held_ime_clones_after_focus_is_restored() {
    let executor = SharedExecutor::new().unwrap();
    let renderer = executor
        .block_on(<iced::Renderer as Headless>::new(
            iced::Font::MONOSPACE,
            iced::Pixels(14.0),
            None,
        ))
        .expect("headless renderer");
    for window in [false, true] {
        let mut app = App {
            profile: waiting(),
            ..App::default()
        };
        drop(app.update(Message::WindowFocused(true)));
        let (ids, ready) = launch(&mut app, &executor);
        drop(app.launched(ids, Ok(ready)));
        let target = app.active_target().unwrap();
        let prior = request(app.terminal(ids.session).unwrap(), &renderer);
        drop(app.update(if window {
            Message::CloseWindow
        } else {
            Message::Control(Control::CloseSession(ids.session))
        }));
        let (decision, count) = app.close_confirmation().unwrap();
        assert_eq!(count, 1);
        assert!(app.interaction_blocked());
        // Forge stale widget/backend focus while the host modal owns input.
        // The host rejection must cancel every clone, not merely defer sending.
        let Owner::Attached(terminal) =
            &mut app.sessions.entries.get_mut(&ids.session).unwrap().owner
        else {
            unreachable!()
        };
        terminal.set_visible(true);
        terminal.set_focused(true);
        let queued = request(terminal, &renderer);
        let held = queued.clone();
        drop(app.update(Message::Terminal((
            ids.session,
            Event::BackendCall(ids.session.get(), B::ImeCommit(queued)),
        ))));
        if window {
            confirm_close(&mut app);
        } else {
            // Queued layout/input controls cannot steal modal restoration.
            drop(app.update(Message::Control(Control::Focus(target))));
            drop(
                app.update(Message::Control(Control::CloseAction(CloseAction::Cancel(
                    decision,
                )))),
            );
            assert_eq!(app.active_target(), Some(target));
            assert!(!app.interaction_blocked());
            assert!(
                app.terminal(ids.session)
                    .unwrap()
                    .renderable_content()
                    .unwrap()
                    .focused
            );
        }
        let Owner::Attached(terminal) =
            &mut app.sessions.entries.get_mut(&ids.session).unwrap().owner
        else {
            unreachable!()
        };
        for commit in [prior, held.clone(), held] {
            assert_eq!(
                terminal.handle(iced_term::Command::ProxyToBackend(B::ImeCommit(commit))),
                Action::Ignore
            );
        }
        assert_eq!(admitted(terminal), 0);
        if !window {
            let fresh = request(app.terminal(ids.session).unwrap(), &renderer);
            for commit in [fresh.clone(), fresh] {
                drop(app.update(Message::Terminal((
                    ids.session,
                    Event::BackendCall(ids.session.get(), B::ImeCommit(commit)),
                ))));
            }
            assert_eq!(admitted(app.terminal(ids.session).unwrap()), "你好".len());
            // Complete the controlled line and await native cleanup rather than
            // triggering the known Unix partial-line shutdown limitation.
            drop(app.update(Message::Terminal((
                ids.session,
                Event::BackendCall(ids.session.get(), B::Write(b"\n".to_vec())),
            ))));
            let handle = app.terminal(ids.session).unwrap().shutdown_handle();
            executor.block_on(async {
                tokio::time::timeout(DEADLINE, handle.wait()).await.unwrap();
            });
        }
        shutdown(&mut app, &executor);
        assert!(app.launches.shutdown_complete());
    }
}
