//! Threads sub-section of the Process Properties insights dialog.
//!
//! Renders the per-thread breakdown collected from `/proc/<pid>/task` (see
//! `taskmanager-platform-linux::engine::process::telemetry::threads`). Mirrors
//! the gpu_engines and open-files cards: a typed collection state is shown
//! verbatim (a denied `/proc/<pid>/task` read renders as "Permission denied",
//! never a silent omission), a healthy process with no enumerated threads is an
//! explicit "no threads" message, and a populated list renders a scrollable
//! mono-font list.
//!
//! Honesty on the CPU columns: cumulative CPU time (`utime + stime`) remains
//! independent from the identity-bound instantaneous rate. The first sample,
//! a counter rollback, or a timestamp/clock gap renders an explicit dash for
//! CPU%, never a fabricated `0.0%`; cumulative time can still remain visible.
//!
//! The card's copy comes from [`ProcessInsightsLabels`] (supplied by the
//! Properties caller), matching the threads label slots the chrome already
//! threads through.

use gpui::{Div, ParentElement, Styled, div, px};
use taskmanager_application::{ProcessTelemetrySnapshot, ProcessThreadInfo};

use crate::core::device_state::DeviceStatus;
use crate::gpui_app::formatting;
use crate::gpui_app::theme::tokens;
use crate::gpui_app::theme::{Theme, mono_font_with_fallback};

use super::ProcessInsightsLabels;

/// Render one thread as `tid  comm  state  cpu-time`. The cpu-time field falls
/// back to an explicit dash when the source did not parse CPU counters, so a
/// gap is never misread as `0.0s`. An empty `comm` is the contract's unknown
/// identity (Windows ToolHelp32 exposes no thread names) and renders the same
/// explicit dash.
fn format_thread(thread: &ProcessThreadInfo) -> String {
    let cpu = thread
        .cpu_time_secs
        .map(|value| format!("{value:.1}s"))
        .unwrap_or_else(formatting::missing_value);
    let cpu_percent = thread
        .cpu_percent
        .map(|value| format!("{value:.1}%"))
        .unwrap_or_else(formatting::missing_value);
    let comm = if thread.comm.is_empty() {
        formatting::missing_value()
    } else {
        thread.comm.clone()
    };
    format!(
        "{}  {}  {}  {}  {}",
        thread.tid,
        comm,
        thread.state.as_short_label(),
        cpu,
        cpu_percent,
    )
}

/// The threads card. Surfaces the per-thread list or an explicit typed message
/// when the source is unavailable, denied, or empty.
pub(in crate::gpui_app::process_insights::view) fn threads_card(
    theme: &Theme,
    snapshot: &ProcessTelemetrySnapshot,
    labels: &ProcessInsightsLabels,
    width: f32,
) -> Div {
    let threads = &snapshot.threads;
    if threads.state.status != DeviceStatus::Healthy {
        return super::card(theme, labels.threads, width).child(
            div()
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(super::status_label(threads.state.status, labels).to_string()),
        );
    }
    let mut content = super::card(theme, labels.threads, width);
    if threads.threads.is_empty() {
        return content.child(
            div()
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(labels.no_threads.to_string()),
        );
    }
    content = content.child(
        div()
            .text_size(tokens::FONT_11)
            .text_color(theme.fg_dim)
            .child(format!("{} · {}", labels.threads, threads.threads.len())),
    );
    content = content.child(
        div()
            .text_size(tokens::FONT_10)
            .text_color(theme.fg_dim)
            .font(mono_font_with_fallback(theme))
            .child(format!(
                "{}  {}  {}  {}  {}",
                labels.thread_id,
                labels.thread_name,
                labels.thread_state,
                labels.thread_cpu_time,
                labels.thread_cpu_percent
            )),
    );
    let (shown, hidden) = super::capped_card_rows(threads.threads.len());
    content = content.child(
        div().flex().flex_col().gap(tokens::SPACE_3).children(
            threads
                .threads
                .iter()
                .take(shown)
                .map(format_thread)
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
    if hidden > 0 {
        content = content.child(crate::gpui_app::elements::more_rows_hint(theme, hidden));
    }
    content
}

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_gpui_app_process_insights_view_threads_tests.rs"]
mod tests;
