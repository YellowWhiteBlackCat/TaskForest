//! GPU-engines sub-section of the Process Properties insights dialog.
//!
//! Renders the per-engine breakdown collected from `/proc/<pid>/fdinfo` (see
//! `taskmanager-platform-linux::engine::process::telemetry::gpu_engines`).
//! Mirrors the open-files and threads cards: a typed collection state is shown
//! verbatim, a healthy process with no engines is an explicit "no engines"
//! message, and a populated breakdown renders a scrollable mono-font list. The
//! cold-start rate gap (first sample, before a delta exists) renders as an
//! explicit unavailable marker — never a fabricated `0.0%`.
//!
//! The card's copy is hard-coded English rather than threaded through
//! [`ProcessInsightsLabels`]: the labels struct is constructed as a full
//! literal by the Properties chrome, so adding fields there would widen this
//! feature's blast radius into the root view. These strings follow the same
//! stable-English-for-capture convention as the capture fixture.

use gpui::{Div, ParentElement, Styled, div, px};
use taskmanager_core::core::process_telemetry::ProcessTelemetrySnapshot;

use crate::gpui_app::theme::mono_font_with_fallback;
use taskmanager_core::core::ProcessGpuEngineUsage;
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use super::ProcessInsightsLabels;

/// Card title.
const TITLE: &str = "GPU engines";
/// Healthy empty message (a live, non-GPU process).
const NO_ENGINES: &str = "No GPU engine counters";

/// Format one engine's cumulative busy time as a human-friendly duration.
fn format_engine_time(nanoseconds: u64) -> String {
    let seconds = nanoseconds as f64 / 1_000_000_000.0;
    format!("{seconds:.1}s")
}

/// Format a cumulative cycle counter (xe fdinfo) as a compact count.
fn format_engine_cycles(cycles: u64) -> String {
    if cycles >= 1_000_000_000 {
        format!("{:.2}G cycles", cycles as f64 / 1_000_000_000.0)
    } else if cycles >= 1_000_000 {
        format!("{:.1}M cycles", cycles as f64 / 1_000_000.0)
    } else {
        format!("{cycles} cycles")
    }
}

/// Render one engine line: `<name>  <rate>  <cumulative>`.
fn format_engine_line(engine: &ProcessGpuEngineUsage, labels: &ProcessInsightsLabels) -> String {
    let rate = engine
        .usage_pct
        .current_value()
        .map(|value| format!("{value:.1}%"))
        .unwrap_or_else(|| labels.unknown.to_string());
    // xe exposes cycles instead of busy ns; the cycle count is the honest
    // cumulative observable there, preferred over the "unknown" fallback.
    let cumulative = engine
        .engine_time_ns
        .current_value()
        .map(|value| format_engine_time(*value))
        .or_else(|| {
            engine
                .engine_cycles
                .current_value()
                .map(|value| format_engine_cycles(*value))
        })
        .unwrap_or_else(|| labels.unknown.to_string());
    format!("{}  {rate}  {cumulative}", engine.name)
}

/// The GPU-engines card. Surfaces the per-engine utilization breakdown or an
/// explicit typed message when the source is unavailable, denied, or empty.
pub(in crate::gpui_app::process_insights::view) fn gpu_engines_card(
    theme: &Theme,
    snapshot: &ProcessTelemetrySnapshot,
    labels: &ProcessInsightsLabels,
    width: f32,
) -> Div {
    let engines = &snapshot.gpu.engines;
    if engines.state.status != DeviceStatus::Healthy {
        return super::card(theme, TITLE, width).child(
            div()
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(super::status_label(engines.state.status, labels).to_string()),
        );
    }
    let mut content = super::card(theme, TITLE, width);
    if engines.engines.is_empty() {
        return content.child(
            div()
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(NO_ENGINES.to_string()),
        );
    }
    content = content.child(
        div()
            .text_size(tokens::FONT_11)
            .text_color(theme.fg_dim)
            .child(format!("{} · {}", TITLE, engines.engines.len())),
    );
    content = content.child(
        div().flex().flex_col().gap(tokens::SPACE_3).children(
            engines
                .engines
                .iter()
                .map(|engine| format_engine_line(engine, labels))
                .map(|line| {
                    div()
                        .min_w(px(0.0))
                        .text_size(tokens::FONT_10)
                        .font(mono_font_with_fallback(theme))
                        .whitespace_normal()
                        .child(line)
                }),
        ),
    );
    content
}

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_gpui_app_process_insights_view_gpu_engines_tests.rs"]
mod tests;
