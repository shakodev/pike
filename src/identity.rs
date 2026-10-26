//! Process-wide, checked identities shared by launch and pure application state.
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TabId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdsExhausted;

/// A single launch attempt. Both IDs come from the existing launch ID domain.
/// There is no public constructor from integers and no way to recycle an ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reservation {
    pub session: SessionId,
    pub request: RequestId,
}

impl Reservation {
    pub(crate) fn allocate() -> Result<Self, IdsExhausted> {
        let id = allocate(&NEXT_ID)?;
        Ok(Self {
            session: SessionId(id),
            request: RequestId(id),
        })
    }
}

impl TabId {
    pub(crate) fn allocate() -> Result<Self, IdsExhausted> {
        allocate(&NEXT_ID).map(Self)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl SessionId {
    pub fn get(self) -> u64 {
        self.0
    }
}

impl RequestId {
    pub fn get(self) -> u64 {
        self.0
    }
}

fn allocate(next: &AtomicU64) -> Result<u64, IdsExhausted> {
    next.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .map_err(|_| IdsExhausted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhaustion_is_permanent_and_never_wraps() {
        // Exercise the actual allocator without exhausting the process global.
        let next = AtomicU64::new(u64::MAX - 1);
        assert_eq!(allocate(&next), Ok(u64::MAX - 1));
        for _ in 0..3 {
            assert_eq!(allocate(&next), Err(IdsExhausted));
        }
        assert_eq!(next.load(Ordering::Relaxed), u64::MAX);
    }
}
