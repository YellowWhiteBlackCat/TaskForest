use super::*;
use gpui::{AppContext, Context, IntoElement, Render, TestAppContext, Window};
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::process_telemetry::{
    ProcessEnvironment, ProcessEnvironmentEntry, ProcessTelemetrySnapshot,
};

fn labels() -> ProcessInsightsLabels {
    ProcessInsightsLabels::capture_fixture()
}

fn snapshot_with(environment: ProcessEnvironment) -> ProcessTelemetrySnapshot {
    ProcessTelemetrySnapshot {
        environment,
        ..ProcessTelemetrySnapshot::default()
    }
}

fn populated_environment() -> ProcessEnvironment {
    ProcessEnvironment {
        state: DeviceState::healthy(1),
        working_directory: Some("/tmp".into()),
        entries: vec![
            ProcessEnvironmentEntry {
                key: "PATH".into(),
                value: "/usr/bin:/bin".into(),
            },
            ProcessEnvironmentEntry {
                key: "USER".into(),
                value: "developer".into(),
            },
        ],
        truncated_count: 5,
    }
}

struct EnvironmentCardView {
    card: Div,
}

impl Render for EnvironmentCardView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        std::mem::replace(&mut self.card, div())
    }
}

fn draw_frame(cx: &mut TestAppContext, snapshot: ProcessTelemetrySnapshot) {
    cx.update(taskmanager_ui::init);
    let theme = Theme::dark();
    let card = environment_card(&theme, &snapshot, &labels(), 480.0);
    let window = cx.add_window(|_w, _cx| EnvironmentCardView { card });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

#[gpui::test]
fn empty_healthy_renders_the_no_environment_message(cx: &mut TestAppContext) {
    draw_frame(
        cx,
        snapshot_with(ProcessEnvironment {
            state: DeviceState::healthy(1),
            ..ProcessEnvironment::default()
        }),
    );
}

#[gpui::test]
fn oversized_environment_list_renders_through_the_capped_path(cx: &mut TestAppContext) {
    let cap = MAX_INSIGHT_CARD_ROWS;
    let environment = ProcessEnvironment {
        state: DeviceState::healthy(1),
        working_directory: None,
        entries: (0..cap + 50)
            .map(|i| ProcessEnvironmentEntry {
                key: format!("VAR_{i}"),
                value: format!("value_{i}"),
            })
            .collect(),
        truncated_count: 10,
    };
    draw_frame(cx, snapshot_with(environment));
}

#[gpui::test]
fn denied_state_renders_a_typed_status_not_blank(cx: &mut TestAppContext) {
    let denied = ProcessEnvironment {
        state: DeviceState::healthy(1).transition(DeviceStatus::PermissionDenied, 2),
        ..ProcessEnvironment::default()
    };
    draw_frame(cx, snapshot_with(denied));
}

#[gpui::test]
fn populated_environment_renders_rows(cx: &mut TestAppContext) {
    draw_frame(cx, snapshot_with(populated_environment()));
}
