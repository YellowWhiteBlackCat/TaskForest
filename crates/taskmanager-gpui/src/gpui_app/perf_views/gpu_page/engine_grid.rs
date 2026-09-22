//! Pure fold for the GPU page's per-engine mini-grid.
//!
//! This module owns which engine cards the grid shows and which utilization
//! value each card reads; the page renderer only paints the returned cells.
//! Keeping the fold separate lets the render contract (complete inventory,
//! accepted-session precedence, honest missing values, row bounding) be
//! asserted without a window while the paint path stays a 1:1 consumer.

use taskmanager_application::GpuEngineRowsState;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::GpuMetrics;
use taskmanager_platform_contract::CapabilityStatus;
use taskmanager_shell::presentation::gpu_engine_rows::{
    GpuEngineRowsPresentation, present_gpu_engine_rows,
};
use taskmanager_telemetry_store::CorrelatedSystemTelemetryHistory;

use crate::gpui_app::history_samples::gpu_engine_series_names;

/// One engine card the mini-grid paints.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GpuEngineMiniGridCell {
    /// The engine identity the card's graph and label render.
    pub(crate) name: String,
    /// The utilization percentage the card readout renders, or `None` when no
    /// finite observation exists (the shared missing marker paints instead).
    pub(crate) utilization_pct: Option<f32>,
}

/// The complete mini-grid one device paints: every visible card in row-major
/// paint order plus the identity count the header summarizes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GpuEngineMiniGridPresentation {
    /// One cell per visible card, in row-major paint order.
    pub(crate) cells: Vec<GpuEngineMiniGridCell>,
    /// Every engine the complete inventory names, visible or bounded away.
    pub(crate) total_engines: usize,
    /// How many cards share one painted row (1..=4).
    pub(crate) columns: usize,
}

impl GpuEngineMiniGridPresentation {
    /// How many cards the grid paints.
    #[must_use]
    pub(crate) fn visible_count(&self) -> usize {
        self.cells.len()
    }

    /// How many painted rows the visible cards occupy.
    #[must_use]
    pub(crate) fn row_count(&self) -> usize {
        self.cells.len().div_ceil(self.columns)
    }
}

/// Fold the engine inventory and the accepted session payload into the cards
/// the mini-grid paints. The inventory name union stays the authority for
/// *which* engines exist; the session payload is consulted first for each
/// card's utilization, with the snapshot's own engine observation as the
/// fallback. Non-finite values stay typed missing — never a fabricated zero.
///
/// `None` means the grid has nothing to paint (no named engine, or a row
/// budget that admits no complete row).
#[must_use]
pub(crate) fn present_gpu_engine_mini_grid(
    history: &CorrelatedSystemTelemetryHistory,
    metrics: &GpuMetrics,
    engine_session: &GpuEngineRowsState,
    engine_device_id: &DeviceId,
    engine_capability_status: Option<CapabilityStatus>,
    max_rows: Option<usize>,
) -> Option<GpuEngineMiniGridPresentation> {
    let mut engine_names = gpu_engine_series_names(history, metrics);
    let active_engines =
        match present_gpu_engine_rows(engine_session, engine_device_id, engine_capability_status) {
            GpuEngineRowsPresentation::Active(engines) => {
                for engine in engines {
                    if !engine_names.iter().any(|name| name == &engine.name) {
                        engine_names.push(engine.name.clone());
                    }
                }
                engine_names.sort_unstable();
                Some(engines)
            }
            _ => None,
        };
    if engine_names.is_empty() {
        return None;
    }
    let total_engines = engine_names.len();
    let columns = total_engines.clamp(1, 4);
    let visible_count = max_rows.map_or(total_engines, |rows| {
        total_engines.min(rows.saturating_mul(columns))
    });
    if visible_count == 0 {
        return None;
    }
    let cells = engine_names
        .into_iter()
        .take(visible_count)
        .map(|name| {
            let utilization_pct = active_engines
                .as_ref()
                .and_then(|engines| engines.iter().find(|engine| engine.name == name))
                .map(|engine| engine.utilization_pct)
                .or_else(|| {
                    metrics
                        .engines
                        .iter()
                        .find(|engine| engine.name == name)
                        .map(|engine| engine.usage_pct)
                })
                .filter(|value| value.is_finite());
            GpuEngineMiniGridCell {
                name,
                utilization_pct,
            }
        })
        .collect();
    Some(GpuEngineMiniGridPresentation {
        cells,
        total_engines,
        columns,
    })
}
