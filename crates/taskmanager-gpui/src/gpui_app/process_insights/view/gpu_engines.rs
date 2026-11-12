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

#[cfg(any(test, feature = "test-support"))]
use gpui::InteractiveElement;
use gpui::{Div, ParentElement, Styled, div, px};
use taskforest_core::core::process_telemetry::ProcessGpuSnapshot;
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::hsla;

use crate::gpui_app::theme::mono_font_with_fallback;
use taskforest_core::core::ProcessGpuEngineUsage;
use taskforest_core::core::device_state::DeviceStatus;
use taskmanager_shell::presentation::{format_engine_cycles, format_engine_time};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use super::ProcessInsightsLabels;
use taskforest_core::core::percent::percent_text;

/// Card title.
const TITLE: &str = "GPU engines";
/// Healthy empty message (a live, non-GPU process).
const NO_ENGINES: &str = "No GPU engine counters";

/// Render one engine line: `<name>  <rate>  <cumulative>`.
fn format_engine_line(engine: &ProcessGpuEngineUsage, labels: &ProcessInsightsLabels) -> String {
    let rate = engine
        .usage_pct
        .current_value()
        .copied()
        .map(percent_text)
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
    gpu: &ProcessGpuSnapshot,
    labels: &ProcessInsightsLabels,
    width: f32,
) -> Div {
    let engines = &gpu.engines;
    if engines.state.status != DeviceStatus::Healthy {
        return super::card(theme, TITLE, width).child(
            div()
                .text_size(font_size(tokens::FONT_11))
                .text_color(hsla(theme.fg_dim))
                .child(super::status_label(engines.state.status, labels).to_string()),
        );
    }
    let content = super::card(theme, TITLE, width).w_full();
    if engines.engines.is_empty() {
        return content.child(
            div()
                .text_size(font_size(tokens::FONT_11))
                .text_color(hsla(theme.fg_dim))
                .child(NO_ENGINES.to_string()),
        );
    }
    #[cfg(any(test, feature = "test-support"))]
    let mut content = content.debug_selector(|| "tm-insights-gpu-engines".to_owned());
    #[cfg(not(any(test, feature = "test-support")))]
    let mut content = content;
    content = content.child(
        div()
            .text_size(font_size(tokens::FONT_11))
            .text_color(hsla(theme.fg_dim))
            .child(format!("{} · {}", TITLE, engines.engines.len())),
    );
    content = content.child(
        div()
            .flex()
            .flex_col()
            .gap(definite_length(tokens::SPACE_3))
            .children(
                engines
                    .engines
                    .iter()
                    .map(|engine| format_engine_line(engine, labels))
                    .map(|line| {
                        div()
                            .min_w(px(0.0))
                            .text_size(font_size(tokens::FONT_10))
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
