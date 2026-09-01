//! `taskmanager-tui` — the Ratatui terminal product binary (ADR-051).
//!
//! Thin by law: it hands this product's capability set to the shared CLI
//! harness (`taskmanager_cli::run`). The TUI product owns the headless
//! `--snapshot [W H]` text-frame capability and runs inside a terminal, so
//! the binary keeps the console subsystem on Windows.

#![forbid(unsafe_code)]

use taskmanager_cli::{FrontendHandlers, run};

/// Launch the TUI. The desktop `app_id` is a graphical-product concept; the
/// TUI accepts and ignores it so the CLI surface stays uniform (the unified
/// CLI now lives in the shared harness).
fn run_gui(app_id: Option<String>, demo: bool) {
    let _ = app_id;
    let result = if demo {
        taskmanager_tui::run_demo()
    } else {
        taskmanager_tui::run_live()
    };
    if let Err(error) = result {
        eprintln!("taskmanager-tui: {error}");
        std::process::exit(1);
    }
}

fn main() {
    run(
        "taskmanager-tui",
        FrontendHandlers {
            run_gui,
            snapshot_text: Some(taskmanager_tui::snapshot_text),
            capture_window: None,
        },
    );
}
