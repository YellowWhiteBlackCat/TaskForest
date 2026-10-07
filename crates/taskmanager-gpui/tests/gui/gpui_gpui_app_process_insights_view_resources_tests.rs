//! Headless visual/projection tests for the process insights resources card.

use super::*;
use gpui::{AppContext, Context, IntoElement, Render, TestAppContext, Window};
use taskmanager_application::project_process_resources;
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::process_telemetry::{
    LimitValue, ProcessResourceObservations, ProcessResourceSnapshot, ResourceGroupMembership,
    ResourceObservation,
};
use taskmanager_ui::init;

fn labels() -> ProcessInsightsLabels {
    ProcessInsightsLabels::capture_fixture()
}

struct ResourceCardView {
    card: Div,
}

impl Render for ResourceCardView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        std::mem::replace(&mut self.card, div())
    }
}

fn draw_resource_frame(cx: &mut TestAppContext, resources: ProcessResourceSnapshot) {
    cx.update(init);
    let theme = Theme::dark();
    let card = resource_card(
        &theme,
        project_process_resources(&resources),
        &labels(),
        480.0,
        UnitPreferences::default(),
    );
    let window = cx.add_window(|_w, _cx| ResourceCardView { card });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

#[gpui::test]
fn resource_card_renders_memory_cpu_quota_pids_and_cgroup_locator(cx: &mut TestAppContext) {
    let now_ms = 1000;
    draw_resource_frame(
        cx,
        ProcessResourceSnapshot::from_observations(
            DeviceState::healthy(now_ms),
            ProcessResourceObservations {
                resource_groups: ResourceObservation::current(
                    vec![ResourceGroupMembership {
                        provider: ProviderId::borrowed("cgroup.test"),
                        native_hierarchy_id: Some(0),
                        capabilities: Vec::new(),
                        native_locator: "/system.slice/worker.scope".into(),
                    }],
                    now_ms,
                ),
                memory_usage_bytes: ResourceObservation::current(256 * 1024 * 1024, now_ms),
                memory_limit: ResourceObservation::current(
                    LimitValue::Value(1024 * 1024 * 1024),
                    now_ms,
                ),
                cpu_time_quota_micros: ResourceObservation::current(
                    LimitValue::Value(150_000),
                    now_ms,
                ),
                cpu_time_period_micros: ResourceObservation::current(100_000, now_ms),
                process_count: ResourceObservation::current(7, now_ms),
                process_limit: ResourceObservation::current(LimitValue::Value(64), now_ms),
                ..ProcessResourceObservations::default()
            },
            Vec::new(),
        ),
    );
}

#[gpui::test]
fn resource_card_renders_unlimited_quota_and_pid_limits(cx: &mut TestAppContext) {
    let now_ms = 1000;
    draw_resource_frame(
        cx,
        ProcessResourceSnapshot::from_observations(
            DeviceState::healthy(now_ms),
            ProcessResourceObservations {
                memory_usage_bytes: ResourceObservation::current(512 * 1024 * 1024, now_ms),
                memory_limit: ResourceObservation::current(LimitValue::Unlimited, now_ms),
                cpu_time_quota_micros: ResourceObservation::current(LimitValue::Unlimited, now_ms),
                process_count: ResourceObservation::current(3, now_ms),
                process_limit: ResourceObservation::current(LimitValue::Unlimited, now_ms),
                ..ProcessResourceObservations::default()
            },
            Vec::new(),
        ),
    );
}
