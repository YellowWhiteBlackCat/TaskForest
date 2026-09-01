//! Product-binary entry harness: one function per product, zero `cfg`.
//!
//! Every frontend product `[[bin]]` (ADR-051) is a thin shim that calls
//! [`run`] with its own binary name and capability handlers. The harness owns
//! argv parsing, the UI-neutral modes, tracing initialization, and the honest
//! "unsupported" reporting for capabilities a product does not carry. Shape
//! differences are values in a struct — there is no feature dispatch anywhere
//! on this path.

#![forbid(unsafe_code)]

use std::io::{self, IsTerminal};
use std::path::Path;

use taskmanager_app_host::NativeAppHost;

use crate::cli::{self, CliCapabilities, CliMode};

/// Window-capture handler: write the product's own capture evidence into
/// `out` (Windows GPUI product only).
pub type CaptureHandler = fn(&Path) -> Result<(), String>;

/// The shape-owned handlers a frontend product contributes to the shared CLI.
///
/// `run_gui` is required (every product has a UI); the optional capabilities
/// follow the products: `snapshot_text` is the TUI product's headless
/// text-frame renderer, `capture_window` the Windows GPUI product's
/// self-capture evidence mode. Absent capabilities print an honest
/// unsupported message instead of silently doing nothing.
pub struct FrontendHandlers {
    /// Launch the product's UI. `app_id` is the already-validated desktop
    /// application ID (used by graphical products); `demo` requests fixture
    /// data with no host I/O.
    pub run_gui: fn(Option<String>, demo: bool),
    /// Render one fixed-size headless text frame (TUI product only).
    pub snapshot_text: Option<fn(u16, u16) -> String>,
    /// Capture the product's own window once into `out` (Windows GPUI only).
    pub capture_window: Option<CaptureHandler>,
}

impl FrontendHandlers {
    /// The capability flags derived from the optional handlers; they shape the
    /// help text and nothing else.
    #[must_use]
    pub const fn capabilities(&self) -> CliCapabilities {
        CliCapabilities {
            snapshot_text: self.snapshot_text.is_some(),
            capture_window: self.capture_window.is_some(),
        }
    }
}

/// Run one product process to completion: parse argv, execute the selected
/// mode, and exit the process on usage errors. This function does not return
/// an error type because every failure path already owns its exit code and
/// stderr document.
pub fn run(binary_name: &'static str, handlers: FrontendHandlers) {
    // Parse argv before any tracing initialization: the JSON snapshot mode
    // owns stdout exclusively (a tracing subscriber defaulting to stdout would
    // corrupt the document), and help/errors are written directly.
    let mode = match cli::parse_args(std::env::args().skip(1)) {
        Ok(mode) => mode,
        Err(error) => {
            let stderr = io::stderr();
            let _ = write_usage(
                &mut stderr.lock(),
                binary_name,
                handlers.capabilities(),
                Some(&error.to_string()),
            );
            std::process::exit(2);
        }
    };

    match mode {
        CliMode::Help => {
            let stdout = io::stdout();
            let _ = cli::print_help_to(&mut stdout.lock(), binary_name, handlers.capabilities());
        }
        CliMode::JsonSnapshot => {
            // The product composition invokes the shared app-host seam; the
            // collector itself is the toolkit-neutral CLI module.
            let client = match NativeAppHost::production().spawn_client() {
                Ok(client) => client,
                Err(error) => {
                    eprintln!("{binary_name} --json: native composition failed: {error}");
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
                        "{binary_name} --suggest-thresholds: native composition failed: {error}"
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
            if let Err(error) = crate::cli_gpu_engines::run_gpu_engines() {
                eprintln!("{binary_name} --gpu-engines: {error}");
                std::process::exit(1);
            }
        }
        CliMode::MemorySmbios => {
            // Same per-feature escalation discipline as --gpu-engines: the
            // SMBIOS memory helper crossing, prompted only because this flag
            // was passed; the typed outcome prints as JSON.
            if let Err(error) = crate::cli_memory_smbios::run_memory_smbios() {
                eprintln!("{binary_name} --memory-smbios: {error}");
                std::process::exit(1);
            }
        }
        CliMode::PackagePower => {
            // Same per-feature escalation discipline as --gpu-engines: the
            // RAPL package-power helper crossing, prompted only because this
            // flag was passed; the typed outcome prints as JSON.
            if let Err(error) = crate::cli_package_power::run_package_power() {
                eprintln!("{binary_name} --package-power: {error}");
                std::process::exit(1);
            }
        }
        CliMode::Msr => {
            // Same per-feature escalation discipline as --gpu-engines: the
            // MSR readout helper crossing (ADR-048), prompted only because
            // this flag was passed; the typed outcome prints as JSON.
            if let Err(error) = crate::cli_msr::run_msr() {
                eprintln!("{binary_name} --msr: {error}");
                std::process::exit(1);
            }
        }
        CliMode::Snapshot { width, height } => match handlers.snapshot_text {
            Some(render) => print!("{}", render(width, height)),
            None => {
                eprintln!(
                    "{binary_name}: --snapshot is only supported by the TUI product \
                     (taskmanager-tui)"
                );
                std::process::exit(2);
            }
        },
        CliMode::CaptureWindow { out } => {
            // Windows+GPUI evidence mode: the frontend composition edge owns
            // the real window, so only a product that carries the capability
            // can run it.
            let capture = handlers.capture_window.unwrap_or(|_| {
                Err("--capture-window is a Windows GPUI evidence mode; run the \
                     taskforest-g product on Windows"
                    .to_owned())
            });
            if let Err(error) = capture(&out) {
                eprintln!("{binary_name} --capture-window: {error}");
                std::process::exit(1);
            }
        }
        CliMode::Gui { app_id, demo } => {
            // Tracing initializes only on the GUI path: the JSON snapshot mode
            // owns stdout exclusively. Color follows the NO_COLOR convention
            // and the writer's terminal-ness, so redirected capture logs are
            // plain text and receipt validators can grep field provenance
            // (e.g. backend="spectacle-active-window") verbatim.
            let ansi = std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
                && std::io::stdout().is_terminal();
            let _ = tracing::subscriber::set_global_default(
                tracing_subscriber::FmtSubscriber::builder()
                    .with_max_level(tracing::Level::INFO)
                    .with_ansi(ansi)
                    .finish(),
            );
            (handlers.run_gui)(app_id, demo);
        }
    }
}

fn write_usage(
    writer: &mut impl io::Write,
    binary_name: &str,
    capabilities: CliCapabilities,
    prefix: Option<&str>,
) -> io::Result<()> {
    if let Some(prefix) = prefix {
        writeln!(writer, "{prefix}")?;
    }
    cli::print_help_to(writer, binary_name, capabilities)?;
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/logic/main_tests.rs"]
mod tests;
