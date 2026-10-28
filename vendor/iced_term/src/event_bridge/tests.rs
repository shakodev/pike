use super::*;
use iced::futures::FutureExt;
use std::sync::mpsc;
use std::time::Duration;

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

fn next(receiver: &Receiver) -> Delivery {
    receiver
        .recv()
        .now_or_never()
        .expect("delivery must be ready")
        .expect("delivery")
}

fn finish(
    sender: Sender,
    receiver: &Receiver,
    expected_error: Option<EventBridgeError>,
) {
    drop(sender);
    if let Some(error) = expected_error {
        assert!(
            matches!(next(receiver), Delivery::Bridge(EventBridgeEvent::Error(e)) if e == error)
        );
    }
    assert!(matches!(
        next(receiver),
        Delivery::Bridge(EventBridgeEvent::Completed)
    ));
    for _ in 0..10 {
        assert!(receiver.recv().now_or_never().unwrap().is_none());
    }
}

#[test]
fn dirty_title_and_bell_floods_coalesce_without_control_capacity() {
    let (sender, receiver) = channel();
    for i in 0..10_000 {
        sender.send(Event::Wakeup);
        sender.send(Event::MouseCursorDirty);
        sender.send(Event::CursorBlinkingChange);
        sender.send(Event::Bell);
        sender.send(Event::Title(i.to_string()));
    }
    assert_eq!(receiver.0.lock().controls.len(), 0);
    assert!(
        matches!(next(&receiver), Delivery::Terminal(Event::Title(s)) if s == "9999")
    );
    assert!(matches!(next(&receiver), Delivery::Terminal(Event::Wakeup)));
    assert!(matches!(
        next(&receiver),
        Delivery::Terminal(Event::MouseCursorDirty)
    ));
    assert!(matches!(
        next(&receiver),
        Delivery::Terminal(Event::CursorBlinkingChange)
    ));
    assert!(matches!(next(&receiver), Delivery::Terminal(Event::Bell)));
    assert!(receiver.recv().now_or_never().is_none());
    sender.send(Event::Title("discarded".into()));
    sender.send(Event::ResetTitle);
    assert!(matches!(
        next(&receiver),
        Delivery::Terminal(Event::ResetTitle)
    ));
    sender.send(Event::Exit);
    assert!(matches!(next(&receiver), Delivery::Terminal(Event::Exit)));
    finish(sender, &receiver, None);
}

#[test]
fn disabled_clipboard_flood_never_retains_payload_or_starves_protocol_queries()
{
    let (sender, receiver) = channel();
    for _ in 0..10_000 {
        sender.send(Event::ClipboardStore(
            alacritty_terminal::term::ClipboardType::Clipboard,
            "x".repeat(1000),
        ));
        sender.send(Event::ClipboardLoad(
            alacritty_terminal::term::ClipboardType::Clipboard,
            Arc::new(|_| panic!("disabled")),
        ));
        sender.send(Event::Title("safe\n\u{202e}title".into()));
        sender.send(Event::Bell);
    }
    sender.send(Event::TextAreaSizeRequest(Arc::new(|_| "size".into())));
    let state = receiver.0.lock();
    assert_eq!(state.controls.len(), 1);
    assert_eq!(state.payload_bytes, 0);
    assert!(state.error.is_none());
    drop(state);
    assert!(matches!(
        next(&receiver),
        Delivery::Terminal(Event::TextAreaSizeRequest(_))
    ));
    assert!(
        matches!(next(&receiver), Delivery::Terminal(Event::Title(title)) if title == "safetitle")
    );
    assert!(matches!(next(&receiver), Delivery::Terminal(Event::Bell)));
}

#[test]
fn title_memory_is_bounded_on_a_utf8_boundary() {
    let (sender, receiver) = channel();
    sender.send(Event::Title("🦀".repeat(TITLE_LIMIT)));
    let Delivery::Terminal(Event::Title(title)) = next(&receiver) else {
        panic!("title")
    };
    assert_eq!(title.len(), TITLE_LIMIT);
    assert_eq!(title.capacity(), TITLE_LIMIT);
    sender.send(Event::Title(format!("a{}", "🦀".repeat(TITLE_LIMIT))));
    let Delivery::Terminal(Event::Title(title)) = next(&receiver) else {
        panic!("title")
    };
    assert!(title.len() < TITLE_LIMIT);
    assert!(title.ends_with('🦀'));
}

#[test]
fn ordered_controls_survive_saturation_with_reliable_lifecycle() {
    let (sender, receiver) = channel();
    for i in 0..CONTROL_LIMIT {
        sender.send(Event::PtyWrite(i.to_string()));
    }
    for _ in 0..10_000 {
        sender.send(Event::PtyWrite("rejected".into()));
        sender.send(Event::Wakeup);
    }
    sender.send(Event::ChildExit(37));
    sender.send(Event::Exit);
    let state = receiver.0.lock();
    assert_eq!(state.controls.len(), CONTROL_LIMIT);
    assert!(state.payload_bytes <= PAYLOAD_LIMIT);
    drop(state);
    for i in 0..CONTROL_LIMIT {
        assert!(
            matches!(next(&receiver), Delivery::Terminal(Event::PtyWrite(s)) if s == i.to_string())
        );
    }
    assert_eq!(receiver.0.lock().payload_bytes, 0);
    assert!(matches!(
        next(&receiver),
        Delivery::Terminal(Event::ChildExit(37))
    ));
    assert!(matches!(
        next(&receiver),
        Delivery::Bridge(EventBridgeEvent::Error(
            EventBridgeError::ControlOverflow
        ))
    ));
    assert!(matches!(next(&receiver), Delivery::Terminal(Event::Exit)));
    finish(sender, &receiver, None);
}

#[test]
fn payload_budget_counts_string_capacity_and_resets_on_dequeue() {
    let (sender, receiver) = channel();
    sender.send(Event::PtyWrite("x".repeat(PAYLOAD_LIMIT)));
    assert_eq!(receiver.0.lock().payload_bytes, PAYLOAD_LIMIT);
    assert!(matches!(
        next(&receiver),
        Delivery::Terminal(Event::PtyWrite(_))
    ));
    assert_eq!(receiver.0.lock().payload_bytes, 0);
    let mut text = String::with_capacity(PAYLOAD_LIMIT + 1);
    text.push('x');
    sender.send(Event::PtyWrite(text));
    assert!(matches!(
        next(&receiver),
        Delivery::Bridge(EventBridgeEvent::Error(
            EventBridgeError::PayloadOverflow
        ))
    ));
    assert_eq!(receiver.0.lock().payload_bytes, 0);
    // Failure remains sticky after its notification has been consumed.
    sender.send(Event::PtyWrite("never admitted".into()));
    assert!(receiver.recv().now_or_never().is_none());
    finish(sender, &receiver, None);
}

#[test]
fn aggregate_payload_overflow_is_explicit() {
    let (sender, receiver) = channel();
    sender.send(Event::PtyWrite("x".repeat(PAYLOAD_LIMIT / 2)));
    sender.send(Event::PtyWrite("x".repeat(PAYLOAD_LIMIT / 2)));
    sender.send(Event::PtyWrite("x".into()));
    assert_eq!(receiver.0.lock().controls.len(), 2);
    for _ in 0..2 {
        next(&receiver);
    }
    assert!(matches!(
        next(&receiver),
        Delivery::Bridge(EventBridgeEvent::Error(
            EventBridgeError::PayloadOverflow
        ))
    ));
}

#[test]
fn requests_with_formatters_remain_in_protocol_order() {
    let (sender, receiver) = channel();
    sender.send(Event::ColorRequest(7, Arc::new(|_| "color".into())));
    sender.send(Event::PtyWrite("reply".into()));
    sender.send(Event::TextAreaSizeRequest(Arc::new(|_| "size".into())));
    sender.send(Event::ClipboardLoad(
        alacritty_terminal::term::ClipboardType::Clipboard,
        Arc::new(|_| "clipboard".into()),
    ));
    assert!(matches!(
        next(&receiver),
        Delivery::Terminal(Event::ColorRequest(7, _))
    ));
    assert!(
        matches!(next(&receiver), Delivery::Terminal(Event::PtyWrite(s)) if s == "reply")
    );
    assert!(matches!(
        next(&receiver),
        Delivery::Terminal(Event::TextAreaSizeRequest(_))
    ));
    assert!(receiver.recv().now_or_never().is_none());
}

#[test]
fn eof_before_exit_reports_error_and_after_exit_ends_cleanly() {
    let (sender, receiver) = channel();
    finish(sender, &receiver, Some(EventBridgeError::BackendStopped));
    let (sender, receiver) = channel();
    sender.send(Event::Exit);
    assert!(matches!(next(&receiver), Delivery::Terminal(Event::Exit)));
    finish(sender, &receiver, None);
}

#[test]
fn loop_proxy_drop_completes_even_with_a_retained_terminal_proxy() {
    let (sender, receiver) = channel();
    let loop_sender = sender.clone().complete_on_drop();
    // Clones must not inherit the event-loop completion marker.
    drop(loop_sender.clone());
    assert!(!receiver.0.lock().completed);
    drop(loop_sender);
    assert!(matches!(
        next(&receiver),
        Delivery::Bridge(EventBridgeEvent::Error(
            EventBridgeError::BackendStopped
        ))
    ));
    assert!(matches!(
        next(&receiver),
        Delivery::Bridge(EventBridgeEvent::Completed)
    ));
    sender.send(Event::Wakeup);
    assert!(receiver.recv().now_or_never().unwrap().is_none());
}

#[test]
fn requested_close_completes_cleanly_but_preserves_an_existing_overload() {
    let (sender, receiver) = channel();
    sender.expect_close();
    finish(sender, &receiver, None);
    let (sender, receiver) = channel();
    sender.send(Event::PtyWrite("x".repeat(PAYLOAD_LIMIT + 1)));
    sender.expect_close();
    finish(sender, &receiver, Some(EventBridgeError::PayloadOverflow));
}

#[test]
fn canceled_receive_and_closed_receiver_do_not_panic_or_stall() {
    let (sender, receiver) = channel();
    assert!(receiver.recv().now_or_never().is_none());
    sender.send(Event::Wakeup);
    assert!(matches!(next(&receiver), Delivery::Terminal(Event::Wakeup)));
    sender.send(Event::PtyWrite("queued".into()));
    let shared = receiver.0.clone();
    drop(receiver);
    for _ in 0..10_000 {
        sender.send(Event::PtyWrite("ignored".into()));
    }
    assert!(shared.lock().receiver_closed);
    assert!(shared.lock().controls.is_empty());
}

#[test]
fn stalled_ui_cannot_block_a_producer_holding_the_terminal_lock() {
    use alacritty_terminal::{
        event::EventListener,
        sync::FairMutex,
        term::{self, test::TermSize, Term},
    };
    let (sender, receiver) = channel();
    let proxy = crate::backend::EventProxy(sender.clone());
    let term = Arc::new(FairMutex::new(Term::new(
        term::Config::default(),
        &TermSize::new(80, 24),
        proxy.clone(),
    )));
    let (done, observed) = mpsc::channel();
    let task = std::thread::spawn(move || {
        let _guard = term.lock();
        for _ in 0..10_000 {
            proxy.send_event(Event::PtyWrite("reply".into()));
        }
        proxy.send_event(Event::Exit);
        done.send(()).unwrap();
    });
    // No consumer polls while the producer saturates every queue.
    observed.recv_timeout(Duration::from_secs(5)).unwrap();
    task.join().unwrap();
    assert_eq!(receiver.0.lock().controls.len(), CONTROL_LIMIT);
    // Another session is independent while the first remains overloaded.
    let (other, active) = channel();
    other.send(Event::PtyWrite("responsive".into()));
    assert!(
        matches!(block_on(active.recv()), Some(Delivery::Terminal(Event::PtyWrite(s))) if s == "responsive")
    );
}

#[test]
fn notifications_wake_a_waiting_consumer_without_a_timer() {
    let (sender, receiver) = channel();
    let (done, observed) = mpsc::channel();
    let task = std::thread::spawn(move || {
        assert!(matches!(
            block_on(receiver.recv()),
            Some(Delivery::Terminal(Event::Wakeup))
        ));
        done.send(()).unwrap();
    });
    sender.send(Event::Wakeup);
    observed.recv_timeout(Duration::from_secs(5)).unwrap();
    task.join().unwrap();
}

#[test]
fn stream_end_waits_for_distinct_sticky_backend_completion() {
    let (sender, receiver) = channel();
    sender.expect_backend();
    sender.expect_close();
    let source = sender.clone().complete_on_drop();
    drop(source);
    assert!(matches!(
        receiver.recv().now_or_never(),
        Some(Some(Delivery::Bridge(EventBridgeEvent::Completed)))
    ));
    assert!(receiver.recv().now_or_never().is_none());
    let completion = crate::BackendCompletion {
        reason: crate::ExitReason::Requested,
        exit_code: None,
        output_drained: false,
    };
    sender.backend_completed(completion);
    sender.backend_completed(completion);
    assert!(
        matches!(receiver.recv().now_or_never(), Some(Some(Delivery::Bridge(EventBridgeEvent::BackendCompleted(result)))) if result == completion)
    );
    assert!(matches!(receiver.recv().now_or_never(), Some(None)));
}
