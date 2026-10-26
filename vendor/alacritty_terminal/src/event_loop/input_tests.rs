use super::*;
use crate::input::INPUT_BYTE_LIMIT;

#[test]
fn legacy_input_and_priority_cancel_never_enqueue_payload_messages() {
    let (sender, receiver) = mpsc::channel();
    let poller = Arc::new(polling::Poller::new().unwrap());
    let channel = EventLoopSender {
        sender,
        poller: Arc::downgrade(&poller),
        input: InputQueue::new(Arc::new(WriterProgress::default())),
        shutdown_requested: Arc::new(AtomicBool::new(false)),
    };
    channel.send(Msg::Input(Cow::Owned(vec![1; INPUT_BYTE_LIMIT]))).unwrap();
    assert_eq!(channel.input_status().outstanding_bytes, INPUT_BYTE_LIMIT);
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    assert!(matches!(
        channel.send(Msg::Input(Cow::Borrowed(b"x"))),
        Err(EventLoopSendError::Input(InputError::Full { .. }))
    ));
    channel.send(Msg::Shutdown).unwrap();
    let status = channel.input_status();
    assert!(status.closed);
    assert_eq!(status.queued_bytes, 0);
    assert_eq!(status.cancelled_bytes, INPUT_BYTE_LIMIT);
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    assert_eq!(channel.send_input(b"late"), Err(InputError::Closed));
}
