//! Render-entry row-projection memos owned by `RootView`.
//!
//! The ARCH rule (ADR-020) keeps projections at the render entry: the page
//! renders every frame while the platform lists tick at ~2 Hz, so the
//! filter/sort/aggregation work runs once per list change (or once per process
//! snapshot) and every hover/mouse/keyboard frame reuses the cached `Rc`.
//! [`super::projection_caches::GpuiProjectionCaches`] owns every memo and its
//! interior mutability; this module only supplies the root-level projections.

use std::rc::Rc;

use super::RootView;
use crate::gpui_app::{app_history_view, services_view, startup_view};
use taskmanager_core::core::services::ServiceItem;
use taskmanager_core::core::session::SessionItem;
use taskmanager_core::core::startup::StartupEntry;

use taskmanager_shell::InfoTable;

impl RootView {
    /// Memoized Services filter+sort projection (generation + filter + query +
    /// the shell-owned inventory sort key it, mirroring
    /// [`RootView::processes_projection`]'s memo): the page renders every
    /// frame while the platform list ticks ~2 Hz, so the filter+clone+order
    /// collapses to an `Rc` clone between changes.
    pub(crate) fn services_rows(&self) -> Rc<Vec<ServiceItem>> {
        let filter = self.services_state.filter;
        let query = self.services_state.query.trim().to_owned();
        let sort = self.inventory_sort(InfoTable::Services);
        let generation = self.services_generation();
        self.projection_caches
            .services(generation, filter, query.clone(), sort, || {
                services_view::sorted_services(self.services(), filter, &query, sort)
            })
    }

    /// Memoized Startup filter+sort projection (see
    /// [`RootView::services_rows`]); the sort key mirrors the shell-owned
    /// inventory slot.
    pub(crate) fn startup_rows(&self) -> Rc<Vec<StartupEntry>> {
        let filter = self.startup_state.filter;
        let query = self.startup_state.query.trim().to_owned();
        let sort = self.inventory_sort(InfoTable::Startup);
        let generation = self.startup_generation();
        self.projection_caches
            .startup(generation, filter, query.clone(), sort, || {
                startup_view::sorted_startup(self.startup_entries(), filter, &query, sort)
            })
    }

    /// Memoized Users/session projection keyed on the snapshot generation and
    /// the shell-owned inventory sort (`None` = provider order). Cloning the
    /// session vector in every render is still avoidable: the platform batch
    /// increments `sessions_generation` only when a new inventory snapshot is
    /// accepted, and the sort slot changes only on a header click.
    pub(crate) fn sessions_rows(&self) -> Rc<Vec<SessionItem>> {
        let sort = self.inventory_sort(InfoTable::Users);
        let generation = self.sessions_generation();
        self.projection_caches.sessions(generation, sort, || {
            let mut rows = self.sessions().to_vec();
            taskmanager_shell::order_session_rows(&mut rows, sort);
            rows
        })
    }

    /// Request-keyed GPUI projection of the shared durable application rows.
    /// The application layer already joined identities and metrics; this cache
    /// only converts Arc samples to GPUI's Rc graph identity once per load.
    pub(crate) fn app_history_rows(
        &self,
        projection: &taskmanager_application::ApplicationHistoryProjection,
    ) -> Rc<Vec<app_history_view::AppHistoryRow>> {
        self.projection_caches.app_history(&projection.rows, || {
            app_history_view::projected_app_history_rows(&projection.rows)
        })
    }
}
