//! `taskforest-b` — the Bevy UI fourth frontend product binary (ADR-051).
//!
//! Thin by law: it hands this product's capability set to the shared CLI
//! harness (`taskmanager_cli::run`) instead of hand-parsing `--demo`. The
//! shared runtime, drain seam, palette adapter and live window all
//! live in the library crate, so every bevy type stays inside
//! `taskmanager_bevy_ui` and this bin stays a composition-free entry point.

#![forbid(unsafe_code)]
// The GUI product must not allocate a console when launched from the Start
// Menu on Windows (same rule as the other graphical products).
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::process::ExitCode;

use taskmanager_cli::{FrontendHandlers, run};

/// Launch the Bevy window. The desktop `app_id` is accepted and ignored so
/// the CLI surface stays uniform across products; `demo` switches the
/// fixture-data window.
fn run_gui(app_id: Option<String>, demo: bool) {
    let _ = app_id;
    let code = if demo {
        taskmanager_bevy_ui::run_demo_window()
    } else {
        taskmanager_bevy_ui::run_window()
    };
    if code != ExitCode::SUCCESS {
        std::process::exit(1);
    }
}

fn main() {
    run(
        "taskforest-b",
        FrontendHandlers {
            run_gui,
            snapshot_text: None,
            capture_window: None,
        },
    );
}
