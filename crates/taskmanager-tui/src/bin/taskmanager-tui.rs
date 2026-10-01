//! `taskmanager-tui` — compatibility binary entry point (ADR-051).
//!
//! Thin by law: it hands this product's capability set to the shared CLI
//! harness (`taskmanager_cli::run`). The TUI product owns the headless
//! `--snapshot [W H]` text-frame capability and runs inside a terminal, so
//! the binary keeps the console subsystem on Windows.

#![forbid(unsafe_code)]

use taskmanager_tui::run_cli;

fn main() {
    run_cli("taskmanager-tui");
}
