//! Capture readiness follows explicitly queried mounted and painted surfaces.
use super::CaptureMarkerState;
use crate::app::{FrontendTrack, Page, PageContent, Route};
use crate::capture::{capture_page_name, capture_perf_device_target, capture_scenario_target};
use crate::confirmation::ConfirmationCapture;
use crate::focus_visible::FocusCapture;
use crate::input_contract::{SemanticAddress, stable_semantic_address};
use crate::menu_modal::MenuModalChanged;
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
use crate::pages::processes::card_lane::{
    ProcessCardFlow, ProcessCardGroup, ProcessFocusDashboard, ProcessViewMode,
};
use crate::pages::processes::menu::{ProcessMenuCtx, ProcessMenuModal, open_for_selected};
use crate::pages::processes::properties_modal::PropertiesCapture;
use crate::pages::processes::{ProcessRowsRoot, ProcessScrollState};
use crate::pages::system::dashboard::SystemDashboardCurve;
use crate::pages::system::dashboard::SystemDashboardState;
use crate::pages::system::{MemoryInventoryAnchor, SystemBody};
use crate::widgets::chart::CurveMeasurement;
use crate::widgets::table::ZeroValueCell;
use crate::window::capture_marker_cores::CoreGridCaptureAccess;
use crate::window::capture_marker_interactive::InteractiveCaptureAccess;
use crate::window::{FeedbackLine, WindowPalette};
use crate::window_surface::{ModalBody, WindowSurfaceKind, WindowSurfaceState};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::{With, Without};
use bevy::ecs::system::{Commands, NonSend, Query, Res, ResMut, SystemParam};
use bevy::text::TextColor;
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, ScrollPosition, UiGlobalTransform};
use taskforest_core::core::metrics::ScalarAvailability;
use taskmanager_ui_contract::navigation::NavOrientation;

type TreeViewportFilter = (With<ModalBody>, Without<SystemBody>);

#[derive(SystemParam)]
pub(super) struct CaptureAccess<'w, 's> {
    pub(super) track: NonSend<'w, FrontendTrack>,
    window_surface: Res<'w, WindowSurfaceState>,
    saved_views: Res<'w, crate::saved_views::SavedViewsState>,
    navigation: Res<'w, NavigationState>,
    nav_strip: Query<'w, 's, &'static ComputedNode, With<NavigationStrip>>,
    pub(super) system_state: Res<'w, SystemDashboardState>,
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
    pub(super) route: Res<'w, Route>,
    pub(super) state: ResMut<'w, CaptureMarkerState>,
    pages: Query<'w, 's, &'static PageContent>,
    pub(super) text: Query<'w, 's, (&'static Text, &'static ComputedNode)>,
    feedback_lines: Query<'w, 's, (&'static Text, &'static ComputedNode), With<FeedbackLine>>,
    focus: FocusCapture<'w, 's>,
    confirmation: ConfirmationCapture<'w, 's>,
    properties: PropertiesCapture<'w, 's>,
    anchors: Query<
        'w,
        's,
        (&'static ComputedNode, &'static UiGlobalTransform),
        With<MemoryInventoryAnchor>,
    >,
    pub(super) zero_cells: Query<
        'w,
        's,
        (
            &'static ZeroValueCell,
            &'static Text,
            &'static ComputedNode,
            &'static TextColor,
        ),
    >,
    pub(super) process_roots: Query<'w, 's, Entity, With<ProcessRowsRoot>>,
    pub(super) process_scroll: Option<Res<'w, ProcessScrollState>>,
    card_mode: Option<Res<'w, ProcessViewMode>>,
    card_flow:
        Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<ProcessCardFlow>>,
    card_groups: Query<
        'w,
        's,
        (
            &'static ProcessCardGroup,
            &'static ComputedNode,
            &'static UiGlobalTransform,
        ),
        With<ProcessCardGroup>,
    >,
    dashboards: Query<
        'w,
        's,
        (&'static ComputedNode, &'static UiGlobalTransform),
        With<ProcessFocusDashboard>,
    >,
    pub(super) palette: Res<'w, WindowPalette>,
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
    cores: CoreGridCaptureAccess<'w, 's>,
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
    interactive: InteractiveCaptureAccess<'w, 's>,
    process_menu: Option<ResMut<'w, ProcessMenuModal>>,
    pub(super) commands: Commands<'w, 's>,
}

pub(super) fn emit_capture_marker(mut access: CaptureAccess) {
    let route = access.route.page;
    if access.state.emitted || access.pages.iter().all(|content| content.page != route) {
        return;
    }
    // The card chrome mounts on the frame the page arrives, while the grid
    // body learns its open height on a later frame: a frame admitted in
    // between is a card header above an empty grid.
    if route == Page::Performance && !super::capture_marker_cores::core_grid_ready(&access.cores) {
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
        Some("applications-card-lane") => {
            if route != Page::Processes
                || access
                    .card_mode
                    .is_none_or(|mode| *mode != ProcessViewMode::Cards)
                || !access
                    .dashboards
                    .iter()
                    .any(|(node, _)| node.size().x > 0.0 && node.size().y > 0.0)
                || !access
                    .card_groups
                    .iter()
                    .any(|(_, node, _)| node.size().x > 0.0 && node.size().y > 0.0)
            {
                return;
            }
            let Some((flow, flow_transform)) = access.card_flow.iter().next() else {
                return;
            };
            let flow_bottom = flow_transform.translation.y + flow.size().y / 2.0;
            let flow_right = flow_transform.translation.x + flow.size().x / 2.0;
            let mut groups_inside = true;
            for (_, node, transform) in &access.card_groups {
                let half = node.size() / 2.0;
                groups_inside &= transform.translation.x - half.x
                    >= flow_transform.translation.x - flow.size().x / 2.0 - 0.5
                    && transform.translation.x + half.x <= flow_right + 0.5
                    && transform.translation.y + half.y <= flow_bottom + 0.5;
            }
            if !groups_inside {
                return;
            }
            if !access.state.data_presented {
                access.state.data_presented = true;
                return;
            }
        }
        Some("apps-zero-gray")
            if !super::capture_marker_zero_gray::apps_zero_gray_ready(&mut access) =>
        {
            return;
        }
        Some("apps-zero-gray") => {}

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
                || !rows.iter().any(|row| {
                    row.item.is_none()
                        && row.cpu.as_ref().is_some_and(|cpu| {
                            matches!(cpu.availability(), ScalarAvailability::Partial(_))
                                && cpu.current_member_count() + 1 == cpu.member_count()
                        })
                        && row.memory.as_ref().is_some_and(|memory| {
                            matches!(memory.availability(), ScalarAvailability::Partial(_))
                                && memory.current_member_count() + 1 == memory.member_count()
                        })
                })
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
        Some("apps-container-group") => {
            let projection = access.track.shell.projection();
            let rows = crate::pages::process_tree::project_items(
                projection.processes_slice(),
                &access.track.process_tree_expansion,
                projection.processes_observed_at_ms,
            );
            if rows.iter().filter(|row| row.item.is_some()).count() < 7
                || rows.iter().any(|row| row.has_children && !row.expanded)
            {
                return;
            }
            let container = rows.iter().find(|row| {
                row.key.is_none()
                    && row.depth == 0
                    && row.item.is_none()
                    && row.expanded
                    && row.semantic_key.starts_with("container:")
            });
            if container.is_none() {
                return;
            }
            let members = rows.iter().filter(|row| {
                row.depth == 1
                    && row
                        .item
                        .is_some_and(|item| (90_001..=90_003).contains(&item.pid))
            });
            if members.clone().count() != 3 || members.clone().any(|row| row.item.is_none()) {
                return;
            }
            if !access.state.container_members_presented {
                access.state.container_members_presented = true;
                return;
            }
        }
        Some("process-context-menu") => {
            // The Applications inventory must be mounted before the anchored
            // menu can float against it; its own measured card is the
            // readiness anchor, so a frame is emitted only after the real
            // glass popover has been laid out (never a prepared-but-unpainted
            // menu).
            if access.process_roots.single().is_err() {
                return;
            }
            if !access.state.menu_open_requested {
                let Some(menu) = access.process_menu.as_mut() else {
                    return;
                };
                if !open_for_selected(menu, &access.track.shell) {
                    return;
                }
                access.commands.trigger(MenuModalChanged::<ProcessMenuCtx>(
                    true,
                    std::marker::PhantomData,
                ));
                access.state.menu_open_requested = true;
                return;
            }
            if !super::capture_marker_interactive::process_menu_mounted(&mut access.interactive) {
                return;
            }
            if !access.state.data_presented {
                access.state.data_presented = true;
                return;
            }
        }
        Some("feedback-overlay") => {
            let Some(notice) = access.track.shell.feedback_notice() else {
                return;
            };
            if !access.feedback_lines.iter().any(|(text, node)| {
                text.0 == notice.text() && node.size().x > 0.0 && node.size().y > 0.0
            }) {
                return;
            }
            if !access.state.data_presented {
                access.state.data_presented = true;
                return;
            }
        }
        Some("tooltip-overlay") => {
            if !super::capture_marker_interactive::tooltip_mounted(&mut access.interactive) {
                return;
            }
            if !access.state.data_presented {
                access.state.data_presented = true;
                return;
            }
        }
        Some("event-center") => {
            if !access
                .window_surface
                .is_kind(WindowSurfaceKind::EventCenter)
                || access
                    .track
                    .shell
                    .projection()
                    .alert_center
                    .event_history()
                    .len()
                    < 2
                || !super::capture_marker_interactive::has_all_texts(
                    &access.text,
                    &[
                        "events.title",
                        "events.activated",
                        "events.cleared",
                        "common.export",
                        "common.close",
                    ],
                )
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
            if !access.window_surface.is_kind(WindowSurfaceKind::SavedViews)
                || !access
                    .saved_views
                    .rows
                    .iter()
                    .any(|row| row.is_user_saved())
                || !super::capture_marker_interactive::has_all_texts(
                    &access.text,
                    &[
                        "saved_views.title",
                        "saved_views.save_current",
                        "common.apply",
                        "common.remove",
                        "common.export",
                        "common.import",
                        "common.close",
                    ],
                )
            {
                return;
            }
            if !access.state.data_presented {
                access.state.data_presented = true;
                return;
            }
        }
        Some("sidebar-edit") => {
            if !access
                .window_surface
                .is_kind(WindowSurfaceKind::SidebarDevices)
                || !super::capture_marker_interactive::has_all_texts(
                    &access.text,
                    &["Edit devices", "Hide", "Move up", "Move down", "Done"],
                )
            {
                return;
            }
            if !access.state.data_presented {
                access.state.data_presented = true;
                return;
            }
        }

        Some(scenario @ ("storage-health" | "sensor-center")) => {
            if !super::capture_marker_health::health_review_ready(&access, route, scenario) {
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
        Some("users-session-confirmation") if route != Page::Sessions => return,
        Some(
            "process-force-kill"
            | "process-tree-confirm"
            | "process-batch-confirm"
            | "users-session-confirmation"
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
            // Bring the anchored card into view. The correction is re-issued
            // EVERY frame until the anchor sits inside the body's rect: the
            // marker runs in `PostUpdate` while the layout that applies the
            // scroll lands in the next frame's layout pass, and a repaint
            // that re-lays the content moves the anchor again. A one-shot
            // correction therefore stops short — the anchor was measured
            // before the scroll took effect, so the next correction still
            // had work to do.
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
            // A compact System page can make this card taller than the body
            // viewport. The body owns vertical scrolling, so prove that the
            // anchored inventory content is actually in view without
            // requiring the entire scrollable card to fit at once.
            if !access.bodies.iter().any(|(node, transform, _)| {
                center.x >= transform.translation.x - node.size().x / 2.0 - 0.5
                    && center.x <= transform.translation.x + node.size().x / 2.0 + 0.5
                    && center.y >= transform.translation.y - node.size().y / 2.0 - 0.5
                    && center.y <= transform.translation.y + node.size().y / 2.0 + 0.5
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
                | "feedback-overlay"
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
