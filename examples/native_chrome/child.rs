//! Controlled child shared by native before/after and pixel fixtures.
use std::io::{self, Read, Write};

pub fn run(exit: bool) -> io::Result<()> {
    print!(
        "\x1b]2;PIKE33 controlled fixture\x07\x1b[2J\x1b[H\x1b[32mPIKE33 controlled prompt > \x1b[0m\x1b[?25h"
    );
    io::stdout().flush()?;
    if exit {
        return Ok(());
    }
    let mut byte = [0];
    while io::stdin().read(&mut byte)? != 0 {}
    Ok(())
}
