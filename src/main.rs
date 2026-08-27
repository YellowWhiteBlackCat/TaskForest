//! Binary entry point: parses CLI arguments and dispatches to the compiled-in
//! UI frontend (ADR-029), the stateless JSON snapshot mode, or help output.
//!
//! Exactly one `ui-*` feature shapes this binary: `ui-gpui` (default) runs
//! the GPUI desktop frontend, `ui-tui` the ratatui frontend, `ui-iced` the
//! iced frontend. The UI-neutral modes (`--json`, `--suggest-thresholds`,
//! `--gpu-engines`) are identical in every shape.

#![forbid(unsafe_code)]
#![allow(linker_messages)]
// The product GUI shapes (ui-gpui, ui-iced) must not allocate a console when
// launched from the Start Menu: a console-subsystem PE makes Windows open a
// terminal window beside the app window. The TUI shape keeps the console
// subsystem — it runs inside one. Piped/redirected stdout in the GUI shapes
// still works; only interactive terminal echo for them is given up.
#![cfg_attr(
    all(target_os = "windows", any(feature = "ui-gpui", feature = "ui-iced")),
    windows_subsystem = "windows"
)]

use std::io;

use taskmanager::cli::{self, CliMode};
use taskmanager::frontend;
use taskmanager_app_host::NativeAppHost;

fn main() {
    // Parse argv before any tracing initialization: the JSON snapshot mode
    // owns stdout exclusively (a tracing subscriber defaulting to stdout would
    // corrupt the document), and help/errors are written directly.
    let mode = match cli::parse_args(std::env::args().skip(1)) {
        Ok(mode) => mode,
        Err(error) => {
            let stderr = io::stderr();
            let _ = write_usage(&mut stderr.lock(), Some(&error.to_string()));
            std::process::exit(2);
        }
    };

    match mode {
        CliMode::Help => {
            let stdout = io::stdout();
            let _ = cli::print_help_to(&mut stdout.lock());
        }
        CliMode::JsonSnapshot => {
            // The binary composition root invokes the shared app-host seam;
            // the collector itself is the toolkit-neutral CLI module.
            let client = match NativeAppHost::production().spawn_client() {
                Ok(client) => client,
                Err(error) => {
                    eprintln!("taskmanager --json: native composition failed: {error}");
                    std::process::exit(1);
                }
            };
            if let Err(error) = cli::run_json_snapshot_with(client) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        CliMode::SuggestThresholds => {
            // Same app-host ownership as --json; only the CLI rendering
            // differs.
            let client = match NativeAppHost::production().spawn_client() {
                Ok(client) => client,
                Err(error) => {
                    eprintln!(
                        "taskmanager --suggest-thresholds: native composition failed: {error}"
                    );
                    std::process::exit(1);
                }
            };
            if let Err(error) = cli::run_suggest_thresholds_with(client) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        CliMode::GpuEngines => {
            // Per-feature escalation (ADR-023): this path does NOT spawn the
            // native telemetry runtime and does NOT elevate the main app. It
            // drives the polkit/pkexec helper on demand and prints the typed
            // outcome (engine data or an honest denial) as JSON.
            if let Err(error) = taskmanager::cli_gpu_engines::run_gpu_engines() {
                eprintln!("taskmanager --gpu-engines: {error}");
                std::process::exit(1);
            }
        }
        CliMode::Snapshot { width, height } => {
            #[cfg(feature = "ui-tui")]
            {
                print!("{}", taskmanager_tui::snapshot_text(width, height));
            }
            #[cfg(not(feature = "ui-tui"))]
            {
                let _ = (width, height);
                eprintln!(
                    "taskmanager: --snapshot is only supported by the TUI shape (build with --no-default-features --features ui-tui)"
                );
                std::process::exit(2);
            }
        }
        CliMode::CaptureWindow { out } => {
            // Windows+GPUI evidence mode: the frontend composition edge owns
            // the real window, so the capture runs through the selected shape
            // (only ui-gpui on Windows implements it).
            if let Err(error) = frontend::run_capture(&out) {
                eprintln!("taskmanager --capture-window: {error}");
                std::process::exit(1);
            }
        }
        CliMode::Gui { app_id, demo } => frontend::run(app_id, demo),
    }
}

fn write_usage(writer: &mut impl io::Write, prefix: Option<&str>) -> io::Result<()> {
    if let Some(prefix) = prefix {
        writeln!(writer, "{prefix}")?;
    }
    cli::print_help_to(writer)?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/logic/main_tests.rs"]
mod tests;
