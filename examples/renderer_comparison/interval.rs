//! Timing validity is separate from resource/latency budgets.
pub fn duration_valid(expected_seconds: u64, actual_ms: f64) -> bool {
    let expected = expected_seconds as f64 * 1000.0;
    actual_ms.is_finite() && (actual_ms - expected).abs() <= tolerance(expected)
}
fn tolerance(expected: f64) -> f64 {
    250.0_f64.max(expected * 0.01)
}
pub fn overlap_valid(
    expected_seconds: u64,
    actual_ms: f64,
    parent: (u128, u128),
    child: (u128, u128),
) -> bool {
    let expected = expected_seconds as f64 * 1000.0;
    let (Some(parent_ms), Some(child_ms)) =
        (parent.1.checked_sub(parent.0), child.1.checked_sub(child.0))
    else {
        return false;
    };
    let overlap = parent.1.min(child.1).saturating_sub(parent.0.max(child.0)) as f64;
    duration_valid(expected_seconds, actual_ms)
        && (parent_ms as f64 - actual_ms).abs() <= tolerance(expected)
        && (child_ms as f64 - expected).abs() <= tolerance(expected)
        && overlap >= expected * 0.98
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_end_and_nonoverlapping_producer_cannot_qualify_a_run() {
        assert!(!duration_valid(3, 12516.0));
        assert!(!overlap_valid(3, 3000.0, (10000, 13000), (1000, 4000)));
        assert!(!overlap_valid(3, 12516.0, (1000, 13516), (1000, 4000)));
    }
    #[test]
    fn small_scheduler_jitter_is_allowed_but_clock_reversal_is_not() {
        assert!(overlap_valid(3, 3020.0, (1000, 4020), (1005, 4007)));
        assert!(!overlap_valid(3, 3000.0, (4000, 1000), (1000, 4000)));
    }
}
