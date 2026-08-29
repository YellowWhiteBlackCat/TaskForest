//! Performance-page history replay (roadmap #4 follow-up, read-only).
//!
//! Renders persisted series from the app-host history replay client
//! over a 1h/24h/7d window with fact-only peak summaries. The panel exists
//! ONLY when persistence is enabled and a query is wired at the composition
//! edge — no data source, no panel (nothing fabricated). Replay never feeds
//! alerts or live state; a query failure surfaces as typed text instead of a
//! blank graph.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, SharedString,
    Styled, Window, div, px,
};
use taskmanager_application::{
    HistoryReplayCompletion, HistoryReplayCompletionDisposition, HistoryReplayController,
    HistoryReplayRequest, HistoryReplayRequestId,
};
use taskmanager_core::core::{HistorySeriesKey, HistoryWindow};

use super::layout::performance_title_row;
use crate::gpui_app::elements;
use crate::gpui_app::graph::{GraphOpts, graph_element};
use crate::gpui_app::root::RootView;
use taskmanager_application::i18n;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

/// One replayed series: the stride-downsampled curve plus its fact-only
/// summary. Gaps stay `NaN` so the graph renders them as holes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HistoryReplayRow {
    pub key: HistorySeriesKey,
    /// Downsampled curve held as one shared `Rc` per completed load: the
    /// graph scene store keys cached geometry on the allocation's identity,
    /// so every frame between two loads replays instead of re-tessellating.
    pub samples: Rc<[f32]>,
    pub peak_value: Option<f64>,
    pub peak_measured_at_ms: Option<u64>,
    pub observed: usize,
    pub gaps: usize,
    pub clock_jumps: u32,
}

/// Renderer projection around the application-owned lifecycle. `rows` are a
/// request-keyed GPUI cache: the canonical payload remains in `controller`,
/// while this one-time `Arc` → `Rc` conversion preserves graph scene identity.
#[derive(Debug)]
pub(crate) struct HistoryReplayState {
    controller: HistoryReplayController,
    projected_request: Option<HistoryReplayRequestId>,
    rows: Vec<HistoryReplayRow>,
}

impl HistoryReplayState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            controller: HistoryReplayController::default(),
            projected_request: None,
            rows: Vec::new(),
        }
    }

    pub(crate) const fn is_open(&self) -> bool {
        self.controller.is_open()
    }

    pub(crate) const fn is_loading(&self) -> bool {
        self.controller.is_loading()
    }

    pub(crate) const fn window(&self) -> HistoryWindow {
        self.controller.selected_window()
    }

    pub(crate) fn rows(&self) -> &[HistoryReplayRow] {
        &self.rows
    }

    pub(crate) fn failure(&self) -> Option<&taskmanager_application::HistoryReplayError> {
        self.controller.failure()
    }

    pub(crate) fn loaded_at_ms(&self) -> Option<u64> {
        self.controller.loaded_at_ms()
    }

    pub(crate) fn rows_window(&self) -> Option<HistoryWindow> {
        self.controller.rows_window()
    }

    pub(crate) fn application_history_projection(
        &self,
        capability: taskmanager_application::ApplicationHistoryCapability,
    ) -> taskmanager_application::ApplicationHistoryProjection {
        self.controller.application_history_projection(capability)
    }

    pub(crate) fn open(&mut self) -> Option<HistoryReplayRequest> {
        self.controller.open().ok()
    }

    pub(crate) fn close(&mut self) {
        self.controller.close();
        self.sync_rows_projection();
    }

    fn refresh(&mut self) -> Option<HistoryReplayRequest> {
        self.controller.refresh().ok()
    }

    fn select_window(&mut self, window: HistoryWindow) -> Option<HistoryReplayRequest> {
        self.controller.select_window(window).ok()
    }

    pub(crate) fn reject_submission(
        &mut self,
        request: HistoryReplayRequest,
        error: taskmanager_application::HistoryReplayError,
    ) {
        let _ = self.controller.reject_submission(request, error);
        self.sync_rows_projection();
    }

    fn complete(&mut self, completion: HistoryReplayCompletion) -> bool {
        if self.controller.complete(completion) != HistoryReplayCompletionDisposition::Applied {
            return false;
        }
        self.sync_rows_projection();
        true
    }

    fn sync_rows_projection(&mut self) {
        let request = self.controller.rows_request_id();
        if request == self.projected_request {
            return;
        }
        self.rows = self
            .controller
            .rows()
            .iter()
            .filter(|row| !row.key.is_application_series())
            .map(|row| HistoryReplayRow {
                key: row.key.clone(),
                samples: Rc::from(row.samples.as_ref()),
                peak_value: row.peak_value,
                peak_measured_at_ms: row.peak_measured_at_ms,
                observed: row.observed,
                gaps: row.gaps,
                clock_jumps: row.clock_jumps,
            })
            .collect();
        self.projected_request = request;
    }
}

/// Human heading for one series: the metric slug plus its device/core scope,
/// stable across locales so tests and captures can key on it.
#[must_use]
pub(crate) fn row_heading(key: &HistorySeriesKey) -> String {
    let mut heading = key.metric().slug().to_owned();
    if let Some(device) = key.device() {
        heading.push_str(" · ");
        heading.push_str(device.as_str());
    }
    if let Some(core) = key.core_index() {
        heading.push_str(&format!(" · core {core}"));
    }
    heading
}

impl RootView {
    pub(crate) fn history_replay_state(&self) -> &HistoryReplayState {
        self.history_runtime.replay()
    }

    /// Whether the Performance main area currently renders the replay panel.
    /// Single source for the render branch AND its tests: open state alone is
    /// not enough — without a wired query there is no data source, so the
    /// page keeps rendering the live graphs.
    pub fn history_replay_visible(&self) -> bool {
        self.history_runtime.replay_available() && self.history_runtime.performance_replay_visible()
    }

    /// Whether the Performance page offers the replay entry toggle at all
    /// (persistence enabled and a query wired at the composition edge).
    pub fn history_replay_entry_available(&self) -> bool {
        self.history_runtime.replay_available()
    }

    pub fn history_replay_startup_unavailable(&self) -> bool {
        self.history_runtime.unavailable_reason().is_some()
    }

    /// Toggle the replay panel. Opening starts a load so the panel never
    /// shows rows from a previous session's window.
    pub fn toggle_history_replay(&mut self, cx: &mut gpui::Context<Self>) {
        if self.history_runtime.replay_available() {
            self.history_runtime.toggle_performance_presentation();
        }
        cx.notify();
    }

    /// Switch the replay window and reload (the window is part of the query).
    pub fn set_history_replay_window(
        &mut self,
        window: HistoryWindow,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(request) = self.history_runtime.replay_mut().select_window(window) {
            self.submit_history_replay_request(request);
            cx.notify();
        }
    }

    /// Submit a typed request to the app-host worker without performing file
    /// I/O or waiting on the UI thread.
    pub fn refresh_history_replay(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(request) = self.history_runtime.replay_mut().refresh() {
            self.submit_history_replay_request(request);
            cx.notify();
        }
    }

    pub(crate) fn drain_history_replay_completions(&mut self, cx: &mut gpui::Context<Self>) {
        let connector_changed = self.history_runtime.drain_connector();
        if connector_changed {
            self.sync_history_persistence_sink();
        }
        let Some(client) = self.history_runtime.replay_client_mut() else {
            if connector_changed {
                cx.notify();
            }
            return;
        };
        let completions = client.drain();
        let mut applied = false;
        for completion in completions {
            applied |= self.history_runtime.replay_mut().complete(completion);
        }
        self.sync_history_capture_readiness();
        if applied || connector_changed {
            cx.notify();
        }
    }

    fn submit_history_replay_request(&mut self, request: HistoryReplayRequest) {
        let error = self
            .history_runtime
            .replay_client_mut()
            .and_then(|client| client.try_request(request).err());
        if let Some(error) = error {
            self.history_runtime
                .replay_mut()
                .reject_submission(request, error);
        }
    }
}

/// Render the replay panel (the Performance main area while open). Buttons
/// mutate state through `RootView` methods on the entity handle; the panel
/// itself renders rows read-only.
pub(crate) fn render_history_replay(
    theme: &Theme,
    state: &HistoryReplayState,
    local_time_rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
    entity: gpui::Entity<RootView>,
) -> AnyElement {
    let window = state.window();
    let mut controls = div().flex().flex_row().items_center().gap(tokens::SPACE_6);
    for candidate in HistoryWindow::ALL {
        let label = history_window_label(candidate).to_string();
        let ent = entity.clone();
        controls = controls.child(elements::tool_btn(
            theme,
            SharedString::from(format!("tm-replay-window-{}", candidate as u8)),
            &label,
            candidate != window,
            candidate == window,
            move |_win: &mut Window, cx: &mut App| {
                ent.update(cx, |view, cx| {
                    if view.history_replay_state().window() != candidate {
                        view.set_history_replay_window(candidate, cx);
                    }
                });
            },
            move |_hovered: &bool, _win: &mut Window, _cx: &mut App| {},
        ));
    }
    let ent = entity.clone();
    controls = controls.child(elements::tool_btn(
        theme,
        "tm-replay-refresh",
        i18n::t("perf.replay.refresh"),
        true,
        false,
        move |_win: &mut Window, cx: &mut App| {
            ent.update(cx, |view, cx| {
                view.refresh_history_replay(cx);
            });
        },
        move |_hovered: &bool, _win: &mut Window, _cx: &mut App| {},
    ));

    let mut column = div()
        .id("tm-replay-panel")
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .size_full()
        .child(performance_title_row(
            theme,
            i18n::t("perf.replay.title").to_string(),
            i18n::t("perf.replay.subtitle").to_string(),
        ))
        .child(controls);

    // How old the shown snapshot is — a read-only replay is stale by design,
    // and the timestamp says so instead of implying liveness.
    if let Some(loaded_at_ms) = state.loaded_at_ms() {
        column = column.child(
            div()
                .id("tm-replay-loaded-at")
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(format!(
                    "{} {}",
                    i18n::t("perf.replay.loaded_at"),
                    format_loaded_at(loaded_at_ms, local_time_rules),
                )),
        );
    }
    if state.is_loading() {
        column = column.child(
            div()
                .id("tm-replay-loading")
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(i18n::t("perf.replay.loading")),
        );
    }
    if let Some(failure) = state.failure() {
        column = column.child(
            div()
                .id("tm-replay-failure")
                .text_color(theme.fg)
                .child(failure.to_string()),
        );
        if let Some(last_good_window) = state.rows_window()
            && last_good_window != state.window()
        {
            column = column.child(
                div()
                    .id("tm-replay-last-good-window")
                    .text_color(theme.fg_dim)
                    .child(format!(
                        "{}: {}",
                        i18n::t("perf.replay.last_good_window"),
                        history_window_label(last_good_window),
                    )),
            );
        }
    }
    if state.rows().is_empty() && !state.is_loading() {
        column = column.child(
            div()
                .id("tm-replay-empty")
                .text_color(theme.fg)
                .child(i18n::t("perf.replay.empty").to_string()),
        );
    } else {
        for (index, row) in state.rows().iter().enumerate() {
            column = column.child(replay_row(theme, row, index));
        }
    }
    column.into_any_element()
}

fn history_window_label(window: HistoryWindow) -> &'static str {
    i18n::t(match window {
        HistoryWindow::OneHour => "perf.replay.window.1h",
        HistoryWindow::TwentyFourHours => "perf.replay.window.24h",
        HistoryWindow::SevenDays => "perf.replay.window.7d",
    })
}

fn replay_row(theme: &Theme, row: &HistoryReplayRow, index: usize) -> AnyElement {
    let summary = match row.peak_value {
        Some(peak) => format!(
            "{} {} · {} {} · {} {}",
            i18n::t("perf.replay.peak"),
            format_peak(peak),
            i18n::t("perf.replay.observed"),
            row.observed,
            i18n::t("perf.replay.gaps"),
            row.gaps,
        ),
        None => i18n::t("perf.replay.no_measured").to_string(),
    };
    let clock_note = (row.clock_jumps > 0)
        .then(|| format!("{} {}", row.clock_jumps, i18n::t("perf.replay.clock_jumps")));
    div()
        .id(("tm-replay-row", index))
        .flex()
        .flex_col()
        .gap(tokens::SPACE_4)
        .child(
            div()
                .flex()
                .flex_row()
                .gap(tokens::SPACE_8)
                .child(row_heading(&row.key))
                .child(summary)
                .children(clock_note),
        )
        .child(div().h(px(72.0)).child(graph_element(
            (ElementId::from("tm-replay-graph"), row.key.file_stem()),
            Rc::clone(&row.samples),
            gpui::Rgba::from(series_color(theme, row.key.metric())),
            GraphOpts {
                gradient_fill: true,
                ref_lines: true,
                ..GraphOpts::default()
            },
        )))
        .into_any_element()
}

/// Peak formatting keeps the persisted unit-free axis honest: three
/// significant decimals, no invented unit suffix (series carry their unit in
/// the metric slug).
fn format_peak(peak: f64) -> String {
    if peak.abs() >= 100.0 {
        format!("{peak:.0}")
    } else if peak.abs() >= 1.0 {
        format!("{peak:.1}")
    } else {
        format!("{peak:.3}")
    }
}

/// Curve color follows the series' device family, mirroring the live
/// Performance pages' palette (fans use the accent the battery/fan views use).
fn series_color(
    theme: &Theme,
    metric: taskmanager_core::core::HistoryMetric,
) -> taskmanager_theme::Color {
    match metric {
        taskmanager_core::core::HistoryMetric::CpuUsagePct
        | taskmanager_core::core::HistoryMetric::CpuCoreUsagePct
        | taskmanager_core::core::HistoryMetric::CpuTemperatureC
        | taskmanager_core::core::HistoryMetric::CpuFrequencyMhz
        | taskmanager_core::core::HistoryMetric::CpuPowerW
        | taskmanager_core::core::HistoryMetric::ApplicationCpuUsagePct => theme.cpu,
        taskmanager_core::core::HistoryMetric::MemoryUsedPct
        | taskmanager_core::core::HistoryMetric::SwapUsedPct
        | taskmanager_core::core::HistoryMetric::ApplicationMemoryBytes => theme.memory,
        taskmanager_core::core::HistoryMetric::StorageActivityPct => theme.disk,
        taskmanager_core::core::HistoryMetric::NetworkRateBps => theme.network,
        taskmanager_core::core::HistoryMetric::GpuUsagePct
        | taskmanager_core::core::HistoryMetric::GpuPowerW
        | taskmanager_core::core::HistoryMetric::GpuTemperatureC
        | taskmanager_core::core::HistoryMetric::GpuFrequencyMhz => theme.gpu,
        taskmanager_core::core::HistoryMetric::BatteryCapacityPct
        | taskmanager_core::core::HistoryMetric::BatteryPowerW
        | taskmanager_core::core::HistoryMetric::BatteryHealthPct => theme.battery,
        taskmanager_core::core::HistoryMetric::FanRpm
        | taskmanager_core::core::HistoryMetric::FanPwmPct
        | taskmanager_core::core::HistoryMetric::FanTemperatureC => theme.accent,
        taskmanager_core::core::HistoryMetric::UptimeSecs
        | taskmanager_core::core::HistoryMetric::ProcessCount
        | taskmanager_core::core::HistoryMetric::ThreadCount
        | taskmanager_core::core::HistoryMetric::ApplicationProcessCount => theme.fg,
    }
}

/// Local wall-clock for the "data as of" line, projected only from the
/// composition-injected rule snapshot. Missing/out-of-range rules render the
/// shared unavailable marker; UTC is never relabeled as local.
fn format_loaded_at(
    loaded_at_ms: u64,
    local_time_rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
) -> String {
    taskmanager_shell::presentation::local_timestamp(loaded_at_ms, local_time_rules)
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_perf_views_history_replay_tests.rs"]
mod tests;
