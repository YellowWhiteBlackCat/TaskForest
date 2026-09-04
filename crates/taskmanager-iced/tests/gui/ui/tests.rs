//! Headless behavior tests for the renderer-edge page projections.
//!
//! Split by topic so no one file holds the whole suite:
//! - [`devices`]: the Performance-device section projections (GPU / Disk /
//!   Network / Fan) and the select-a-device selector.
//! - [`devices_visibility`]: the settings-driven ShowDevice filters over the
//!   select-a-device rail and the preference-aware rate formatter (extracted
//!   from `devices`).
//! - [`battery`]: the Battery section projection + the select-Battery selector
//!   routing (extracted from `devices`).
//! - [`perf_device_graphs`]: the per-device mini-graph data wiring (each device
//!   row plots its OWN per-device window) + the section render paths for the
//!   collecting and plotted states.
//! - [`pages`]: the pure formatting/column projections and the Services /
//!   Startup / System / export / modal / overlay render paths.
//! - [`process_sparkline`]: the per-row CPU sparkline geometry, the cross-frame
//!   fingerprint/cache-clear gate, the projection `depth` field, and the
//!   non-sortable Trend header layout.
//! - [`views`]: the App-history page and canonical category-tree projection.

pub(crate) use super::performance::{
    CompactDetailViewport, PerfDetail, available_perf_devices, bounded_sidebar_label, chunk_count,
    compact_detail_viewport, perf_detail_kind, performance_sidebar_label, resolved_perf_device,
};
use super::process_projection::ProcessProjection;
use super::tables::{ServiceRow, service_matches_lower};
use super::*;
use crate::ui::applications::applications_table_rows_range;
use crate::ui::applications::rows::RowRender;

/// Build the Applications data rows from the shared visible-row projection.
/// The projection is the single source of render order (and of the keyboard
/// navigation order in [`crate::app`]): every row carries its `flat_index`
/// into the shared process list, so selection, focus, and the shared action
/// paths always resolve to the process the row actually renders. This is the
/// pure dispatch the headless tests assert on (no pixel read-back); the row
/// builders are the seams.
pub(crate) fn applications_table_rows(
    ctx: &RowRender,
    projection: &ProcessProjection,
) -> Vec<Element<'static, Message, iced::Theme, iced::Renderer>> {
    applications_table_rows_range(ctx, projection, 0, projection.rows().len())
}

/// The Services-page name/description filter: case-insensitive substring
/// match over both columns; the empty query keeps every row. Pure function so
/// the headless tests assert the filter without rendering.
#[must_use]
pub(super) fn filtered_services<'a>(rows: &'a [ServiceRow], query: &str) -> Vec<&'a ServiceRow> {
    let query = query.trim().to_lowercase();
    rows.iter()
        .filter(|service| service_matches_lower(service, &query))
        .collect()
}

#[path = "tests/battery.rs"]
mod battery;
#[path = "tests/devices.rs"]
mod devices;
#[path = "tests/devices_visibility.rs"]
mod devices_visibility;
#[path = "tests/pages.rs"]
mod pages;
#[path = "tests/perf_device_graphs.rs"]
mod perf_device_graphs;
#[path = "tests/process_sparkline.rs"]
mod process_sparkline;
#[path = "tests/views.rs"]
mod views;

#[path = "tests/gpu_engines_and_smart_test.rs"]
mod gpu_engines_and_smart_test;
