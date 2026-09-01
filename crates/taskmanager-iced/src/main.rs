//! `taskforest-i` — the iced desktop product binary (ADR-051).
//!
//! Thin by law: it hands this product's capability set to the shared CLI
//! harness (`taskmanager_cli::run`). The iced product has no `--snapshot`
//! capability (TUI's) and no `--capture-window` capability (Windows GPUI's).

#![forbid(unsafe_code)]
// The GUI product must not allocate a console when launched from the Start
// Menu: a console-subsystem PE makes Windows open a terminal window beside
// the app window. Piped/redirected stdout still works; only interactive
// terminal echo is given up.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use taskmanager_cli::{FrontendHandlers, run};

/// Launch the iced frontend. The desktop `app_id` is accepted and ignored so
/// the CLI surface stays uniform across products (the iced window identity
/// comes from the crate's own composition edge).
fn run_gui(app_id: Option<String>, demo: bool) {
    let _ = app_id;
    if let Err(error) = taskmanager_iced::run(demo) {
        eprintln!("taskforest-i: {error}");
        std::process::exit(1);
    }
}

fn main() {
    run(
        "taskforest-i",
        FrontendHandlers {
            run_gui,
            snapshot_text: None,
            capture_window: None,
        },
    );
}
