//! # taskforest-daemon
//!
//! Background continuous telemetry collection and persistence daemon for TaskForest.
//!
//! Provides ADR-055 single-writer history persistence, on-demand sampling, and
//! IPC streaming over Unix Domain Sockets or Named Pipes.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use taskmanager_daemon::config::{DaemonConfig, default_daemon_history_dir};
use taskmanager_daemon::persistence::DaemonHistoryManager;
use taskmanager_daemon::service::DaemonService;
use taskmanager_ipc::transport::IpcEndpoint;

const HELP: &str = "\
TaskForest Background Daemon

USAGE:
    taskforest-daemon [OPTIONS]

OPTIONS:
    -s, --socket <PATH>         Custom Unix Domain Socket path (Linux/macOS)
    -d, --history-dir <PATH>    Custom directory for historical metrics persistence
    -i, --interval <MS>         Active sampling cadence in milliseconds (default: 1000)
    --idle-interval <MS>        Background idle cadence in milliseconds (default: 10000)
    -u, --user                  Run in unprivileged user-session mode
    -v, --version               Print version information
    -h, --help                  Print help information
";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = pico_args::Arguments::from_env();

    if args.contains(["-h", "--help"]) {
        print!("{HELP}");
        return Ok(());
    }

    if args.contains(["-v", "--version"]) {
        println!("taskforest-daemon {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let is_user_mode = args.contains(["-u", "--user"]);
    let custom_socket: Option<PathBuf> = args.opt_value_from_str(["-s", "--socket"])?;
    let custom_history: Option<PathBuf> = args.opt_value_from_str(["-d", "--history-dir"])?;
    let active_interval_ms: u32 = args
        .opt_value_from_str(["-i", "--interval"])?
        .unwrap_or(1000);
    let idle_interval_ms: u32 = args
        .opt_value_from_str("--idle-interval")?
        .unwrap_or(10_000);

    let endpoint = if let Some(path) = custom_socket {
        IpcEndpoint::unix(path)
    } else {
        IpcEndpoint::default_platform()
    };

    let history_dir = custom_history.unwrap_or_else(|| default_daemon_history_dir(is_user_mode));

    println!(
        "Starting taskforest-daemon v{}...",
        env!("CARGO_PKG_VERSION")
    );
    println!("Endpoint: {endpoint:?}");
    println!("History:  {}", history_dir.display());

    let mut config = DaemonConfig::new(endpoint, history_dir.clone());
    config.active_interval_ms = active_interval_ms;
    config.idle_interval_ms = idle_interval_ms;

    let history = match DaemonHistoryManager::open(&history_dir) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Failed to acquire history single-writer lock: {e}");
            std::process::exit(1);
        }
    };

    let service = Arc::new(DaemonService::new(config, history));
    let service_for_signal = Arc::clone(&service);

    // Register Ctrl+C / SIGINT / SIGTERM handler
    let _ = ctrlc::set_handler(move || {
        println!("\nReceived termination signal, shutting down gracefully...");
        service_for_signal.stop();
    });

    let handle = Arc::clone(&service).start_on_thread()?;
    while service.is_running() {
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = handle.join();

    println!("taskforest-daemon stopped.");
    Ok(())
}
