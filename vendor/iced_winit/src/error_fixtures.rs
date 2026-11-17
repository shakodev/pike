//! Off-by-default native fixture injection at the existing compositor result.
//! Normal Pike builds do not enable or compile this module. No environment hook.
use crate::graphics::compositor::SurfaceError;
use std::{collections::VecDeque, sync::Mutex};

static FAULTS: Mutex<VecDeque<SurfaceError>> = Mutex::new(VecDeque::new());

/// Arm at most four synthetic failures. Only the separate native fixture enables
/// this feature; the existing real compositor still presents each tested frame.
pub fn inject(error: SurfaceError, count: usize) {
    let mut faults = FAULTS.lock().unwrap();
    faults.clear();
    faults.extend(std::iter::repeat_n(error, count.min(4)));
}

pub(crate) fn present(
    result: Result<(), SurfaceError>,
) -> Result<(), SurfaceError> {
    FAULTS.lock().unwrap().pop_front().map_or(result, Err)
}
