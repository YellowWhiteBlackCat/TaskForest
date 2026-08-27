//! Iced frontend launcher (ADR-029): the composition edge of the iced shape.
//!
//! The shared `taskmanager-app-host` is the only native composition seam: its
//! client is handed to this frontend through the application port, mirroring
//! the GPUI and TUI launchers. `demo` runs fixture data with no host I/O.
//! The single `taskmanager` binary calls this under the `ui-iced` feature.

use std::cell::RefCell;
use std::sync::mpsc::{Receiver, channel};

use iced::Task;
use taskmanager_app_host::NativeAppHost;
use taskmanager_assets::product;

use crate::{IcedApp, Message, ui};

type InstanceLease = (
    Option<Box<dyn taskmanager_app_host::InstanceGuard>>,
    Option<Receiver<taskmanager_app_host::InstanceEvent>>,
);

/// The same two viewport contracts used by the GPUI frontend: a spacious
/// desktop layout and the minimum compact layout that must keep controls
/// reachable. `TM_ICED_WINDOW_SIZE` is capture-only; ordinary launches use the
/// desktop size and then follow real resize events.
pub(crate) fn initial_window_size() -> iced::Size {
    std::env::var("TM_ICED_WINDOW_SIZE")
        .ok()
        .and_then(|value| {
            let (width, height) = value.split_once('x')?;
            let width = width.parse::<f32>().ok()?;
            let height = height.parse::<f32>().ok()?;
            (width.is_finite() && height.is_finite() && width >= 720.0 && height >= 480.0)
                .then_some(iced::Size::new(width, height))
        })
        .unwrap_or_else(|| iced::Size::new(1180.0, 780.0))
}

fn platform_specific_settings() -> iced::window::settings::PlatformSpecific {
    #[cfg(target_os = "linux")]
    {
        iced::window::settings::PlatformSpecific {
            application_id: product::ICED_APP_ID.to_string(),
            ..Default::default()
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Iced 0.14 exposes application_id only for its Linux window
        // settings. Windows/macOS use their native window identity paths;
        // keeping this branch typed and empty avoids pretending that a Linux
        // setting is portable across platforms.
        Default::default()
    }
}

/// Acquire the Iced product's singleton before constructing a native window.
/// `None` is the typed secondary-launch result: the platform adapter has
/// already sent `Activate` to the primary, so the process must exit without
/// creating a second window or tray icon.
fn acquire_instance(demo: bool) -> Option<InstanceLease> {
    if demo {
        return Some((None, None));
    }

    let (sender, receiver) = channel();
    match taskmanager_app_host::acquire_single_instance(product::ICED_NAME, sender) {
        Ok(taskmanager_app_host::InstanceRole::Primary(guard)) => {
            Some((Some(guard), Some(receiver)))
        }
        Ok(taskmanager_app_host::InstanceRole::Secondary) => None,
        Err(failure) => {
            eprintln!(
                "taskforest-i: single-instance unavailable ({failure}); continuing without the guard"
            );
            Some((None, None))
        }
    }
}

/// Run the iced frontend to completion. `demo` skips the native runtime and
/// renders fixture data (no host I/O, no host actions).
pub fn run(demo: bool) -> iced::Result {
    let Some((instance_guard, instance_rx)) = acquire_instance(demo) else {
        return Ok(());
    };
    let host = NativeAppHost::production();
    let platform = if demo {
        None
    } else {
        match host.spawn_client() {
            Ok(client) => Some(client),
            Err(error) => {
                eprintln!("taskmanager (ui-iced): native platform composition failed: {error}");
                std::process::exit(1);
            }
        }
    };
    let initial_platform = RefCell::new(Some(platform));
    let local_time_rules = host.local_time_rules();
    let initial_instance_guard = RefCell::new(instance_guard);
    let initial_instance_rx = RefCell::new(instance_rx);

    let builder = iced::application(
        // The boot closure is required to be `Fn`; the platform client is
        // handed over exactly once through interior mutability.
        move || {
            if demo {
                let app = IcedApp::demo_for_capture();
                // The demo boot skips `load_config`; pin the shared catalog to
                // the demo's language here (see `i18n::sync_shared_language`).
                crate::i18n::sync_shared_language(app.language());
                (app, Task::none())
            } else {
                let platform = initial_platform.borrow_mut().take().flatten();
                let (config_client, config_error) = match host.config_client() {
                    Ok(client) => (Some(client), None),
                    Err(error) => (None, Some(error)),
                };
                let mut app = IcedApp::new_with_native_runtime_clients(
                    platform,
                    config_client,
                    None,
                    local_time_rules.clone(),
                );
                if let Some(error) = config_error {
                    app.shell.report_notice(
                        taskmanager_shell::FeedbackSource::Settings,
                        taskmanager_shell::FeedbackSeverity::Error,
                        taskmanager_shell::FeedbackLifecycle::UntilReplaced,
                        format!("Configuration runtime unavailable: {error}"),
                    );
                }
                match host.snapshot_export_client() {
                    Ok(client) => app.install_snapshot_export_client(client),
                    Err(error) => app.shell.report_notice(
                        taskmanager_shell::FeedbackSource::Persistence,
                        taskmanager_shell::FeedbackSeverity::Error,
                        taskmanager_shell::FeedbackLifecycle::UntilReplaced,
                        format!("Snapshot export runtime unavailable: {error}"),
                    ),
                }
                match host.diagnostic_bundle_client() {
                    Ok(client) => app.install_service_log_export_client(client),
                    Err(error) => app.shell.report_notice(
                        taskmanager_shell::FeedbackSource::Persistence,
                        taskmanager_shell::FeedbackSeverity::Error,
                        taskmanager_shell::FeedbackLifecycle::UntilReplaced,
                        format!("Diagnostic bundle runtime unavailable: {error}"),
                    ),
                }
                // Restore the persisted appearance preferences (skin, mode,
                // contrast, fonts, density) before the first frame renders.
                app.load_config();
                app.activate_history_replay_for_boot();
                app.install_history_frontend_connector(host.history_frontend_connector());
                app.install_instance_runtime(
                    initial_instance_guard.borrow_mut().take(),
                    initial_instance_rx.borrow_mut().take(),
                );
                // Spawn after language/config restoration so tray labels and
                // the first rendered frame use the same locale. A typed native
                // failure leaves a normal window-only Iced app.
                crate::tray::spawn_tray_host(&mut app);
                // Ask the OS for its CURRENT color scheme: `theme_changes`
                // only reports subsequent changes, and a `System` mode
                // preference should follow the desktop from the first frame.
                (app, crate::app::appearance::initial_query())
            }
        },
        // OS color-scheme observations are reduced by `app::appearance`
        // ahead of the domain router (the router's arm for the variant only
        // satisfies exhaustive routing). The observation carries no input, so
        // it legitimately bypasses the input-driven lifecycle envelope.
        move |state: &mut IcedApp, message: Message| match message {
            Message::SystemThemeChanged(mode) => {
                crate::app::appearance::reduce_system_theme_change(state, mode);
                Task::none()
            }
            message => state.update(message),
        },
        ui::view,
    )
    .title(|_: &IcedApp| product::ICED_NAME.to_string())
    .theme(|state: &IcedApp| crate::theme::iced_theme(state.theme()))
    .subscription(|state: &IcedApp| {
        iced::Subscription::batch([state.subscription(), crate::app::appearance::subscription()])
    })
    // User interface size is an explicit product zoom layered on top of the
    // compositor DPI factor by iced/winit. It does not rewrite row density.
    .scale_factor(|state: &IcedApp| state.ui_size().renderer_scale())
    // Native close requests are reduced by `Message::WindowCloseRequested`:
    // a live tray minimizes the only window, while a tray-less build closes
    // it explicitly. This keeps the process lifetime deterministic.
    .exit_on_close_request(false)
    .window(iced::window::Settings {
        platform_specific: platform_specific_settings(),
        ..Default::default()
    })
    .window_size(initial_window_size())
    .default_font(crate::theme::BUNDLED_UI_FONT);

    // Register the same bundled faces GPUI embeds (ADR-026 fonts policy) so the
    // resolved UI/mono families exist in iced's font database: MiSans VF (the
    // UI default) and Roboto Mono (the metrics/mono face). iced's
    // default `Shaping::Auto` then routes any non-ASCII text through cosmic-text
    // Advanced shaping with font fallback, so zh strings pick up MiSans VF
    // glyphs automatically — no per-site shaping call is needed (iced compiles
    // the Advanced path unconditionally; only the default variant is
    // feature-gated, and the unpinned default is `Auto`).
    let builder = taskmanager_assets::embedded_fonts()
        .into_iter()
        .fold(builder, |builder, font_bytes| builder.font(font_bytes));

    builder.run()
}

#[cfg(test)]
#[path = "../tests/gui/run_tests.rs"]
mod tests;
