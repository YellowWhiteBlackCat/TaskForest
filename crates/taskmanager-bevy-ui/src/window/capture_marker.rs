//! Capture readiness follows the mounted and painted product surfaces.
use super::CaptureMarkerState;
use crate::app::{PageContent, Route};
use crate::capture::{capture_page_name, capture_scenario_target};
use crate::pages::history::HistoryProjectionResource;
use crate::pages::system::{MemoryInventoryAnchor, SystemBody};
use bevy::ecs::query::With;
use bevy::ecs::world::World;
use bevy::ui::{ComputedNode, ScrollPosition, UiGlobalTransform};

pub(super) fn emit_capture_marker(world: &mut World) {
    let route = world.resource::<Route>().page;
    if world.resource::<CaptureMarkerState>().emitted
        || world
            .query::<&PageContent>()
            .iter(world)
            .all(|content| content.page != route)
    {
        return;
    }
    match capture_scenario_target() {
        Some("keyboard-focus") if !crate::focus_visible::capture_ready(world, false) => {
            return;
        }
        Some("settings-switch-focus") if !crate::focus_visible::capture_ready(world, true) => {
            return;
        }
        Some(
            "process-force-kill"
            | "process-tree-confirm"
            | "process-batch-confirm"
            | "smart-self-test-confirm",
        ) if !crate::confirmation::capture_ready(world) => {
            return;
        }
        Some(
            scenario @ ("process-properties-performance"
            | "process-memory-pss-swap"
            | "process-network-details"
            | "process-gpu-details"
            | "process-resource-limits"
            | "process-isolation"),
        ) if !crate::pages::processes::properties_modal::capture_ready(world, scenario) => {
            return;
        }
        Some("system-dashboard" | "history-60m")
            if !crate::pages::system::dashboard::presented(world) =>
        {
            return;
        }
        Some("system-hardware") => {
            let anchor = world
                .query_filtered::<(&ComputedNode, &UiGlobalTransform), With<MemoryInventoryAnchor>>(
                )
                .iter(world)
                .next()
                .map(|(node, transform)| (node.size(), transform.translation));
            let Some((size, center)) = anchor.filter(|(size, _)| size.x > 0.0 && size.y > 0.0)
            else {
                return;
            };
            if !world
                .resource::<CaptureMarkerState>()
                .hardware_scroll_requested
            {
                let mut body = world.query_filtered::<(&ComputedNode, &UiGlobalTransform, &mut ScrollPosition), With<SystemBody>>();
                for (node, transform, mut scroll) in body.iter_mut(world) {
                    let maximum = ((node.content_size().y - node.size().y)
                        * node.inverse_scale_factor())
                    .max(0.0);
                    let delta = (center.y - size.y / 2.0 - transform.translation.y
                        + node.size().y / 2.0)
                        * node.inverse_scale_factor();
                    scroll.0.y = (scroll.0.y + delta).clamp(0.0, maximum);
                }
                world
                    .resource_mut::<CaptureMarkerState>()
                    .hardware_scroll_requested = true;
                return;
            }
            let mut body =
                world.query_filtered::<(&ComputedNode, &UiGlobalTransform), With<SystemBody>>();
            if !body.iter(world).any(|(node, transform)| {
                center.y - size.y / 2.0 >= transform.translation.y - node.size().y / 2.0 - 0.5
                    && center.y + size.y / 2.0
                        <= transform.translation.y + node.size().y / 2.0 + 0.5
            }) {
                return;
            }
        }
        Some("history-replay") => {
            use crate::pages::history::control::{
                HistoryCommand, PerformanceHistoryProjectionResource, PerformancePresentation,
            };
            if !world
                .resource::<CaptureMarkerState>()
                .history_open_requested
                && world
                    .non_send::<crate::pages::history::HistoryRuntime>()
                    .available()
            {
                world
                    .resource_mut::<CaptureMarkerState>()
                    .history_open_requested = true;
                world.trigger(HistoryCommand::OpenPerformance);
            }
            if *world.resource::<PerformancePresentation>() != PerformancePresentation::Replay
                || world
                    .resource::<PerformanceHistoryProjectionResource>()
                    .0
                    .rows
                    .is_empty()
                || !crate::pages::performance::replay::charts_presented(world)
            {
                return;
            }
        }
        Some("application-history-replay")
            if world
                .resource::<HistoryProjectionResource>()
                .0
                .rows
                .is_empty() =>
        {
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
    ) && !world.resource::<CaptureMarkerState>().data_presented
    {
        world.resource_mut::<CaptureMarkerState>().data_presented = true;
        return;
    }
    let page = capture_page_name(route);
    println!("BEVY_CAPTURE_MARKER event=frame_ready mode=demo page={page}");
    println!("BEVY_CAPTURE_MARKER event=target_ready mode=demo page={page}");
    world.resource_mut::<CaptureMarkerState>().emitted = true;
}
