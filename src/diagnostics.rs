//! Explicit opt-in, process-memory-only metadata. No logger, timer or worker.
use crate::errors::{Error, Owner};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub const RECORD_LIMIT: usize = 64;
pub const RETENTION: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub owner: Owner,
    pub error: Error,
    pub count: u32,
    pub duration_ms: Option<u64>,
    pub bytes: Option<usize>,
}

struct Record {
    first: Instant,
    metadata: Metadata,
}

#[derive(Default)]
pub struct Diagnostics {
    enabled: bool,
    records: VecDeque<Record>,
}

impl Diagnostics {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Disabling releases every retained record immediately.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.clear();
        }
    }

    pub fn clear(&mut self) {
        self.records = VecDeque::new();
    }

    pub fn record(
        &mut self,
        owner: Owner,
        error: Error,
        duration: Option<Duration>,
        bytes: Option<usize>,
        now: Instant,
    ) {
        if !self.enabled {
            return;
        }
        self.expire(now);
        let duration_ms = duration.map(|d| d.as_millis().min(u64::MAX as u128) as u64);
        if let Some(record) = self
            .records
            .iter_mut()
            .find(|r| r.metadata.owner == owner && r.metadata.error == error)
        {
            record.metadata.count = record.metadata.count.saturating_add(1);
            record.metadata.duration_ms = duration_ms;
            record.metadata.bytes = bytes;
            return;
        }
        if self.records.len() == RECORD_LIMIT {
            self.records.pop_front();
        }
        self.records.push_back(Record {
            first: now,
            metadata: Metadata {
                owner,
                error,
                count: 1,
                duration_ms,
                bytes,
            },
        });
    }

    pub(crate) fn expire(&mut self, now: Instant) {
        self.records
            .retain(|r| now.saturating_duration_since(r.first) < RETENTION);
        if self.records.is_empty() {
            self.clear();
        }
    }

    /// Expiration runs on admission or explicit capture, never in the background.
    /// Nothing is written to disk; callers explicitly choose where to save this
    /// bounded metadata. No API accepts arbitrary diagnostic context strings.
    pub fn capture(&mut self, now: Instant) -> String {
        self.expire(now);
        if !self.enabled {
            return String::new();
        }
        let mut out = String::new();
        for record in &self.records {
            use std::fmt::Write;
            let _ = writeln!(out, "{:?}", record.metadata);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{launch::LaunchError, tabs::Title};

    fn session() -> crate::identity::SessionId {
        let mut model = crate::workspace::Model::default();
        let (_, crate::session::Effect::Spawn(reservation)) = model.create_tab().unwrap() else {
            panic!("spawn")
        };
        reservation.session
    }

    #[test]
    fn opt_in_coalescing_retention_eviction_and_off_release_are_explicit() {
        let mut d = Diagnostics::default();
        let now = Instant::now();
        d.record(Owner::Application, Error::Cleanup, None, None, now);
        assert!(d.records.is_empty());
        assert!(d.capture(now).is_empty());
        d.set_enabled(true);
        for _ in 0..10000 {
            d.record(
                Owner::Application,
                Error::Cleanup,
                Some(Duration::from_millis(7)),
                Some(123),
                now,
            );
        }
        assert_eq!(d.records.len(), 1);
        assert_eq!(d.records[0].metadata.count, 10000);
        assert!(d.capture(now).contains("duration_ms: Some(7)"));
        // Repeats never extend retention beyond the first occurrence.
        d.record(
            Owner::Application,
            Error::Cleanup,
            None,
            None,
            now + RETENTION / 2,
        );
        assert!(d.capture(now + RETENTION).is_empty());
        for _ in 0..RECORD_LIMIT * 4 {
            d.record(Owner::Session(session()), Error::Cleanup, None, None, now);
        }
        assert_eq!(d.records.len(), RECORD_LIMIT);
        d.set_enabled(false);
        assert!(d.records.is_empty());
        assert_eq!(d.records.capacity(), 0);
        d.set_enabled(true);
        assert!(d.capture(now).is_empty());
    }

    #[test]
    fn secret_like_source_values_cannot_enter_metadata_including_repeats_and_eviction() {
        let secrets = [
            "SECRET_OUTPUT_34",
            "SECRET_INPUT_34",
            "SECRET_CLIPBOARD_34",
            "SECRET_ENV_34",
            "SECRET_ARGS_34",
            "SECRET_TITLE_34",
        ];
        let mut d = Diagnostics::default();
        d.set_enabled(true);
        let now = Instant::now();
        let title = Title::new(&format!("\u{202e}\x1b{}{}", secrets[5], "x".repeat(4096))).unwrap();
        assert!(title.as_str().chars().count() <= crate::tabs::TITLE_LIMIT);
        assert!(!title.as_str().chars().any(char::is_control));
        assert!(!format!("{title:?}").contains(secrets[5]));
        let error = std::io::Error::new(std::io::ErrorKind::BrokenPipe, secrets.join(" "));
        for _ in 0..RECORD_LIMIT * 4 {
            let owner = Owner::Session(session());
            for _ in 0..10 {
                d.record(
                    owner,
                    Error::Launch(LaunchError::Spawn(error.kind())),
                    Some(Duration::MAX),
                    Some(usize::MAX),
                    now,
                );
            }
        }
        let capture = d.capture(now);
        for secret in secrets {
            assert!(!capture.contains(secret));
        }
        assert!(capture.len() < RECORD_LIMIT * 1024);
    }
}
