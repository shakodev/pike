//! Setup must not write its scene into an attached terminal's placeholder grid.
use iced_term::ResizeResult;
use pike::identity::SessionId;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct LayoutBarrier(BTreeMap<SessionId, ResizeResult>);

impl LayoutBarrier {
    pub fn observe(&mut self, id: SessionId, result: ResizeResult) {
        if self
            .0
            .get(&id)
            .is_none_or(|previous| previous.serial < result.serial)
        {
            self.0.insert(id, result);
        }
    }

    pub fn ready(&self, id: SessionId, requests: u64, columns: usize, rows: usize) -> bool {
        self.0.get(&id).is_some_and(|ack| {
            requests > 0
                && ack.serial == requests
                && ack.result.is_ok()
                && usize::from(ack.size.num_cols) == columns
                && usize::from(ack.size.num_lines) == rows
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_term::{ResizeError, WindowSize};

    #[test]
    fn initial_grid_and_stale_or_failed_native_resizes_do_not_release_setup() {
        let mut model = pike::workspace::Model::default();
        model.create_tab().unwrap();
        model.create_tab().unwrap();
        let mut ids = model.sessions().iter().map(|(id, _)| id);
        let id = ids.next().unwrap();
        let other = ids.next().unwrap();
        let mut barrier = LayoutBarrier::default();
        let mut ack = ResizeResult {
            serial: 1,
            size: WindowSize {
                num_cols: 46,
                num_lines: 24,
                cell_width: 8,
                cell_height: 16,
            },
            result: Ok(()),
        };
        assert!(!barrier.ready(id, 0, 80, 50));
        assert!(!barrier.ready(id, 1, 46, 24));
        barrier.observe(id, ack);
        assert!(barrier.ready(id, 1, 46, 24));
        assert!(!barrier.ready(other, 1, 46, 24));
        assert!(!barrier.ready(id, 2, 46, 24));
        assert!(!barrier.ready(id, 1, 80, 50));
        ack.serial = 2;
        ack.result = Err(ResizeError {
            kind: std::io::ErrorKind::Other,
            code: None,
        });
        barrier.observe(id, ack);
        assert!(!barrier.ready(id, 2, 46, 24));
        ack.serial = 1;
        ack.result = Ok(());
        barrier.observe(id, ack);
        assert!(
            !barrier.ready(id, 1, 46, 24),
            "late acknowledgements cannot reopen the gate"
        );
        ack.serial = 3;
        barrier.observe(id, ack);
        assert!(barrier.ready(id, 3, 46, 24));
    }
}
