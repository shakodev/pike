use super::*;

fn opened() -> Arc<Session> {
    let session = Arc::new(Session::default());
    session.requested(true);
    assert!(session.event(&Event::Opened).unwrap().is_none());
    session
}

#[test]
fn ime_preedit_is_local_and_commit_is_utf8_once_even_after_native_closed() {
    let session = opened();
    for event in [
        Event::Preedit("ni".into(), Some(0..2)),
        Event::Preedit(String::new(), None),
    ] {
        assert!(session.event(&event).unwrap().is_none());
        assert!(session.composing());
    }
    let commit = session
        .event(&Event::Commit("你好e\u{301}".into()))
        .unwrap()
        .unwrap();
    assert!(!format!("{commit:?}").contains("你好"));
    session.event(&Event::Closed).unwrap();
    assert!(!session.composing());
    assert_eq!(
        commit.take(&session).unwrap().as_deref(),
        Some("你好e\u{301}")
    );
    assert!(commit.take(&session).unwrap().is_none());
    assert!(session
        .event(&Event::Commit("late".into()))
        .unwrap()
        .is_none());
}

#[test]
fn ime_preview_bound_and_invalid_byte_ranges_never_make_a_commit() {
    let session = opened();
    assert!(session
        .event(&Event::Preedit("你".into(), Some(1..2)))
        .unwrap()
        .is_none());
    assert_eq!(session.preedit().unwrap().selection, None);
    assert_eq!(
        session
            .event(&Event::Preedit("x".repeat(PREEDIT_BYTE_LIMIT + 1), None))
            .unwrap_err(),
        ImeError::PreeditTooLarge {
            bytes: PREEDIT_BYTE_LIMIT + 1
        }
    );
    assert!(session.preedit().is_none());
    assert!(session.lock().pending.is_empty());
}

#[test]
fn ime_cancel_owner_change_and_modal_suspend_invalidate_all_deferred_clones() {
    let a = opened();
    let commit = a.event(&Event::Commit("secret".into())).unwrap().unwrap();
    let replay = commit.clone();
    a.cancel();
    assert!(commit.take(&a).unwrap().is_none());
    assert!(replay.take(&a).unwrap().is_none());
    a.requested(true);
    assert!(a
        .event(&Event::Commit("old owner".into()))
        .unwrap()
        .is_none());
    a.event(&Event::Opened).unwrap();
    let wrong = a.event(&Event::Commit("private".into())).unwrap().unwrap();
    let b = opened();
    assert_eq!(wrong.take(&b).unwrap_err(), ImeError::WrongSession);
    assert!(wrong.take(&a).unwrap().is_none());
    a.set_enabled(false);
    assert!(!a.composing());
    a.requested(true);
    assert!(a.event(&Event::Opened).unwrap().is_none());
    assert!(!a.composing());
    a.set_enabled(true);
    a.requested(true);
    assert!(a.event(&Event::Commit("stale".into())).unwrap().is_none());
}

#[test]
fn ime_pending_bytes_count_and_last_clone_drop_are_bounded() {
    let session = opened();
    let first = session
        .event(&Event::Commit("x".repeat(INPUT_BYTE_LIMIT)))
        .unwrap()
        .unwrap();
    assert!(matches!(
        session.event(&Event::Commit("y".into())),
        Err(ImeError::CommitQueueFull { available: 0, .. })
    ));
    let clone = first.clone();
    drop(first);
    assert!(session.event(&Event::Commit("y".into())).is_err());
    drop(clone);
    let mut commits = Vec::new();
    for _ in 0..PENDING_COMMIT_LIMIT {
        commits
            .push(session.event(&Event::Commit("界".into())).unwrap().unwrap());
    }
    assert!(session.event(&Event::Commit("y".into())).is_err());
    for commit in &commits {
        assert_eq!(commit.take(&session).unwrap().as_deref(), Some("界"));
    }
    assert!(session
        .event(&Event::Commit("next".into()))
        .unwrap()
        .is_some());
    session.cancel();
    assert!(session.lock().pending.is_empty());
}

#[test]
fn ime_window_owner_requires_disabled_boundary_and_fresh_opened() {
    let a = opened();
    let b = Arc::new(Session::default());
    let mut window = Window::default();
    window.claim(&a);
    assert!(!window.disable_once());
    a.event(&Event::Preedit("secret".into(), None)).unwrap();
    window.claim(&b);
    assert!(!a.composing());
    assert!(a.preedit().is_none());
    assert!(window.disable_once());
    assert!(!window.disable_once());
    assert!(b.event(&Event::Opened).unwrap().is_none()); // not yet requested
    assert!(!b.composing());
    b.requested(true);
    assert!(b.event(&Event::Commit("late A".into())).unwrap().is_none());
    b.event(&Event::Opened).unwrap();
    let commit = b.event(&Event::Commit("B".into())).unwrap().unwrap();
    window.release(&b);
    assert!(commit.take(&b).unwrap().is_none());
    assert!(window.disable_once());
}

#[test]
fn ime_expired_window_owner_still_forces_native_reset() {
    let mut window = Window::default();
    let a = opened();
    window.claim(&a);
    drop(a);
    let b = opened();
    window.claim(&b);
    assert!(window.disable_once());
    assert!(!window.disable_once());
}
