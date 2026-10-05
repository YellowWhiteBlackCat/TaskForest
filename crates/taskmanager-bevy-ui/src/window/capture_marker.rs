//! Capture readiness follows explicitly queried mounted and painted surfaces.
use super::CaptureMarkerState;
use crate::app::{PageContent, Route};
use crate::capture::{capture_page_name, capture_scenario_target};
use crate::confirmation::ConfirmationCapture;
use crate::focus_visible::FocusCapture;
use crate::pages::history::control::{
    HistoryCommand, PerformanceHistoryProjectionResource, PerformancePresentation,
};
use crate::pages::history::{HistoryProjectionResource, HistoryRuntime};
use crate::pages::performance::replay::{ChartSize, ReplayChart};
use crate::pages::processes::properties_modal::PropertiesCapture;
use crate::pages::system::dashboard::SystemDashboardCurve;
use crate::pages::system::{MemoryInventoryAnchor, SystemBody};
use crate::widgets::chart::CurveMeasurement;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, NonSend, Query, Res, ResMut, SystemParam};
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, ScrollPosition, UiGlobalTransform};

#[derive(SystemParam)]
pub(super) struct CaptureAccess<'w, 's> {
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
    match capture_scenario_target() {
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
