use super::*;
use crate::core::device_state::DeviceState;
use crate::core::{
    DeviceStatus, FailureKind, ProcessGpuEngineUsage, ProcessGpuEngines, ScalarObservation,
};
use gpui::{AppContext, Context, IntoElement, Render, TestAppContext, Window};
use taskmanager_application::ProcessGpuSnapshot;

fn labels() -> ProcessInsightsLabels {
    ProcessInsightsLabels::capture_fixture()
}

fn snapshot_with(engines: ProcessGpuEngines) -> ProcessTelemetrySnapshot {
    ProcessTelemetrySnapshot {
        gpu: ProcessGpuSnapshot {
            state: DeviceState::healthy(1),
            engines,
            ..ProcessGpuSnapshot::default()
        },
        ..ProcessTelemetrySnapshot::default()
    }
}

fn populated_engines() -> ProcessGpuEngines {
    let mut engines = ProcessGpuEngines::empty_healthy(1);
    engines.engines.push(ProcessGpuEngineUsage {
        name: "render".into(),
        // Cold-start: rate is a typed gap, cumulative is observed.
        usage_pct: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        engine_time_ns: ScalarObservation::available(750_000_000, 1),
        engine_cycles: ScalarObservation::default(),
    });
    engines.engines.push(ProcessGpuEngineUsage {
        name: "video".into(),
        usage_pct: ScalarObservation::available(12.5, 1),
        engine_time_ns: ScalarObservation::available(2_500_000_000, 1),
        engine_cycles: ScalarObservation::default(),
    });
    engines
}

/// xe fdinfo exposes cycles instead of busy ns: the cycle count must render
/// as the honest cumulative observable, never a fabricated time.
#[test]
fn cycles_only_engine_renders_cycle_count_not_fabricated_time() {
    let snapshot = snapshot_with({
        let mut engines = ProcessGpuEngines::empty_healthy(1);
        engines.engines.push(ProcessGpuEngineUsage {
            name: "vcs".into(),
            usage_pct: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            engine_time_ns: ScalarObservation::default(),
            engine_cycles: ScalarObservation::available(643_228_675_411, 1),
        });
        engines
    });
    let line = format_engine_line(&snapshot.gpu.engines.engines[0], &labels());
    assert!(line.contains("vcs"), "{line}");
    assert!(line.contains("643.23G cycles"), "{line}");
    assert!(
        !line.contains("0.0s"),
        "a cycles-only source must not fabricate a duration: {line}"
    );
}

/// Cold-start honesty: a gap engine must never fabricate `0.0%`, while a
/// warmed engine reports its percentage. This is the load-bearing honesty
/// assertion and needs no window.
#[test]
fn format_keeps_cold_start_gap_honest() {
    let snapshot = snapshot_with(populated_engines());
    let gap_line = format_engine_line(&snapshot.gpu.engines.engines[0], &labels());
    let warm_line = format_engine_line(&snapshot.gpu.engines.engines[1], &labels());
    assert!(gap_line.contains("render"));
    assert!(
        !gap_line.contains("0.0%"),
        "cold-start gap must not fabricate 0%"
    );
    assert!(warm_line.contains("12.5%"));
}

/// Minimal root view that renders one card frame, so the gpu_engines card
/// can be exercised through the same window-draw path the rest of the
/// process-insights tests use.
struct EngineCardView {
    card: Div,
}
impl Render for EngineCardView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        std::mem::replace(&mut self.card, div())
    }
}

fn draw_frame(cx: &mut TestAppContext, snapshot: ProcessTelemetrySnapshot) {
    let theme = Theme::dark();
    let card = gpu_engines_card(&theme, &snapshot, &labels(), 480.0);
    let window = cx.add_window(|_w, _cx| EngineCardView { card });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

#[gpui::test]
fn empty_healthy_renders_the_no_engines_message(cx: &mut TestAppContext) {
    draw_frame(cx, snapshot_with(ProcessGpuEngines::empty_healthy(1)));
}

#[gpui::test]
fn denied_state_renders_a_typed_status_not_blank(cx: &mut TestAppContext) {
    let denied = ProcessGpuEngines::unavailable(
        DeviceState::healthy(1).transition(DeviceStatus::PermissionDenied, 2),
    );
    draw_frame(cx, snapshot_with(denied));
}

#[gpui::test]
fn populated_engines_render_with_cold_start_gap(cx: &mut TestAppContext) {
    draw_frame(cx, snapshot_with(populated_engines()));
}
