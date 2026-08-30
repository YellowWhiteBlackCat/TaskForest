//! Exhaustive rendering system for the one active GPUI window surface.

use super::super::window_surface::WindowSurface;
use super::super::{
    DetailsPanelProps, ProcessDetailsSection, RootView, WindowSurfaceDismissReason,
    WindowSurfaceKind, batch_process, details_panel_content, diagnostic_bundle, elements, i18n,
    perf_views, service_control, services_view, system_health, termination,
};
use super::overlays;
use gpui::{AnyElement, App, Context, Div, IntoElement, ParentElement, Stateful, Window, px};
use taskmanager_application::{
    PendingConfirmation, ProcessTerminationAction, ProcessTerminationConfirmation,
    SurfaceDismissReason, SurfaceKind,
};
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_theme::Theme;

use taskmanager_ui::layout::{BoundedScrollRailSpec, bounded_scroll_region_with_rail};

pub(super) fn compose_active_surface(
    view: &mut RootView,
    root: Stateful<Div>,
    theme: &Theme,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    let close_entity = cx.entity();
    if let Some(kind) = view.shell.interaction.kind() {
        return compose_shared_surface(view, root, theme, kind, close_entity, window, cx);
    }
    match view.active_window_surface().cloned() {
        None => root,
        Some(
            surface @ (WindowSurface::Settings
            | WindowSurface::Help
            | WindowSurface::SystemAbout
            | WindowSurface::About
            | WindowSurface::FirstRun
            | WindowSurface::RunTask
            | WindowSurface::DashboardPanel(_)),
        ) => overlays::compose_primary_dialogs(view, root, &surface, theme, window, cx),
        Some(WindowSurface::DiagnosticBundle(state)) => {
            root.child(diagnostic_bundle::render_diagnostic_bundle_dialog(
                theme,
                state,
                close_entity.clone(),
                view.dialog_scroll.diagnostic_preview.clone(),
                window,
                cx,
            ))
        }
        Some(WindowSurface::ServiceDetails(service_id)) => {
            render_service_details(view, root, theme, service_id, window, cx)
        }
        Some(WindowSurface::DiskSmart(index)) => {
            render_disk_smart(view, root, theme, index, window, cx)
        }
        Some(WindowSurface::ProcessAffinity(_)) => root,
    }
}

fn compose_shared_surface(
    view: &mut RootView,
    root: Stateful<Div>,
    theme: &Theme,
    kind: SurfaceKind,
    close_entity: gpui::Entity<RootView>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    match kind {
        SurfaceKind::ProcessProperties => {
            let Some(identity) = view.process_properties_identity() else {
                return root;
            };
            render_process_properties(view, root, theme, identity, window, cx)
        }
        SurfaceKind::Confirmation(_) => match view.pending_confirmation().cloned() {
            Some(PendingConfirmation::EndTask(target)) => {
                root.child(termination::render_process_termination_dialog(
                    theme,
                    ProcessTerminationConfirmation {
                        action: ProcessTerminationAction::EndTask,
                        root: target,
                        descendants_leaf_first: Vec::new(),
                    },
                    close_entity,
                    window,
                    cx,
                ))
            }
            Some(PendingConfirmation::ProcessTermination(intent)) => {
                root.child(termination::render_process_termination_dialog(
                    theme,
                    intent,
                    close_entity,
                    window,
                    cx,
                ))
            }
            Some(PendingConfirmation::ProcessBatch(intent)) => {
                root.child(batch_process::render_process_batch_dialog(
                    theme,
                    intent,
                    close_entity,
                    view.dialog_scroll.process_batch.clone(),
                    window,
                    cx,
                ))
            }
            Some(
                PendingConfirmation::ServiceControl(_) | PendingConfirmation::StartupControl(_),
            ) => {
                let Some(intent) = service_control::confirmation_dialog(view) else {
                    return root;
                };
                root.child(service_control::render_service_control_confirmation_dialog(
                    theme,
                    intent,
                    close_entity,
                    window,
                    cx,
                ))
            }
            Some(PendingConfirmation::SmartSelfTest(intent)) => {
                let error = match view.shell.smart_self_test_state() {
                    taskmanager_application::SmartSelfTestState::Failed(failed)
                        if failed.intent == intent =>
                    {
                        Some(failed.failure)
                    }
                    _ => None,
                };
                root.child(system_health::render_system_health_confirmation_dialog(
                    theme,
                    intent,
                    error,
                    close_entity,
                    window,
                    cx,
                ))
            }
            // GPUI currently offers no login-session destructive menu. The
            // branch is still swallowed by the shared input scope; adding the
            // menu requires a renderer in the same change before it may arm.
            Some(PendingConfirmation::SessionControl(_)) | None => root,
        },
    }
}

fn render_process_properties(
    view: &mut RootView,
    root: Stateful<Div>,
    theme: &Theme,
    identity: ProcessLiveKey,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    let frozen_start_token = view
        .process_properties_target()
        .and_then(|target| target.authoritative_start_token());
    let target = view
        .process_details_target(identity)
        .filter(|(item, _)| item.current_start_token() == frozen_start_token);
    let Some((item, histories)) = target else {
        view.dismiss_shared_surface(
            SurfaceKind::ProcessProperties,
            SurfaceDismissReason::TargetUnavailable,
        );
        return root;
    };
    let close_entity = cx.entity();
    let close = close_entity.clone();
    let on_close = move |_window: &mut Window, cx: &mut App| {
        close.update(cx, |view, cx| {
            view.dismiss_shared_surface(
                SurfaceKind::ProcessProperties,
                SurfaceDismissReason::CloseButton,
            );
            cx.notify();
        });
    };
    let viewport = window.viewport_size();
    let max_dialog_width = (f32::from(viewport.width) - 80.0).max(320.0);
    let dialog_width = if view.details_section == ProcessDetailsSection::Insights {
        max_dialog_width.min(900.0)
    } else {
        max_dialog_width.min(480.0)
    };
    let content_width = (dialog_width - 50.0).max(270.0);
    let content_height = (f32::from(viewport.height) - 150.0).max(260.0);
    let content: AnyElement = bounded_scroll_region_with_rail(
        BoundedScrollRailSpec {
            id: "process-properties-scroll",
            viewport_selector: "tm-process-properties-scroll",
            scrollbar_id: "process-properties-scrollbar",
            scrollbar_selector: "tm-process-properties-scrollbar",
            track_selector: "tm-process-properties-scrollbar-track",
            width: Some(px(content_width)),
            max_height: px(content_height),
            scroll: view.dialog_scroll.process_details.clone(),
            palette: theme.palette(),
        },
        details_panel_content(DetailsPanelProps {
            t: theme,
            item: &item,
            histories: &histories,
            active: view.details_section,
            insights: view.process_insights.render_state(),
            available_width: content_width,
            net_escalation: *view.shell.network_escalation_state(),
            entity: close_entity,
            local_time_rules: &view.local_time_rules,
            units: view.display_units(),
        }),
    )
    .into_any_element();
    root.child(elements::dialog_overlay_width(
        theme,
        window,
        cx,
        px(dialog_width),
        i18n::t("dialog.properties"),
        on_close,
        content,
    ))
}

fn render_service_details(
    view: &mut RootView,
    root: Stateful<Div>,
    theme: &Theme,
    service_id: taskmanager_core::core::target::ServiceId,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    let Some(item) = view
        .services()
        .iter()
        .find(|service| service.id == service_id)
        .cloned()
    else {
        view.dismiss_window_surface(
            WindowSurfaceKind::ServiceDetails,
            WindowSurfaceDismissReason::TargetUnavailable,
        );
        return root;
    };
    let close = cx.entity();
    let on_close = move |_window: &mut Window, cx: &mut App| {
        close.update(cx, |view, cx| {
            view.dismiss_window_surface(
                WindowSurfaceKind::ServiceDetails,
                WindowSurfaceDismissReason::CloseButton,
            );
            cx.notify();
        });
    };
    let content_height = (f32::from(window.viewport_size().height) - 150.0).max(260.0);
    let content: AnyElement = bounded_scroll_region_with_rail(
        BoundedScrollRailSpec {
            id: "service-details-scroll",
            viewport_selector: "tm-service-details-scroll",
            scrollbar_id: "service-details-scrollbar",
            scrollbar_selector: "tm-service-details-scrollbar",
            track_selector: "tm-service-details-scrollbar-track",
            width: Some(px(430.0)),
            max_height: px(content_height),
            scroll: view.dialog_scroll.service_details.clone(),
            palette: theme.palette(),
        },
        services_view::render_details(
            theme,
            &item,
            view.service_details
                .details_for(&service_id, &view.shell.service_dependencies),
            view.service_log_now_ms.saturating_mul(1_000),
            cx,
        ),
    )
    .into_any_element();
    root.child(elements::dialog_overlay(
        theme,
        window,
        cx,
        i18n::t("dialog.service_details"),
        on_close,
        content,
    ))
}

fn render_disk_smart(
    view: &mut RootView,
    root: Stateful<Div>,
    theme: &Theme,
    index: usize,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    let Some(disk) = view
        .system_snapshot()
        .disks
        .get(index)
        .filter(|disk| disk.smart_temperature_c.is_some())
        .cloned()
    else {
        view.dismiss_window_surface(
            WindowSurfaceKind::DiskSmart,
            WindowSurfaceDismissReason::TargetUnavailable,
        );
        return root;
    };
    let close = cx.entity();
    let on_close = move |_window: &mut Window, cx: &mut App| {
        close.update(cx, |view, cx| {
            view.dismiss_window_surface(
                WindowSurfaceKind::DiskSmart,
                WindowSurfaceDismissReason::CloseButton,
            );
            cx.notify();
        });
    };
    let content: AnyElement = perf_views::render_smart_dialog(theme, &disk).into_any_element();
    root.child(elements::dialog_overlay(
        theme,
        window,
        cx,
        i18n::t("dialog.smart_health"),
        on_close,
        content,
    ))
}
