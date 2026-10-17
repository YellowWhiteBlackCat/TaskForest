//! Window option mapping and one-shot RootView creation.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::{
    RootStartupFacts, apply_root_startup_config, requested_window_decorations, spawn_update_loop,
};
use crate::gpui_app::chrome::WindowDecorationsPreference;
use crate::gpui_app::root::{RootView, responsive};
use crate::window_presentation::GpuiSurfaceRole;
use crossbeam_channel::Receiver;
use gpui::{
    App, AppContext, Bounds, Entity, TitlebarOptions, Window, WindowBounds, WindowOptions, point,
    px, size,
};
use taskmanager_app_host::{
    DaemonSubscriptionWorker, DiagnosticBundleClient, SnapshotExportClient, WindowCaptureClient,
    WindowPresentation,
};
use taskmanager_application::{
    ConfigClient, ConfigRevision, PlatformClient, TelemetryRefreshPolicy,
};
use taskmanager_assets::product;
use taskmanager_core::core::config::Config;
use taskmanager_ipc::DaemonMessage;
use taskmanager_platform_contract::{InstanceEvent, InstanceGuard};
use taskmanager_telemetry_store::{CorrelatedSystemTelemetryIngestor, TelemetryStore};
use taskmanager_theme::Theme;
use taskmanager_ui::theme_binding::background_appearance;
use tracing::{error, warn};

type RootBuilder = Box<dyn FnOnce(&mut Window, &mut App) -> Entity<RootView>>;

pub(super) struct RootWindowInputs {
    pub theme: Theme,
    pub telemetry: Arc<TelemetryStore>,
    pub telemetry_ingestor: CorrelatedSystemTelemetryIngestor,
    pub telemetry_refresh_policy: TelemetryRefreshPolicy,
    pub platform: PlatformClient,
    pub cfg: Config,
    pub root_startup_facts: RootStartupFacts,
    pub has_explicit_page_override: bool,
    pub snapshot_export_client: SnapshotExportClient,
    pub window_capture_client: WindowCaptureClient,
    pub diagnostic_bundle_client: DiagnosticBundleClient,
    pub service_log_export_client: DiagnosticBundleClient,
    pub instance_guard: Option<Box<dyn InstanceGuard>>,
    pub instance_rx: std::sync::mpsc::Receiver<InstanceEvent>,
    pub config_bootstrap_notice: Option<String>,
    pub config_client: ConfigClient,
    pub applied_config_revision: Option<ConfigRevision>,
    pub capture_mode: bool,
    pub surface_role: GpuiSurfaceRole,
    pub daemon_worker: Option<DaemonSubscriptionWorker>,
    pub daemon_rx: Option<Receiver<DaemonMessage>>,
}

pub(super) fn make_root_builder(inputs: RootWindowInputs) -> RootBuilder {
    let RootWindowInputs {
        theme,
        telemetry,
        telemetry_ingestor,
        telemetry_refresh_policy,
        platform,
        cfg,
        root_startup_facts,
        has_explicit_page_override,
        snapshot_export_client,
        window_capture_client,
        diagnostic_bundle_client,
        service_log_export_client,
        instance_guard,
        instance_rx,
        config_bootstrap_notice,
        config_client,
        applied_config_revision,
        capture_mode,
        surface_role,
        daemon_worker,
        daemon_rx,
    } = inputs;

    Box::new(move |window, cx| {
        window.set_window_title(product::GPUI_NAME);
        let entity: Entity<RootView> = cx.new(|cx| {
            let mut view = RootView::new_with_platform_and_surface_role(
                theme,
                telemetry,
                telemetry_ingestor,
                telemetry_refresh_policy,
                platform,
                surface_role,
                cx,
            );
            view.snapshot_export.install(snapshot_export_client);
            view.window_capture.install(window_capture_client);
            view.diagnostic_bundle_runtime
                .install(diagnostic_bundle_client);
            view.service_details
                .install_export_client(service_log_export_client);
            apply_root_startup_config(
                &mut view,
                &cfg,
                root_startup_facts,
                has_explicit_page_override,
                cx,
            );
            view
        });
        let weak = entity.downgrade();
        entity.update(cx, |view, _cx| {
            if let Some(guard) = instance_guard {
                view.instance_guard = Some(guard);
            }
            view.instance_rx = Some(instance_rx);
            if let (Some(worker), Some(rx)) = (daemon_worker, daemon_rx) {
                view.install_daemon_subscriber(worker, rx);
            }
        });
        if let Some(notice) = config_bootstrap_notice {
            entity.update(cx, |view, cx| view.show_local_feedback(notice, cx));
        }
        if !capture_mode {
            let tray_available = super::super::tray::spawn_tray_host(&entity, cx);
            if tray_available {
                window.on_window_should_close(cx, |window, _cx| {
                    window.minimize_window();
                    false
                });
            }
        }
        let window_handle = window.window_handle();
        spawn_update_loop(
            cx,
            weak,
            window_handle,
            config_client,
            applied_config_revision,
        );
        entity
    })
}

fn main_window_options(
    cfg: &Config,
    theme: &Theme,
    app_id: &str,
    presentation: &WindowPresentation,
    cx: &App,
) -> Result<WindowOptions, String> {
    let mut options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(product::GPUI_NAME.into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(120.0), px(80.0)),
            size: responsive::initial_window_size(),
        })),
        window_decorations: Some(requested_window_decorations(
            WindowDecorationsPreference::from_config_token(&cfg.window_decorations),
        )),
        window_background: background_appearance(theme),
        app_id: Some(app_id.to_owned()),
        window_min_size: Some(size(px(responsive::MIN_WIDTH), px(responsive::MIN_HEIGHT))),
        ..Default::default()
    };
    crate::window_presentation::apply_to_window_options(presentation, &mut options, cx)?;
    #[cfg(target_os = "linux")]
    if matches!(&options.kind, gpui::WindowKind::LayerShell(_)) {
        options.window_min_size = None;
    }
    Ok(options)
}

pub(super) fn prepare_window_options(
    cx: &mut App,
    cfg: &Config,
    theme: &Theme,
    app_id: &str,
    presentation: &WindowPresentation,
) -> Option<(WindowOptions, Option<WindowOptions>)> {
    let options = match main_window_options(cfg, theme, app_id, presentation, cx) {
        Ok(options) => options,
        Err(error) => {
            error!(%error, "GPUI window presentation is unavailable");
            cx.quit();
            return None;
        }
    };
    let fallback_options = if crate::window_presentation::has_normal_window_fallback(presentation) {
        match main_window_options(cfg, theme, app_id, &WindowPresentation::standalone(), cx) {
            Ok(options) => Some(options),
            Err(error) => {
                error!(%error, "normal GPUI fallback presentation is unavailable");
                cx.quit();
                return None;
            }
        }
    } else {
        None
    };
    Some((options, fallback_options))
}

pub(super) fn open_main_window(
    cx: &mut App,
    options: WindowOptions,
    mut fallback_options: Option<WindowOptions>,
    presentation: &WindowPresentation,
    fallback_theme: Theme,
    root_builder: RootBuilder,
) {
    let shared_builder = Rc::new(RefCell::new(Some(root_builder)));
    let shared_entity = Rc::new(RefCell::new(None::<Entity<RootView>>));
    let mut open_window = |options: WindowOptions| {
        let builder = shared_builder.clone();
        let entity = shared_entity.clone();
        cx.open_window(options, move |window, cx| {
            let build = { builder.borrow_mut().take() };
            if let Some(build) = build {
                let root = build(window, cx);
                *entity.borrow_mut() = Some(root.clone());
                root
            } else if let Some(root) = { entity.borrow().clone() } {
                root
            } else {
                cx.quit();
                cx.new(|cx| RootView::new_demo(fallback_theme, cx))
            }
        })
    };
    let initial_result = open_window(options);
    let window_result = match initial_result {
        Err(error)
            if fallback_options.is_some()
                && crate::window_presentation::should_fallback_to_normal_window(
                    presentation,
                    error.as_ref(),
                ) =>
        {
            warn!(%error, "Wayland layer-shell is unavailable; opening a normal GPUI window");
            if let Some(options) = fallback_options.take() {
                open_window(options)
            } else {
                Err(error)
            }
        }
        result => result,
    };
    drop(open_window);
    if let Err(error) = window_result {
        error!(%error, "main GPUI window failed to open");
        cx.quit();
    }
    cx.activate(true);
}
