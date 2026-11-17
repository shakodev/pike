//! Cancellation completes its own attempt without weakening input admission.
use super::*;
use iced::futures::{channel::oneshot, stream::BoxStream};

fn held_read(
    executor: &SharedExecutor,
    task: Task<Message>,
) -> (
    BoxStream<'static, iced_runtime::Action<Message>>,
    oneshot::Sender<Option<String>>,
) {
    let mut actions = iced_runtime::task::into_stream(task).unwrap();
    let channel = executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(action) = actions.next().await {
                if let iced_runtime::Action::Clipboard(iced_runtime::clipboard::Action::Read {
                    channel,
                    ..
                }) = action
                {
                    return channel;
                }
            }
            panic!("native read")
        })
        .await
        .unwrap()
    });
    (actions, channel)
}

fn consume(
    executor: &SharedExecutor,
    app: &mut App,
    actions: &mut BoxStream<'static, iced_runtime::Action<Message>>,
) -> commands::PasteRequest {
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(action) = actions.next().await {
                if let iced_runtime::Action::Output(Message::CommandPaste(request, text)) = action {
                    let replay = request.clone();
                    drop(app.update(Message::CommandPaste(request, text)));
                    return replay;
                }
            }
            panic!("read reply")
        })
        .await
        .unwrap()
    })
}

#[test]
fn native_and_modal_revocation_allow_fresh_retry_and_old_reply_cannot_end_it() {
    for native in [true, false] {
        let executor = SharedExecutor::new().unwrap();
        let (mut app, owner, other) = pair(&executor);
        app.set_diagnostics_enabled(true);
        app.report_error(
            Owner::Session(owner.session),
            Error::Clipboard(iced_term::PasteError::ClipboardUnavailable),
        );
        let old = app.latest_error(Owner::Session(owner.session)).unwrap();
        let request = app.recovery_request(old.id).unwrap();
        let task = app.update(Message::ErrorRetry(request));
        let (mut actions, reply) = held_read(&executor, task);
        let before = app.terminal(owner.session).unwrap().input_status();
        if native {
            drop(app.update(Message::WindowFocused(false)));
            drop(app.update(Message::WindowFocused(true)));
        } else {
            drop(app.update(Message::Control(Control::Command(commands::Command::Help))));
            drop(app.update(Message::Control(Control::DismissHelp)));
        }
        reply.send(Some("SECRET_CANCELLED_PASTE34".into())).unwrap();
        let replay = consume(&executor, &mut app, &mut actions);
        assert_eq!(app.terminal(owner.session).unwrap().input_status(), before);
        let current = app.latest_error(Owner::Session(owner.session)).unwrap();
        assert_ne!(current.id, old.id);
        assert!(!current.retrying);
        let fresh = app.recovery_request(current.id).unwrap();
        let task = app.update(Message::ErrorRetry(fresh));
        let (mut fresh_actions, fresh_reply) = held_read(&executor, task);
        assert!(
            app.latest_error(Owner::Session(owner.session))
                .unwrap()
                .retrying
        );
        assert_eq!(
            app.update(Message::CommandPaste(
                replay,
                Some("SECRET_OLD_REPLY34".into())
            ))
            .units(),
            0
        );
        assert!(
            app.latest_error(Owner::Session(owner.session))
                .unwrap()
                .retrying
        );
        assert_eq!(
            app.latest_error(Owner::Session(owner.session)).unwrap().id,
            current.id
        );
        fresh_reply.send(Some("z".into())).unwrap();
        consume(&executor, &mut app, &mut fresh_actions);
        let after = app.terminal(owner.session).unwrap().input_status();
        assert_eq!(
            after.written_bytes + after.outstanding_bytes + after.cancelled_bytes,
            before.written_bytes + before.outstanding_bytes + before.cancelled_bytes + 1
        );
        assert!(app.latest_error(Owner::Session(owner.session)).is_none());
        assert_eq!(app.active_session(), Some(owner.session));
        assert_eq!(
            app.model.sessions().get(other.session).unwrap().lifecycle(),
            Lifecycle::Running
        );
        let capture = app.diagnostic_capture();
        assert!(!capture.contains("SECRET_CANCELLED_PASTE34"));
        assert!(!capture.contains("SECRET_OLD_REPLY34"));
        shutdown(&mut app, &executor);
    }
}
