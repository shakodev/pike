//! Compile the independent error/diagnostic contract before host integration.
#![allow(dead_code)]
use pike::{identity, launch, session, settings, tabs, workspace};
#[path = "../src/diagnostics.rs"]
mod diagnostics;
#[path = "../src/errors.rs"]
mod errors;
