//! At most one current clipboard acknowledgment per stable owner. Strings and
//! focus authority never enter this operation-ordering ledger.
use super::{NOTICE_LIMIT, Owner};
use crate::identity::Reservation;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteReceipt {
    pub(crate) reservation: Option<Reservation>,
    serial: u64,
}
impl WriteReceipt {
    pub(crate) fn owner(self) -> Owner {
        self.reservation
            .map_or(Owner::Application, |r| Owner::Session(r.session))
    }
}

#[derive(Default)]
pub(crate) struct Writes {
    next: u64,
    current: VecDeque<WriteReceipt>,
}
impl Writes {
    pub(crate) fn begin(&mut self, reservation: Option<Reservation>) -> Option<WriteReceipt> {
        self.next = self.next.checked_add(1)?;
        let receipt = WriteReceipt {
            reservation,
            serial: self.next,
        };
        self.current.retain(|r| r.owner() != receipt.owner());
        if self.current.len() == NOTICE_LIMIT {
            self.current.pop_front();
        }
        self.current.push_back(receipt);
        Some(receipt)
    }
    pub(crate) fn finish(&mut self, receipt: WriteReceipt) -> bool {
        let Some(position) = self.current.iter().position(|r| *r == receipt) else {
            return false;
        };
        self.current.remove(position);
        true
    }
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(Owner) -> bool) {
        self.current.retain(|r| keep(r.owner()));
    }
    pub(crate) fn clear(&mut self) {
        self.current = VecDeque::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newer_operation_invalidates_old_and_duplicate_receipts_and_clear_releases_memory() {
        let mut writes = Writes::default();
        let old = writes.begin(None).unwrap();
        let current = writes.begin(None).unwrap();
        assert!(!writes.finish(old));
        assert!(writes.finish(current));
        assert!(!writes.finish(current));
        writes.clear();
        assert_eq!(writes.current.capacity(), 0);
        assert_ne!(writes.begin(None), Some(old));
        writes.next = u64::MAX;
        assert!(writes.begin(None).is_none());
    }
}
