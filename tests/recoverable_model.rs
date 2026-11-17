//! Compile the independent error/diagnostic contract before host integration.
#![allow(dead_code, unused_imports)]
use pike::{app, identity, launch, session, settings, tabs, workspace};
#[path = "../src/diagnostics.rs"]
mod diagnostics;
#[path = "../src/errors.rs"]
mod errors;
