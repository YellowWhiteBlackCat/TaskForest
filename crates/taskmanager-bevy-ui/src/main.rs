//! `taskforest-b` — the Bevy UI fourth frontend binary (M0 skeleton).
//!
//! The bin is deliberately thin: the shared runtime, drain seam, palette
//! adapter and placeholder window all live in the library crate, so every
//! bevy type stays inside `taskmanager_bevy_ui` and the artifact pattern of
//! the frontend binaries build script (taskforest-g / taskforest-i /
//! taskforest-b) sees one composition-free entry point.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    if std::env::args().any(|argument| argument == "--demo") {
        taskmanager_bevy_ui::run_demo_window()
    } else {
        taskmanager_bevy_ui::run_placeholder_window()
    }
}
