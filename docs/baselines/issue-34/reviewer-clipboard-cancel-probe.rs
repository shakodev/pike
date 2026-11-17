//! Independent real-App/PTY check of retry state after an owned read is revoked.
use super::*;

#[test]
fn revoked_clipboard_reply_keeps_fresh_recovery_available_without_writing() {
    let executor = SharedExecutor::new().unwrap();
    let (mut app, owner, other) = pair(&executor);
    app.report_error(
        Owner::Session(owner.session),
        Error::Clipboard(iced_term::PasteError::ClipboardUnavailable),
    );
    let notice = app.latest_error(Owner::Session(owner.session)).unwrap();
    let request = app.recovery_request(notice.id).unwrap();
    let task = app.update(Message::ErrorRetry(request));
    let mut actions = iced_runtime::task::into_stream(task).unwrap();
    let native_reply = executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(action) = actions.next().await {
                if let iced_runtime::Action::Clipboard(
                    iced_runtime::clipboard::Action::Read { channel, .. }
                ) = action {
                    return channel;
                }
            }
            panic!("missing existing native clipboard reader")
        }).await.unwrap()
    });
    assert!(app.latest_error(Owner::Session(owner.session)).unwrap().retrying);
    let before = app.terminal(owner.session).unwrap().input_status();
    drop(app.select(app.model.target(other.session).unwrap()));
    drop(app.select(app.model.target(owner.session).unwrap()));
    native_reply.send(Some("SYNTHETIC_REVOKED_PASTE34".into())).unwrap();
    executor.block_on(async {
        tokio::time::timeout(DEADLINE, async {
            while let Some(action) = actions.next().await {
                if let iced_runtime::Action::Output(message @ Message::CommandPaste(..)) = action {
                    drop(app.update(message));
                    return;
                }
            }
            panic!("missing real clipboard task reply")
        }).await.unwrap()
    });
    let after = app.terminal(owner.session).unwrap().input_status();
    let current_notice = app.latest_error(Owner::Session(owner.session)).unwrap();
    let still_retrying = current_notice.retrying;
    // A correction may rotate ErrorId to revoke older controls. Fresh recovery
    // must use whichever notice is current after cancellation.
    let fresh_recovery_available = app.recovery_request(current_notice.id).is_some();
    let unchanged_input = before.written_bytes + before.outstanding_bytes + before.cancelled_bytes
        == after.written_bytes + after.outstanding_bytes + after.cancelled_bytes;
    eprintln!("owner_roundtrip=true stale_reply_wrote={} retrying_after_reply={still_retrying} fresh_recovery_available={fresh_recovery_available}", !unchanged_input);
    // Always account for both actual owned native PTYs before the RED assertion.
    shutdown(&mut app, &executor);
    assert!(unchanged_input, "revoked paste must never write");
    assert!(!still_retrying && fresh_recovery_available,
        "revoking a clipboard reply must end its pending recovery and allow a fresh explicit attempt");
}
