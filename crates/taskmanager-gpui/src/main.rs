//! `taskforest-g` — the GPUI desktop product binary (ADR-051).
//!
//! Thin by law: it hands this product's capability set to the shared CLI
//! harness (`taskmanager_cli::run`) and owns nothing else. The GPUI product
//! carries the Windows `--capture-window` evidence capability; it has no
//! `--snapshot` capability (that is the TUI product's).

#![forbid(unsafe_code)]
// The GUI product must not allocate a console when launched from the Start
// Menu: a console-subsystem PE makes Windows open a terminal window beside
// the app window. Piped/redirected stdout still works; only interactive
// terminal echo is given up.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::path::Path;

use taskmanager_cli::{FrontendHandlers, run};

/// Windows-only window self-capture (`Windows.Graphics.Capture` through the
/// gpui crate's audited capture edge). On every other platform the capability
/// is honestly unsupported — the mode simply is not this product's.
fn capture_window(out: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        taskmanager_gpui::capture::run(out)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = out;
        Err("--capture-window is a Windows GPUI evidence mode; run the \
             taskforest-g product on Windows"
            .to_owned())
    }
}

fn main() {
    run(
        "taskforest-g",
        FrontendHandlers {
            run_gui: taskmanager_gpui::run,
            snapshot_text: None,
            capture_window: Some(capture_window),
        },
    );
}
