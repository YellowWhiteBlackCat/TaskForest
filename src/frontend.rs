//! Feature-scoped frontend composition.
//!
//! The binary entry point stays shared, while each toolkit's composition edge
//! lives in its own file. This is both clearer ownership and evidence hygiene:
//! a GPUI-only edit cannot make an Iced/TUI pixel receipt stale.

#[cfg(feature = "ui-iced")]
mod iced;
#[cfg(feature = "ui-tui")]
mod tui;

/// Launch the one frontend selected by the compile-time UI feature.
pub fn run(app_id: Option<String>, demo: bool) {
    #[cfg(not(feature = "ui-gpui"))]
    let _ = app_id;

    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(tracing::Level::INFO)
            .finish(),
    );

    #[cfg(feature = "ui-gpui")]
    taskmanager_gpui::run(app_id, demo);
    #[cfg(feature = "ui-tui")]
    tui::run(demo);
    #[cfg(feature = "ui-iced")]
    iced::run(demo);
}

/// Run the `--capture-window` evidence mode against the compiled-in frontend.
/// Only the GPUI shape on Windows implements it (it owns the real window);
/// every other shape reports the mode as unsupported.
pub fn run_capture(out: &std::path::Path) -> Result<(), String> {
    #[cfg(all(feature = "ui-gpui", target_os = "windows"))]
    {
        taskmanager_gpui::capture::run(out)
    }
    #[cfg(not(all(feature = "ui-gpui", target_os = "windows")))]
    {
        let _ = out;
        Err("--capture-window is a Windows+GPUI evidence mode; build the ui-gpui shape on Windows to use it".to_owned())
    }
}
