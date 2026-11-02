//! One outstanding input/readback observation; End revokes all late callbacks.
use std::time::Instant;

pub struct Samples {
    requested: usize,
    started: usize,
    completed: usize,
    active: bool,
    ended: bool,
    pending: Option<Pending>,
}
struct Pending {
    nonce: usize,
    sent: Instant,
    snapshot: Option<Instant>,
    in_flight: bool,
    attempts: u8,
}
pub struct Sample {
    pub nonce: usize,
    pub sent: Instant,
    pub snapshot: Instant,
    pub captured: Instant,
}

impl Samples {
    pub fn new(requested: usize) -> Self {
        Self {
            requested,
            started: 0,
            completed: 0,
            active: false,
            ended: false,
            pending: None,
        }
    }
    pub fn open(&mut self) {
        self.active = true;
    }
    pub fn begin(&mut self, now: Instant) -> Option<usize> {
        if !self.active || self.pending.is_some() || self.started == self.requested {
            return None;
        }
        self.started += 1;
        self.pending = Some(Pending {
            nonce: self.started,
            sent: now,
            snapshot: None,
            in_flight: false,
            attempts: 0,
        });
        Some(self.started)
    }
    pub fn expected_snapshot(&self) -> Option<usize> {
        self.pending
            .as_ref()
            .filter(|p| self.active && p.snapshot.is_none())
            .map(|p| p.nonce)
    }
    pub fn snapshot(&mut self, now: Instant) -> Option<usize> {
        let pending = self
            .pending
            .as_mut()
            .filter(|p| self.active && p.snapshot.is_none())?;
        pending.snapshot = Some(now);
        Some(pending.nonce)
    }
    pub fn capture(&mut self, nonce: usize, now: Instant) -> Option<Sample> {
        let pending = self
            .pending
            .as_ref()
            .filter(|p| self.active && p.nonce == nonce && p.snapshot.is_some() && p.in_flight)?;
        let sample = Sample {
            nonce,
            sent: pending.sent,
            snapshot: pending.snapshot?,
            captured: now,
        };
        self.pending = None;
        self.completed += 1;
        Some(sample)
    }
    pub fn waiting_frame(&self) -> bool {
        self.active
            && self
                .pending
                .as_ref()
                .is_some_and(|p| p.snapshot.is_some() && !p.in_flight && p.attempts < 3)
    }
    pub fn frame(&mut self) -> Option<usize> {
        if !self.waiting_frame() {
            return None;
        }
        let pending = self.pending.as_mut()?;
        pending.in_flight = true;
        pending.attempts += 1;
        Some(pending.nonce)
    }
    pub fn reject_pixels(&mut self, nonce: usize) {
        if let Some(pending) = self
            .pending
            .as_mut()
            .filter(|p| p.nonce == nonce && p.in_flight)
        {
            pending.in_flight = false;
        }
    }
    pub fn end(&mut self) -> bool {
        self.active = false;
        self.ended = true;
        self.pending = None;
        self.complete()
    }
    pub fn complete(&self) -> bool {
        self.ended && self.started == self.requested && self.completed == self.requested
    }
    pub fn completed(&self) -> usize {
        self.completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn end_revokes_pending_capture_and_incomplete_samples_fail() {
        let mut samples = Samples::new(1);
        let now = Instant::now();
        assert!(samples.begin(now).is_none());
        samples.open();
        assert_eq!(samples.begin(now), Some(1));
        assert_eq!(samples.snapshot(now), Some(1));
        assert_eq!(samples.frame(), Some(1));
        assert!(!samples.end());
        assert!(samples.capture(1, now).is_none());
        assert_eq!(samples.completed(), 0);
        assert!(samples.begin(now).is_none());
    }
    #[test]
    fn stale_or_duplicate_nonces_cannot_consume_current_capture() {
        let mut samples = Samples::new(2);
        let now = Instant::now();
        samples.open();
        for nonce in 1..=2 {
            assert_eq!(samples.begin(now), Some(nonce));
            assert!(samples.begin(now).is_none());
            assert_eq!(samples.snapshot(now), Some(nonce));
            assert!(samples.snapshot(now).is_none());
            assert_eq!(samples.frame(), Some(nonce));
            assert!(samples.capture(nonce + 1, now).is_none());
            assert!(samples.capture(nonce, now).is_some());
            assert!(samples.capture(nonce, now).is_none());
        }
        assert!(samples.begin(now).is_none());
        assert!(samples.end());
        assert_eq!(samples.completed(), 2);
    }
    #[test]
    fn zero_sample_idle_interval_finishes_without_pending_work() {
        let mut samples = Samples::new(0);
        assert!(!samples.complete());
        samples.open();
        assert!(samples.end());
    }
    #[test]
    fn stale_pixels_retry_only_on_frames_with_a_finite_attempt_budget() {
        let mut samples = Samples::new(1);
        let now = Instant::now();
        samples.open();
        samples.begin(now);
        samples.snapshot(now);
        assert!(samples.capture(1, now).is_none());
        for _ in 0..3 {
            assert_eq!(samples.frame(), Some(1));
            assert!(samples.frame().is_none());
            samples.reject_pixels(1);
        }
        assert!(!samples.waiting_frame());
        assert!(samples.frame().is_none());
        assert!(!samples.end());
    }
}
