//! GPUI-only toolkit event loop over the shared native app-host seam.

use gpui::{App, Application};
use taskmanager_app_host::NativeAppHost;
use taskmanager_assets::product;
use tracing::{error, info};

use crate::assets::TaskManagerAssets;

/// Launch the GPUI desktop frontend.
pub fn run(app_id: Option<String>, demo: bool) {
    if demo {
        run_demo(app_id);
        return;
    }
    run_production(app_id);
}

fn run_demo(app_id: Option<String>) {
    info!(
        frontend = product::GPUI_NAME,
        "Starting TaskForest demo frontend..."
    );
    Application::new()
        .with_assets(TaskManagerAssets)
        .run(move |cx: &mut App| {
            crate::gpui_app::init_demo(cx, app_id);
        });
}

fn run_production(app_id: Option<String>) {
    info!(
        frontend = product::GPUI_NAME,
        "Starting TaskForest frontend..."
    );
    let host = NativeAppHost::production();
    let config_client = match host.config_client() {
        Ok(client) => client,
        Err(error) => {
            error!(%error, detail = error.detail(), "configuration runtime unavailable");
            return;
        }
    };
    let snapshot_export_client = match host.snapshot_export_client() {
        Ok(client) => client,
        Err(error) => {
            error!(%error, detail = error.detail(), "snapshot export runtime unavailable");
            return;
        }
    };
    let window_capture_client = match host.window_capture_client() {
        Ok(client) => client,
        Err(error) => {
            error!(%error, detail = error.detail(), "window capture runtime unavailable");
            return;
        }
    };
    let diagnostic_bundle_client = match host.diagnostic_bundle_client() {
        Ok(client) => client,
        Err(error) => {
            error!(%error, detail = error.detail(), "diagnostic bundle runtime unavailable");
            return;
        }
    };
    let service_log_export_client = match host.diagnostic_bundle_client() {
        Ok(client) => client,
        Err(error) => {
            error!(%error, detail = error.detail(), "service log export runtime unavailable");
            return;
        }
    };
    let native_locale_name = host.native_locale_name();
    let local_time_rules = host.local_time_rules();
    let platform_factory = host.clone();
    let history_connector = host.history_frontend_connector();

    taskmanager_app_host::register_in_process_capture(Box::new(|output| {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        gpui::request_window_frame_capture_with_channel(output.to_path_buf(), tx);
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(res) => res,
            Err(e) => Err(format!("in-process capture timed out: {e}")),
        }
    }));

    Application::new()
        .with_assets(TaskManagerAssets)
        .run(move |cx: &mut App| {
            if let Err(composition_error) = crate::gpui_app::init(
                cx,
                move || platform_factory.spawn_client(),
                crate::gpui_app::StartupRuntime {
                    config_client,
                    snapshot_export_client,
                    window_capture_client,
                    diagnostic_bundle_client,
                    service_log_export_client,
                    history_connector,
                },
                crate::gpui_app::StartupEnvironment {
                    native_locale_name,
                    local_time_rules,
                    custom_app_id: app_id,
                    presentation: crate::window_presentation::from_environment(),
                },
            ) {
                error!(%composition_error, "native platform composition failed");
                cx.quit();
            }
        });
}
