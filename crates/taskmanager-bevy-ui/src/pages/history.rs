//! Read-only application-history page adapter for the Bevy frontend.
//!
//! This module is deliberately an integration-ready page, not a second
//! history authority. The frontend writer and replay worker stay behind
//! [`taskmanager_app_host::HistoryFrontendConnector`]; application owns the
//! typed `HistoryReplayController` and its stable-identity join. Bevy only
//! owns the lifecycle adapter, a bounded render model, and the `bsn!` scene.
//!
//! The `AppHistory` route is registered in `pages.rs`/`app.rs`; the window
//! composition installs `HistoryRuntime` plus `HistoryProjectionResource`.
//! The page still keeps the distinction between a mounted route and continuous
//! real in-process persistence evidence explicit.

use bevy::ecs::hierarchy::Children;
use std::sync::Arc;

use crate::app::{FrontendTrack, Route, RouteChanged, SharedRuntimeHandle};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;

use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Res, ResMut, SystemParam};
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, FlexDirection, JustifyContent, Node, UiRect, percent, px,
};
use bevy::ui::widget::Text;
use taskforest_core::core::history::{ApplicationHistoryIdentity, HistoryWindow};
use taskmanager_app_host::{
    HistoryFrontendConnector, HistoryFrontendConnectorStartError, HistoryFrontendRuntime,
};
use taskmanager_application::history_decimation::lttb_gap_preserving;
use taskmanager_application::i18n::t;
use taskmanager_application::{
    ApplicationHistoryCapability, ApplicationHistoryMetricSeries, ApplicationHistoryProjection,
    ApplicationHistoryStatus, MAX_HISTORY_REPLAY_POINTS, PerformanceHistoryProjection,
};

pub(crate) use taskmanager_shell::presentation::scalar_text;
use taskmanager_shell::presentation::{bytes, missing_value};

use crate::palette::{UiPalette, space_2, space_8, space_24};
use crate::window::{Role, TextRole};
use taskforest_core::core::history::HistoryRecordSink;
use taskmanager_application::ApplicationHistoryRow;

pub(crate) mod control;
pub(crate) mod scene;

#[derive(Clone, Component, Default)]
pub(crate) struct HistoryPageRoot;

#[derive(Clone, Component, Default)]
pub(crate) struct HistoryBody;

#[derive(Clone, Component, Default)]
pub(crate) struct HistoryStatusLine;

/// The page repeats the application-layer replay ceiling as a defensive
/// renderer bound. The application worker already publishes at most this
/// many points; keeping the guard here prevents a future wider payload from
/// turning one row into an unbounded UI allocation.
pub(crate) const MAX_RENDERED_HISTORY_POINTS: usize = MAX_HISTORY_REPLAY_POINTS;

/// One bounded metric view owned by the page model.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HistoryMetricView {
    pub(crate) samples: Arc<[f32]>,
    pub(crate) peak_value: Option<f64>,
    pub(crate) gaps: usize,
    pub(crate) clock_jumps: u32,
}

impl HistoryMetricView {
    fn from_series(series: &ApplicationHistoryMetricSeries) -> Self {
        Self {
            samples: bounded_samples(&series.gap_aware_samples()),
            peak_value: series.peak_value,
            gaps: series.gaps,
            clock_jumps: series.clock_jumps,
        }
    }

    #[must_use]
    pub(crate) fn finite_sample_count(&self) -> usize {
        self.samples
            .iter()
            .copied()
            .filter(|sample| sample.is_finite())
            .count()
    }
}

/// One row in the Bevy presentation model. `identity` remains typed all the
/// way to the row; the display string is derived from it and never becomes a
/// join key. Verified and fallback process-name identities therefore cannot
/// silently collapse into one row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HistoryRowModel {
    pub(crate) identity: ApplicationHistoryIdentity,
    pub(crate) display_name: String,
    pub(crate) verified: bool,
    pub(crate) cpu: Option<HistoryMetricView>,
    pub(crate) memory: Option<HistoryMetricView>,
    pub(crate) process_count: Option<HistoryMetricView>,
}

impl HistoryRowModel {
    fn from_application_row(row: &ApplicationHistoryRow) -> Self {
        Self {
            identity: row.identity.clone(),
            display_name: row.display_name().to_owned(),
            verified: row.identity.is_verified(),
            cpu: row.cpu_usage.as_ref().map(HistoryMetricView::from_series),
            memory: row.memory.as_ref().map(HistoryMetricView::from_series),
            process_count: row
                .process_count
                .as_ref()
                .map(HistoryMetricView::from_series),
        }
    }

    #[must_use]
    pub(crate) fn cpu_peak(&self) -> Option<f64> {
        self.cpu.as_ref().and_then(|metric| metric.peak_value)
    }

    #[must_use]
    pub(crate) fn memory_peak(&self) -> Option<f64> {
        self.memory.as_ref().and_then(|metric| metric.peak_value)
    }

    #[must_use]
    pub(crate) fn process_count_peak(&self) -> Option<f64> {
        self.process_count
            .as_ref()
            .and_then(|metric| metric.peak_value)
    }
}

/// Lifecycle detail that the page must keep visible instead of treating a
/// missing row set as a confirmed empty inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HistoryPageNotice {
    pub(crate) stale: bool,
    pub(crate) error_code: Option<&'static str>,
    pub(crate) unavailable_code: Option<&'static str>,
}

/// Immutable, bounded page model. It is derived from the application
/// projection on every accepted replay completion; it never reads the current
/// process list and never writes back to the replay/session authority.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HistoryPageModel {
    pub(crate) status: ApplicationHistoryStatus,
    pub(crate) selected_window: HistoryWindow,
    pub(crate) rows_window: Option<HistoryWindow>,
    pub(crate) refreshing: bool,
    pub(crate) rows: Vec<HistoryRowModel>,
    pub(crate) notice: HistoryPageNotice,
}

impl HistoryPageModel {
    #[must_use]
    pub(crate) fn from_projection(projection: &ApplicationHistoryProjection) -> Self {
        let stale = !projection.rows.is_empty()
            && (projection.refreshing
                || projection.rows_window != Some(projection.selected_window));
        let error_code = projection
            .failure
            .as_ref()
            .map(|failure| failure.kind().stable_code());
        Self {
            status: projection.status,
            selected_window: projection.selected_window,
            rows_window: projection.rows_window,
            refreshing: projection.refreshing,
            rows: projection
                .rows
                .iter()
                .map(HistoryRowModel::from_application_row)
                .collect(),
            notice: HistoryPageNotice {
                stale,
                error_code,
                unavailable_code: projection
                    .unavailable_reason
                    .map(|reason| reason.stable_code()),
            },
        }
    }

    #[must_use]
    pub(crate) fn has_visible_rows(&self) -> bool {
        !self.rows.is_empty()
    }
}

/// Keep the render-side envelope bounded even if a future application
/// projection violates its current worker bound. Preserve the complete
/// selected window and explicit gaps rather than keeping only a recent tail.
fn bounded_samples(samples: &[f32]) -> Arc<[f32]> {
    lttb_gap_preserving(samples, MAX_RENDERED_HISTORY_POINTS).into()
}

/// Format a byte peak with the same neutral missing-value semantics as the
/// other frontends.
pub(crate) fn memory_text(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or_else(missing_value, |value| {
            bytes(value.min(u64::MAX as f64) as u64)
        })
}

pub(crate) fn process_count_text(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or_else(missing_value, |value| format!("{value:.0}"))
}

fn window_label(window: HistoryWindow) -> &'static str {
    match window {
        HistoryWindow::OneHour => t("perf.replay.window.1h"),
        HistoryWindow::TwentyFourHours => t("perf.replay.window.24h"),
        HistoryWindow::SevenDays => t("perf.replay.window.7d"),
    }
}

/// The summary text intentionally names stale/error state even when last-good
/// rows remain visible. A successful prior window must not look like a fresh
/// result for the newly selected window.
pub(crate) fn summary_text(model: &HistoryPageModel) -> String {
    let mut parts = vec![
        format!("{} {}", model.rows.len(), t("history.application.title")),
        window_label(model.selected_window).to_owned(),
    ];
    if model.notice.stale || model.refreshing {
        parts.push(t("history.application.refreshing").to_owned());
    }
    if let Some(code) = model.notice.error_code {
        parts.push(format!("error={code}"));
    }
    if let Some(code) = model.notice.unavailable_code {
        parts.push(format!("unavailable={code}"));
    }
    parts.join(" · ")
}

fn row_provenance(row: &HistoryRowModel) -> &'static str {
    t(if row.verified {
        "history.application.verified"
    } else {
        "history.application.unverified"
    })
}

fn row_annotation(row: &HistoryRowModel) -> String {
    let gaps = row.cpu.as_ref().map_or(0, |metric| {
        metric.gaps
            + metric
                .samples
                .iter()
                .filter(|sample| sample.is_nan())
                .count()
    });
    let jumps = row.cpu.as_ref().map_or(0, |metric| metric.clock_jumps);
    match (gaps, jumps) {
        (0, 0) => row_provenance(row).to_owned(),
        (gaps, 0) => format!("{} · {gaps} gap(s)", row_provenance(row)),
        (0, jumps) => format!("{} · {jumps} clock jump(s)", row_provenance(row)),
        (gaps, jumps) => format!(
            "{} · {gaps} gap(s) · {jumps} clock jump(s)",
            row_provenance(row)
        ),
    }
}

// ---- read-only app-host connector lifecycle ----

/// Bevy's view of the shared history session lifecycle.
///
/// The lifecycle itself (connect handshake, capability projection, record
/// sink, replay request rule) belongs to
/// [`taskmanager_app_host::HistoryFrontendRuntime`]; this type only names the
/// frontend's entry point to it.
#[derive(Default)]
pub(crate) struct HistoryRuntime {
    runtime: HistoryFrontendRuntime,
}

impl HistoryRuntime {
    /// Install the app-host connector after composition. A failed connector is
    /// retained as a typed unavailable state only when history was requested;
    /// disabled history remains inert and performs no writer/replay work.
    pub(crate) fn install_connector(
        &mut self,
        result: Result<HistoryFrontendConnector, HistoryFrontendConnectorStartError>,
    ) {
        self.runtime.install_connector(result);
    }

    /// Change the canonical enabled preference without touching storage. The
    /// connector performs bounded persistence/replay bootstrap away from the
    /// UI thread.
    pub(crate) fn request(&mut self, enabled: bool) {
        self.runtime.request(enabled);
    }

    /// Whether the frontend asked for history persistence in this run.
    pub(crate) fn requested(&self) -> bool {
        self.runtime.requested()
    }

    #[must_use]
    pub(crate) fn projection(&self) -> ApplicationHistoryProjection {
        self.runtime.application_history_projection()
    }

    pub(crate) fn performance_projection(&self) -> PerformanceHistoryProjection {
        self.runtime.performance_history_projection()
    }

    pub(crate) fn available(&self) -> bool {
        self.runtime.is_active()
    }

    pub(crate) fn select_window(&mut self, window: HistoryWindow) {
        self.runtime.select_window(window);
    }

    pub(crate) fn refresh(&mut self) {
        self.runtime.refresh_replay();
    }

    /// Drain only non-blocking completion lanes. The returned flag is an ECS
    /// change fact for the projection system; idle frames do no scene work.
    /// Whether the history runtime's state advanced since the last drain.
    ///
    /// The answer belongs to the runtime owner
    /// ([`taskmanager_app_host::HistoryFrontendRuntime::drain_changes`]): the
    /// connector's outcome and the replay's completions are one question, not
    /// two answers each frontend has to combine.
    pub(crate) fn drain(&mut self) -> bool {
        self.runtime.drain_changes()
    }

    pub(crate) fn capability(&self) -> ApplicationHistoryCapability {
        self.runtime.capability()
    }

    pub(crate) fn record_sink(&self) -> Option<std::sync::Arc<dyn HistoryRecordSink>> {
        self.runtime.record_sink()
    }
}

/// updates after [`HistoryRuntime::drain`] reports a completion.
#[derive(Clone, Resource)]
pub(crate) struct HistoryProjectionResource(pub(crate) ApplicationHistoryProjection);

impl Default for HistoryProjectionResource {
    fn default() -> Self {
        Self(HistoryRuntime::default().projection())
    }
}

/// Triggered only when the read-only projection changes. Page observers use it
/// to rebuild their body; there is no frame polling or process-list join.
#[derive(Event)]
pub(crate) struct ApplicationHistoryChanged;

/// Mainline integration system: drain the app-host read lane and publish an
/// immutable projection snapshot into the Bevy world.
#[derive(SystemParam)]
pub(crate) struct HistoryTargets<'w> {
    projection: ResMut<'w, HistoryProjectionResource>,
    performance: ResMut<'w, control::PerformanceHistoryProjectionResource>,
    presentation: ResMut<'w, control::PerformancePresentation>,
    route: Option<Res<'w, Route>>,
}

pub(crate) fn drain_history_system(
    mut runtime: NonSendMut<HistoryRuntime>,
    mut track: NonSendMut<FrontendTrack>,
    mut targets: HistoryTargets,
    handle: Option<Res<SharedRuntimeHandle>>,
    mut commands: Commands,
) {
    let before = runtime.capability();
    if let Some(handle) = handle {
        let mut config = handle.shared.lock_config();
        if let Some(client) = config.as_mut() {
            let _ = client.drain();
            if let Some(snapshot) = client.snapshot()
                && runtime.requested() != snapshot.history_persistence
            {
                runtime.request(snapshot.history_persistence);
            }
        }
    }
    let changed = runtime.drain() || before != runtime.capability();
    if changed {
        track
            .shell
            .set_history_persistence_sink(runtime.record_sink());
        targets.projection.0 = runtime.projection();
        targets.performance.0 = runtime.performance_projection();
        commands.trigger(ApplicationHistoryChanged);
        commands.trigger(control::PerformanceHistoryChanged);
        if targets
            .route
            .as_ref()
            .is_some_and(|route| route.page == crate::app::Page::Settings)
        {
            commands.trigger(RouteChanged);
        }
    }
    if !runtime.available() && *targets.presentation != control::PerformancePresentation::Live {
        *targets.presentation = control::PerformancePresentation::Live;
        commands.trigger(control::PerformanceHistoryChanged);
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/history.rs"]
mod tests;
