//! Bounded synthetic-editor diagnostics; a cleanup failure cannot mask the
//! original assertion/readiness error. No production terminal data is sampled.
use iced_term::Terminal;
use std::{
    fmt::Debug,
    fs,
    io::{self, Write},
    path::Path,
};

pub fn record(output: &Path, stage: &str, terminal: &Terminal) -> io::Result<()> {
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(output.join("micro-stages.txt"))?;
    writeln!(
        log,
        "stage={stage} input={:?} work={:?} completion={:?}",
        terminal.input_status(),
        terminal.work_stats(),
        terminal.shutdown_handle().completion()
    )?;
    if let Some(content) = terminal.renderable_content() {
        writeln!(
            log,
            "point={:?} shape={:?} mode={:?} first={:?}",
            content.cursor_point,
            content.cursor_shape,
            content.terminal_mode,
            content.viewport.cells().first()
        )?;
        for row in content
            .viewport
            .cells()
            .chunks(content.viewport.columns().max(1))
            .take(8)
        {
            let text: String = row.iter().take(80).map(|cell| cell.c).collect();
            writeln!(log, "row={text:?}")?;
        }
    }
    Ok(())
}

pub fn finish<T: Debug>(primary: io::Result<()>, cleanup: io::Result<T>) -> io::Result<T> {
    match (primary, cleanup) {
        (Ok(()), Ok(completed)) => Ok(completed),
        (primary, cleanup) => Err(io::Error::other(format!(
            "actual Micro primary={primary:?}; cleanup={cleanup:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_timeout_never_hides_primary_readiness_failure() {
        let error = finish::<()>(
            Err(io::Error::other("readiness15s")),
            Err(io::Error::other("shutdown10s")),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("readiness15s") && error.contains("shutdown10s"));
    }
    #[test]
    fn natural_cleanup_does_not_turn_a_failed_pixel_verdict_into_success() {
        let error = finish(
            Err(io::Error::other("pixel failure")),
            Ok("ChildExit0 drained"),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("pixel failure") && error.contains("ChildExit0 drained"));
        assert_eq!(finish(Ok(()), Ok(7)).unwrap(), 7);
    }
}
