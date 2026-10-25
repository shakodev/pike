use super::*;
use std::sync::atomic::Ordering;

fn queue() -> InputQueue {
    InputQueue::new(Arc::new(WriterProgress::default()))
}

fn send(queue: &InputQueue, bytes: &[u8]) -> Result<(), InputError> {
    queue.send(bytes, || Ok(()))
}

#[test]
fn full_paste_is_preflighted_and_rejection_changes_nothing() {
    let queue = queue();
    assert_eq!(
        send(&queue, &vec![0; INPUT_BYTE_LIMIT + 1]),
        Err(InputError::Full { requested: INPUT_BYTE_LIMIT + 1, available: INPUT_BYTE_LIMIT })
    );
    assert_eq!(queue.status().outstanding_bytes, 0);
    assert!(queue.is_empty());
    send(&queue, &vec![1; INPUT_BYTE_LIMIT]).unwrap();
    assert_eq!(send(&queue, b"x"), Err(InputError::Full { requested: 1, available: 0 }));
    assert_eq!(queue.status().outstanding_bytes, INPUT_BYTE_LIMIT);
    assert_eq!(queue.lock().chunks.len(), INPUT_BYTE_LIMIT / INPUT_CHUNK_SIZE);
}

#[test]
fn dequeuing_or_buffering_does_not_release_writer_credit() {
    let queue = queue();
    send(&queue, &vec![0; INPUT_BYTE_LIMIT]).unwrap();
    let chunk = queue.take().unwrap();
    assert_eq!(chunk.len(), INPUT_CHUNK_SIZE);
    assert_eq!(queue.status().outstanding_bytes, INPUT_BYTE_LIMIT);
    assert!(queue.preflight(1).is_err());
    queue.acknowledge(3);
    assert_eq!(queue.status().outstanding_bytes, INPUT_BYTE_LIMIT - 3);
    send(&queue, b"abc").unwrap();
    assert!(queue.preflight(1).is_err());
}

#[test]
fn byte_order_and_utf8_survive_chunk_boundaries_exactly_once() {
    let queue = queue();
    let text = format!("{}{}", "x".repeat(INPUT_CHUNK_SIZE - 1), "界🦀e\u{301}".repeat(9000));
    send(&queue, text.as_bytes()).unwrap();
    send(&queue, b"last").unwrap();
    let mut received = Vec::new();
    while let Some(chunk) = queue.take() {
        assert!(chunk.len() <= INPUT_CHUNK_SIZE);
        queue.acknowledge(chunk.len());
        received.extend(chunk);
    }
    assert_eq!(received, [text.as_bytes(), b"last"].concat());
    assert_eq!(queue.status().outstanding_bytes, 0);
    assert_eq!(queue.status().written_bytes, received.len());
}

#[test]
fn tiny_writes_are_packed_and_cannot_grow_message_overhead() {
    let queue = queue();
    for _ in 0..100_000 {
        send(&queue, b"x").unwrap();
    }
    let state = queue.lock();
    assert_eq!(state.chunks.len(), 100_000_usize.div_ceil(INPUT_CHUNK_SIZE));
    assert_eq!(state.queued, 100_000);
}

#[test]
fn wake_failure_is_an_atomic_rejection() {
    let queue = queue();
    assert_eq!(
        queue.send(b"never admitted", || Err(io::Error::other("fixture"))),
        Err(InputError::WakeFailed)
    );
    assert_eq!(queue.status().outstanding_bytes, 0);
    assert!(queue.lock().chunks.is_empty());
}

#[test]
fn cancel_clears_pending_allocations_and_rejects_late_input() {
    let queue = queue();
    send(&queue, &vec![0; INPUT_BYTE_LIMIT]).unwrap();
    let _in_flight = queue.take().unwrap();
    queue.close();
    queue.close();
    let status = queue.status();
    assert!(status.closed);
    assert_eq!(status.queued_bytes, 0);
    assert_eq!(status.outstanding_bytes, 0);
    assert_eq!(status.cancelled_bytes, INPUT_BYTE_LIMIT);
    assert_eq!(send(&queue, b"late"), Err(InputError::Closed));
    assert!(queue.take().is_none());
    // An OS write already in progress at cancellation can finish.
    queue.acknowledge(10);
    assert_eq!(queue.status().cancelled_bytes, INPUT_BYTE_LIMIT - 10);
}

#[test]
fn concurrent_producers_share_one_atomic_admission_budget() {
    let queue = queue();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let queue = queue.clone();
            std::thread::spawn(move || send(&queue, &vec![1; INPUT_BYTE_LIMIT / 2]))
        })
        .collect();
    let admitted =
        threads.into_iter().map(|thread| thread.join().unwrap()).filter(Result::is_ok).count();
    assert_eq!(admitted, 2);
    assert_eq!(queue.status().outstanding_bytes, INPUT_BYTE_LIMIT);
}

#[test]
fn cumulative_accounting_remains_correct_at_counter_wrap() {
    let queue = queue();
    queue.lock().accepted = usize::MAX - 2;
    queue.0.progress.written.store(usize::MAX - 2, Ordering::Release);
    send(&queue, b"abcdef").unwrap();
    assert_eq!(queue.status().outstanding_bytes, 6);
    queue.acknowledge(6);
    assert_eq!(queue.status().outstanding_bytes, 0);
}
