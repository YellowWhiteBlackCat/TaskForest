//! Capture readiness follows explicitly queried mounted and painted surfaces.
use super::CaptureMarkerState;
use crate::app::{FrontendTrack, Page, PageContent, Route};
use crate::capture::{capture_page_name, capture_perf_device_target, capture_scenario_target};
use crate::confirmation::ConfirmationCapture;
use crate::focus_visible::FocusCapture;
use crate::input_contract::{SemanticAddress, stable_semantic_address};
use crate::navigation::{NavigationState, NavigationStrip, RAIL_WIDTH};
use crate::pages::history::control::{
    HistoryCommand, PerformanceHistoryProjectionResource, PerformancePresentation,
};
use crate::pages::history::{HistoryProjectionResource, HistoryRuntime};
use crate::pages::performance::device_curves::DeviceCurve;
use crate::pages::performance::replay::{ChartSize, ReplayChart};
use crate::pages::performance::{
    DeviceViewCategory, DynBlock, PerformanceDeviceFocus, PerformanceDeviceTarget,
};
use crate::pages::process_tree::ProcessTreeRowMarker;
use crate::pages::processes::properties_modal::PropertiesCapture;
use crate::pages::system::dashboard::SystemDashboardCurve;
use crate::pages::system::dashboard::SystemDashboardState;
use crate::pages::system::{MemoryInventoryAnchor, SystemBody};
use crate::widgets::chart::CurveMeasurement;
use crate::window_surface::{ModalBody, WindowSurface, WindowSurfaceState};
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::{With, Without};
use bevy::ecs::system::{Commands, NonSend, Query, Res, ResMut, SystemParam};
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, ScrollPosition, UiGlobalTransform};
use taskmanager_application::i18n::t;
use taskmanager_application::system_timeline::SystemPageSection;
use taskmanager_shell::presentation::health_review::HealthReviewSection;
use taskmanager_ui_contract::navigation::NavOrientation;

type TreeViewportFilter = (With<ModalBody>, Without<SystemBody>);

#[derive(SystemParam)]
pub(super) struct CaptureAccess<'w, 's> {
    track: NonSend<'w, FrontendTrack>,
    window_surface: Res<'w, WindowSurfaceState>,
    saved_views: Res<'w, crate::saved_views::SavedViewsState>,
    navigation: Res<'w, NavigationState>,
    nav_strip: Query<'w, 's, &'static ComputedNode, With<NavigationStrip>>,
    system_state: Res<'w, SystemDashboardState>,
    device_focus: Res<'w, PerformanceDeviceFocus>,
    device_categories: Query<'w, 's, (&'static DeviceViewCategory, &'static ComputedNode)>,
    device_blocks: Query<'w, 's, (&'static DynBlock, &'static ComputedNode)>,
    device_curves: Query<
        'w,
        's,
        (
            &'static DeviceCurve,
            &'static ComputedNode,
            &'static CurveMeasurement,
        ),
    >,
    route: Res<'w, Route>,
    state: ResMut<'w, CaptureMarkerState>,
    pages: Query<'w, 's, &'static PageContent>,
    text: Query<'w, 's, (&'static Text, &'static ComputedNode)>,
    focus: FocusCapture<'w, 's>,
    confirmation: ConfirmationCapture<'w, 's>,
    properties: PropertiesCapture<'w, 's>,
    anchors: Query<
        'w,
        's,
        (&'static ComputedNode, &'static UiGlobalTransform),
        With<MemoryInventoryAnchor>,
    >,
    tree_rows: Query<
        'w,
        's,
        (
            &'static SemanticAddress,
            &'static ComputedNode,
            &'static UiGlobalTransform,
        ),
        With<ProcessTreeRowMarker>,
    >,
    tree_bodies: Query<
        'w,
        's,
        (
            &'static ComputedNode,
            &'static UiGlobalTransform,
            &'static mut ScrollPosition,
        ),
        TreeViewportFilter,
    >,
    bodies: Query<
        'w,
        's,
        (
            &'static ComputedNode,
            &'static UiGlobalTransform,
            &'static mut ScrollPosition,
        ),
        With<SystemBody>,
    >,
    dashboard: Query<
        'w,
        's,
        (
            &'static SystemDashboardCurve,
            &'static CurveMeasurement,
            &'static ComputedNode,
        ),
    >,
    replay: Query<
        'w,
        's,
        (
            &'static ReplayChart,
            &'static ChartSize,
            Option<&'static Children>,
        ),
    >,
    history_runtime: NonSend<'w, HistoryRuntime>,
    performance: Res<'w, PerformancePresentation>,
    performance_history: Res<'w, PerformanceHistoryProjectionResource>,
    application_history: Res<'w, HistoryProjectionResource>,
    commands: Commands<'w, 's>,
}

pub(super) fn emit_capture_marker(mut access: CaptureAccess) {
    let route = access.route.page;
    if access.state.emitted || access.pages.iter().all(|content| content.page != route) {
        return;
    }
    if let Some(target) = capture_perf_device_target(&access.track.shell) {
        if access.device_focus.0 != target
            || !access.device_categories.iter().any(|(category, node)| {
                category.0 == target.category() && node.size().x > 0.0 && node.size().y > 0.0
            })
        {
            return;
        }
        let device_id = match &target {
            PerformanceDeviceTarget::Disk(id)
            | PerformanceDeviceTarget::Network(id)
            | PerformanceDeviceTarget::Gpu(id)
            | PerformanceDeviceTarget::Battery(id) => Some(id),
            _ => None,
        };
        if device_id.is_some_and(|id| {
            !access
                .device_blocks
                .iter()
                .any(|(block, node)| &block.1 == id && node.size().x > 0.0 && node.size().y > 0.0)
        }) {
            return;
        }
        if let PerformanceDeviceTarget::Battery(id) = &target {
            let painted = access
                .device_curves
                .iter()
                .filter(|(curve, node, measurement)| {
                    &curve.id == id
                        && node.size().x > 0.0
                        && node.size().y > 0.0
                        && measurement.0.is_some()
                        && curve
                            .samples(&access.track.shell)
                            .iter()
                            .filter(|sample| sample.is_finite())
                            .count()
                            >= 8
                })
                .count();
            if painted != 2 {
                return;
            }
        }
        if !access.state.data_presented {
            access.state.data_presented = true;
            return;
        }
    }
    match capture_scenario_target() {
        Some("apps-group-expanded") => {
            let projection = access.track.shell.projection();
            let rows = crate::pages::process_tree::project_items(
                projection.processes_slice(),
                &access.track.process_tree_expansion,
                projection.processes_observed_at_ms,
            );
            if rows.iter().filter(|row| row.item.is_some()).count() < 7
                || !rows.iter().any(|row| row.depth >= 3)
                || rows.iter().any(|row| row.has_children && !row.expanded)
            {
                return;
            }
            let Some(target) = rows
                .iter()
                .find(|row| row.item.is_some_and(|item| item.pid == 90_003))
                .map(|row| stable_semantic_address("process-tree", &row.semantic_key))
            else {
                return;
            };
            let Some((_, node, position)) = access
                .tree_rows
                .iter()
                .find(|(address, _, _)| address.0 == target)
            else {
                return;
            };
            let (size, center) = (node.size(), position.translation);
            if size.y <= 0.0 {
                return;
            }
            if !access.state.tree_scroll_requested {
                for (body, position, mut scroll) in &mut access.tree_bodies {
                    let maximum = ((body.content_size().y - body.size().y)
                        * body.inverse_scale_factor())
                    .max(0.0);
                    let delta = (center.y - size.y / 2.0 - position.translation.y
                        + body.size().y / 2.0)
                        * body.inverse_scale_factor();
                    scroll.0.y = (scroll.0.y + delta).clamp(0.0, maximum);
                }
                access.state.tree_scroll_requested = true;
                return;
            }
            if !access.tree_bodies.iter().any(|(body, position, _)| {
                center.y - size.y / 2.0 >= position.translation.y - body.size().y / 2.0 - 0.5
                    && center.y + size.y / 2.0 <= position.translation.y + body.size().y / 2.0 + 0.5
            }) {
                return;
            }
            if !access.state.data_presented {
                access.state.data_presented = true;
                return;
            }
        }
        Some("event-center") => {
            if !matches!(access.window_surface.0, Some(WindowSurface::EventCenter))
                || access
                    .track
                    .shell
                    .projection()
                    .alert_center
                    .event_history()
                    .len()
                    < 2
                || ![
                    "events.title",
                    "events.activated",
                    "events.cleared",
                    "common.export",
                    "common.close",
                ]
                .iter()
                .all(|key| {
                    access.text.iter().any(|(text, node)| {
                        text.0 == t(key) && node.size().x > 0.0 && node.size().y > 0.0
                    })
                })
            {
                return;
            }
            if !access.state.data_presented {
                access.state.data_presented = true;
                return;
            }
        }
        Some("vertical-nav") => {
            if access.navigation.0 != NavOrientation::Vertical
                || !access.nav_strip.iter().any(|node| {
                    node.size().x > 0.0
                        && node.size().x <= RAIL_WIDTH + 0.5
                        && node.size().y > node.size().x
                })
            {
                return;
            }
            if !access.state.data_presented {
                access.state.data_presented = true;
                return;
            }
        }
        Some("saved-view-presets") => {
            if !matches!(access.window_surface.0, Some(WindowSurface::SavedViews))
                || !access
                    .saved_views
                    .rows
                    .iter()
                    .any(|row| row.is_user_saved())
                || ![
                    "saved_views.title",
                    "saved_views.save_current",
                    "common.apply",
                    "common.remove",
                    "common.export",
                    "common.import",
                    "common.close",
                ]
                .iter()
                .all(|key| {
                    access.text.iter().any(|(text, node)| {
                        text.0 == t(key) && node.size().x > 0.0 && node.size().y > 0.0
                    })
                })
            {
                return;
            }
            if !access.state.data_presented {
                access.state.data_presented = true;
                return;
            }
        }
        Some("sidebar-edit") => {
            if !matches!(access.window_surface.0, Some(WindowSurface::SidebarDevices))
                || !["Edit devices", "Hide", "Move up", "Move down", "Done"]
                    .iter()
                    .all(|label| {
                        access.text.iter().any(|(text, node)| {
                            text.0 == *label && node.size().x > 0.0 && node.size().y > 0.0
                        })
                    })
            {
                return;
            }
            if !access.state.data_presented {
                access.state.data_presented = true;
                return;
            }
        }

        Some(scenario @ ("storage-health" | "sensor-center")) => {
            let expected = if scenario == "sensor-center" {
                HealthReviewSection::Sensors
            } else {
                HealthReviewSection::Storage
            };
            if route != Page::System
                || access.system_state.section != SystemPageSection::Health
                || access.system_state.health_section != expected
            {
                return;
            }
            let projection = access.track.shell.projection();
            if !projection
                .storage_health_projection()
                .is_some_and(|(snapshot, _)| snapshot.filesystems.len() == 3)
                || !projection
                    .sensors
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.readings.len() == 4)
            {
                return;
            }
            let labels: &[&str] = if scenario == "sensor-center" {
                &["CPU package", "Chassis fan", "Package power"]
            } else {
                &[t("health.read_only"), t("health.errors_reported")]
            };
            if !labels.iter().all(|label| {
                access.text.iter().any(|(text, node)| {
                    node.size().x > 0.0 && node.size().y > 0.0 && text.0.contains(label)
                })
            }) {
                return;
            }
        }
        Some("startup-failure-evidence") => {
            if !access.text.iter().any(|(text, node)| {
                node.size().x > 0.0
                    && node.size().y > 0.0
                    && [
                        "taskforest-g.service",
                        "taskforest-i.service",
                        "taskforest.service",
                    ]
                    .iter()
                    .all(|unit| text.0.contains(unit))
            }) {
                return;
            }
        }
        Some("keyboard-focus")
            if !crate::focus_visible::capture_ready(&mut access.focus, false) =>
        {
            return;
        }
        Some("settings-switch-focus")
            if !crate::focus_visible::capture_ready(&mut access.focus, true) =>
        {
            return;
        }
        Some(
            "process-force-kill"
            | "process-tree-confirm"
            | "process-batch-confirm"
            | "smart-self-test-confirm",
        ) if !crate::confirmation::capture_ready(&access.confirmation) => return,
        Some(
            scenario @ ("process-properties-performance"
            | "process-memory-pss-swap"
            | "process-network-details"
            | "process-gpu-details"
            | "process-resource-limits"
            | "process-isolation"),
        ) if !crate::pages::processes::properties_modal::capture_ready(
            &mut access.properties,
            scenario,
        ) =>
        {
            return;
        }
        Some("system-dashboard" | "history-60m")
            if !crate::pages::system::dashboard::presented(&access.dashboard) =>
        {
            return;
        }
        Some("system-hardware") => {
            let anchor = access
                .anchors
                .iter()
                .next()
                .map(|(node, transform)| (node.size(), transform.translation));
            let Some((size, center)) = anchor.filter(|(size, _)| size.x > 0.0 && size.y > 0.0)
            else {
                return;
            };
            if !access.state.hardware_scroll_requested {
                for (node, transform, mut scroll) in &mut access.bodies {
                    let maximum = ((node.content_size().y - node.size().y)
                        * node.inverse_scale_factor())
                    .max(0.0);
                    let delta = (center.y - size.y / 2.0 - transform.translation.y
                        + node.size().y / 2.0)
                        * node.inverse_scale_factor();
                    scroll.0.y = (scroll.0.y + delta).clamp(0.0, maximum);
                }
                access.state.hardware_scroll_requested = true;
                return;
            }
            if !access.bodies.iter().any(|(node, transform, _)| {
                center.y - size.y / 2.0 >= transform.translation.y - node.size().y / 2.0 - 0.5
                    && center.y + size.y / 2.0
                        <= transform.translation.y + node.size().y / 2.0 + 0.5
            }) {
                return;
            }
        }
        Some("history-replay") => {
            if !access.state.history_open_requested && access.history_runtime.available() {
                access.state.history_open_requested = true;
                access.commands.trigger(HistoryCommand::OpenPerformance);
            }
            if *access.performance != PerformancePresentation::Replay
                || access.performance_history.0.rows.is_empty()
                || !crate::pages::performance::replay::charts_presented(&access.replay)
            {
                return;
            }
        }
        Some("application-history-replay") if access.application_history.0.rows.is_empty() => {
            return;
        }
        _ => {}
    }
    if matches!(
        capture_scenario_target(),
        Some(
            "history-replay"
                | "application-history-replay"
                | "system-hardware"
                | "system-dashboard"
                | "history-60m"
                | "process-properties-performance"
                | "process-memory-pss-swap"
                | "process-network-details"
                | "process-gpu-details"
                | "process-resource-limits"
                | "process-isolation"
        )
    ) && !access.state.data_presented
    {
        access.state.data_presented = true;
        return;
    }
    let page = capture_page_name(route);
    println!("BEVY_CAPTURE_MARKER event=frame_ready mode=demo page={page}");
    println!("BEVY_CAPTURE_MARKER event=target_ready mode=demo page={page}");
    access.state.emitted = true;
}
