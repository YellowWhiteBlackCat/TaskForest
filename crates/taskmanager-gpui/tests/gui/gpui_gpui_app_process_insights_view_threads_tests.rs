use super::*;
use gpui::{AppContext, Context, IntoElement, Render, TestAppContext, Window};
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::process_telemetry::{ProcessThreads, ThreadState};

fn labels() -> ProcessInsightsLabels {
    ProcessInsightsLabels::capture_fixture()
}

fn snapshot_with(threads: ProcessThreads) -> ProcessTelemetrySnapshot {
    ProcessTelemetrySnapshot {
        threads,
        ..ProcessTelemetrySnapshot::default()
    }
}

fn populated_threads() -> ProcessThreads {
    ProcessThreads {
        state: DeviceState::healthy(1),
        threads: vec![
            ProcessThreadInfo {
                tid: 4242,
                comm: "telemetry-main".into(),
                state: ThreadState::Sleep,
                cpu_time_secs: Some(12.5),
                cpu_percent: Some(18.5),
            },
            // A thread whose `stat` lacked parseable CPU counters: the row
            // is kept with cpu_time_secs = None and must render an explicit
            // dash rather than a fabricated "0.0s".
            ProcessThreadInfo {
                tid: 4243,
                comm: "reaper".into(),
                state: ThreadState::Running,
                cpu_time_secs: None,
                cpu_percent: None,
            },
        ],
    }
}

/// Honesty: a thread without parsed CPU counters must render the explicit
/// dash, never a fabricated "0.0s". This is the load-bearing honesty
/// assertion and needs no window.
#[test]
fn format_keeps_missing_cpu_time_honest() {
    let threads = populated_threads();
    let warm_line = format_thread(&threads.threads[0]);
    let gap_line = format_thread(&threads.threads[1]);
    assert!(warm_line.contains("12.5s"));
    assert!(warm_line.contains("18.5%"));
    assert!(
        gap_line.contains("—") && !gap_line.contains("0.0s") && !gap_line.contains("0.0%"),
        "a thread with missing CPU values must show dashes, got: {gap_line}"
    );
}

/// Minimal root view that renders one card frame, so the threads card can
/// be exercised through the same window-draw path the rest of the
/// process-insights tests use.
struct ThreadsCardView {
    card: Div,
}
impl Render for ThreadsCardView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        std::mem::replace(&mut self.card, div())
    }
}

fn draw_frame(cx: &mut TestAppContext, snapshot: ProcessTelemetrySnapshot) {
    let theme = Theme::dark();
    let card = threads_card(&theme, &snapshot, &labels(), 480.0);
    let window = cx.add_window(|_w, _cx| ThreadsCardView { card });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

#[gpui::test]
fn empty_healthy_renders_the_no_threads_message(cx: &mut TestAppContext) {
    draw_frame(
        cx,
        snapshot_with(ProcessThreads {
            state: DeviceState::healthy(1),
            ..ProcessThreads::default()
        }),
    );
}

/// A browser-class process (hundreds of threads beyond the card cap)
/// renders through the bounded window without panic; the window math and
/// the "… {count} more" remainder are locked by `view::cap_tests`.
#[gpui::test]
fn oversized_thread_list_renders_through_the_capped_path(cx: &mut TestAppContext) {
    let cap = super::super::MAX_INSIGHT_CARD_ROWS;
    let threads = ProcessThreads {
        state: DeviceState::healthy(1),
        threads: (0..cap + 137)
            .map(|i| ProcessThreadInfo {
                tid: 1000 + i as u32,
                comm: format!("worker-{i}"),
                state: ThreadState::Sleep,
                cpu_time_secs: Some(i as f64),
                cpu_percent: Some(0.25),
            })
            .collect(),
    };
    draw_frame(cx, snapshot_with(threads));
}

/// A denied `/proc/<pid>/task` read renders the typed "Permission denied"
/// status, never a blank card.
#[gpui::test]
fn denied_state_renders_a_typed_status_not_blank(cx: &mut TestAppContext) {
    let denied = ProcessThreads {
        state: DeviceState::healthy(1).transition(DeviceStatus::PermissionDenied, 2),
        ..ProcessThreads::default()
    };
    draw_frame(cx, snapshot_with(denied));
}

#[gpui::test]
fn populated_threads_render_with_missing_cpu_dash(cx: &mut TestAppContext) {
    draw_frame(cx, snapshot_with(populated_threads()));
}
