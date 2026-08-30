//! Renderer projection-cache ownership for Iced.
//!
//! The component keeps interior mutability out of [`super::IcedApp`] and gives
//! each memo an explicit invalidation boundary. Callers receive owned `Rc`
//! handles, never a live `RefCell` guard, so view composition cannot create
//! overlapping dynamic borrows.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::trend::TrendSeries;

use crate::perf_history::{
    ProcessPerfHistory, ProcessPerfHistoryCache, ProcessPerfHistorySnapshot,
};
use crate::ui::process_projection::{ProcessProjection, ProcessProjectionFingerprint};

use super::history_series::{DeviceSeriesKey, HistorySeriesCache};
use super::projection::{
    AppHistoryFingerprint, AppHistoryMemo, AppHistoryRowModel, InventoryDataFingerprint,
    InventoryProjection, RailProjectionFingerprint,
};

#[derive(Default)]
struct ProcessProjectionMemo {
    fingerprint: Option<ProcessProjectionFingerprint>,
    projection: Rc<ProcessProjection>,
    generation: u64,
}

struct AppHistoryEntry {
    fingerprint: AppHistoryFingerprint,
    model: Rc<AppHistoryMemo>,
}

struct RailProjectionEntry {
    fingerprint: RailProjectionFingerprint,
    rows: Rc<Vec<crate::ui::perf_rail::RailRow>>,
}

/// The split-direction device families the two-series device graphs consume:
/// one disk's read/write windows and one NIC's rx/tx windows, cached as ONE
/// pair entry per device identity so the paired curves a chart strokes always
/// come from the same history epoch.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum DualDeviceSeriesFamily {
    DiskReadWrite,
    NetworkRxTx,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct DualSeriesKey {
    family: DualDeviceSeriesFamily,
    device_id: String,
    /// The viewed device generation: a row/ring generation flip must not
    /// serve the previous instance's cached pair even before the store
    /// revision moves.
    generation: u64,
}

#[derive(Clone)]
struct DualSeriesEntry {
    /// History epoch the pair was loaded under (`LiveGraphHistory::revision`
    /// already folds the visible capacity in; the explicit field keeps the
    /// cache key self-contained).
    revision: u64,
    capacity: usize,
    samples: (Rc<[f32]>, Rc<[f32]>),
}

fn project_inventory<T, M>(
    cache: &RefCell<InventoryProjection<T>>,
    fingerprint: InventoryDataFingerprint,
    query: &str,
    build: impl FnOnce() -> Vec<T>,
    matches: M,
) -> (Rc<Vec<T>>, Rc<Vec<usize>>, u64)
where
    M: Fn(&T, &str) -> bool,
{
    let rebuild = !cache.borrow().matches_data(fingerprint);
    if rebuild {
        // The renderer builder may re-enter another projection path. Never
        // keep a dynamic borrow alive across that callback.
        let rows = build();
        cache
            .borrow_mut()
            .replace_rows_and_project(fingerprint, query, rows, matches)
    } else {
        cache.borrow_mut().project_query(query, matches)
    }
}

/// The eight renderer-only memo domains owned by one Iced application.
///
/// - process performance: tracked pid + local ring revision;
/// - graph history: history revision + capacity (+ device identity);
/// - process table: typed process projection fingerprint;
/// - app history: process revision/list/query/status/sort;
/// - Services/Startup/Users: independent domain revision/list/sort/query;
/// - Performance rail: system/history revisions + materialized window,
///   device selector order and unit preferences.
#[derive(Default)]
pub(super) struct IcedProjectionCaches {
    process_performance: RefCell<Option<ProcessPerfHistoryCache>>,
    history: RefCell<HistorySeriesCache>,
    dual_device: RefCell<HashMap<DualSeriesKey, DualSeriesEntry>>,
    processes: RefCell<ProcessProjectionMemo>,
    app_history: RefCell<Option<AppHistoryEntry>>,
    services: RefCell<InventoryProjection<crate::ui::tables::ServiceRow>>,
    startup: RefCell<InventoryProjection<crate::ui::startup_table::StartupRow>>,
    users: RefCell<InventoryProjection<crate::ui::users::UserRow>>,
    performance_rail: RefCell<Option<RailProjectionEntry>>,
}

impl IcedProjectionCaches {
    pub(super) fn process_performance(
        &self,
        history: &ProcessPerfHistory,
    ) -> ProcessPerfHistorySnapshot {
        let mut cache = self.process_performance.borrow_mut();
        if let Some(entry) = cache.as_ref()
            && entry.identity == history.identity()
            && entry.revision == history.revision()
        {
            return entry.snapshot.clone();
        }
        let snapshot = history.snapshot();
        *cache = Some(ProcessPerfHistoryCache {
            identity: history.identity(),
            revision: history.revision(),
            snapshot: snapshot.clone(),
        });
        snapshot
    }

    pub(super) fn metric_series(&self, shell: &ShellApp, series: TrendSeries) -> Rc<[f32]> {
        self.history
            .borrow_mut()
            .get(shell, shell.history.revision(), series)
    }

    pub(super) fn per_core_series(&self, shell: &ShellApp) -> Rc<Vec<Rc<[f32]>>> {
        self.history
            .borrow_mut()
            .core(shell, shell.history.revision())
    }

    pub(super) fn device_series(
        &self,
        shell: &ShellApp,
        key: DeviceSeriesKey,
        load: impl FnOnce(&ShellApp) -> Vec<f32>,
    ) -> Rc<[f32]> {
        self.history
            .borrow_mut()
            .cached_device(shell, shell.history.revision(), key, load)
    }

    /// Shared split-direction window PAIR for one device's two-series graph
    /// (disk read/write, NIC rx/tx). Keyed by device identity + family +
    /// viewed generation and invalidated by history revision and visible
    /// capacity, so the paired curves always come from one epoch and the
    /// bounded `VecDeque`→slice copies happen once per device after a real
    /// history write — cache hits clone only the two `Rc` handles.
    pub(super) fn dual_device_series(
        &self,
        shell: &ShellApp,
        family: DualDeviceSeriesFamily,
        device_id: &str,
        generation: u64,
        load: impl FnOnce(&ShellApp) -> (Vec<f32>, Vec<f32>),
    ) -> (Rc<[f32]>, Rc<[f32]>) {
        let key = DualSeriesKey {
            family,
            device_id: device_id.to_owned(),
            generation,
        };
        let revision = shell.history.revision();
        let capacity = shell.history.capacity();
        if let Some(entry) = self.dual_device.borrow().get(&key)
            && entry.revision == revision
            && entry.capacity == capacity
        {
            return (Rc::clone(&entry.samples.0), Rc::clone(&entry.samples.1));
        }
        let (primary, secondary) = load(shell);
        let samples = (
            Rc::from(primary.into_boxed_slice()),
            Rc::from(secondary.into_boxed_slice()),
        );
        self.dual_device.borrow_mut().insert(
            key,
            DualSeriesEntry {
                revision,
                capacity,
                samples: (Rc::clone(&samples.0), Rc::clone(&samples.1)),
            },
        );
        samples
    }

    pub(super) fn process_projection(
        &self,
        fingerprint: ProcessProjectionFingerprint,
        build: impl FnOnce() -> ProcessProjection,
    ) -> (Rc<ProcessProjection>, u64) {
        {
            let cache = self.processes.borrow();
            if cache.fingerprint.as_ref() == Some(&fingerprint) {
                return (Rc::clone(&cache.projection), cache.generation);
            }
        }
        let projection = Rc::new(build());
        let mut cache = self.processes.borrow_mut();
        cache.fingerprint = Some(fingerprint);
        cache.projection = Rc::clone(&projection);
        cache.generation = cache.generation.wrapping_add(1);
        (projection, cache.generation)
    }

    pub(super) fn app_history(
        &self,
        fingerprint: AppHistoryFingerprint,
        build: impl FnOnce() -> Rc<Vec<AppHistoryRowModel>>,
    ) -> Rc<AppHistoryMemo> {
        if let Some(entry) = self.app_history.borrow().as_ref()
            && entry.fingerprint == fingerprint
        {
            return Rc::clone(&entry.model);
        }
        let generation = self
            .app_history
            .borrow()
            .as_ref()
            .map_or(1, |entry| entry.model.generation.wrapping_add(1));
        let rows = build();
        let model = Rc::new(AppHistoryMemo { rows, generation });
        *self.app_history.borrow_mut() = Some(AppHistoryEntry {
            fingerprint,
            model: Rc::clone(&model),
        });
        model
    }

    pub(super) fn services(
        &self,
        fingerprint: InventoryDataFingerprint,
        query: &str,
        build: impl FnOnce() -> Vec<crate::ui::tables::ServiceRow>,
    ) -> (Rc<Vec<crate::ui::tables::ServiceRow>>, Rc<Vec<usize>>, u64) {
        project_inventory(
            &self.services,
            fingerprint,
            query,
            build,
            crate::ui::tables::service_matches_lower,
        )
    }

    pub(super) fn startup(
        &self,
        fingerprint: InventoryDataFingerprint,
        build: impl FnOnce() -> Vec<crate::ui::startup_table::StartupRow>,
    ) -> (
        Rc<Vec<crate::ui::startup_table::StartupRow>>,
        Rc<Vec<usize>>,
        u64,
    ) {
        project_inventory(&self.startup, fingerprint, "", build, |_, _| true)
    }

    pub(super) fn users(
        &self,
        fingerprint: InventoryDataFingerprint,
        build: impl FnOnce() -> Vec<crate::ui::users::UserRow>,
    ) -> (Rc<Vec<crate::ui::users::UserRow>>, Rc<Vec<usize>>, u64) {
        project_inventory(&self.users, fingerprint, "", build, |_, _| true)
    }

    pub(super) fn performance_rail(
        &self,
        fingerprint: RailProjectionFingerprint,
        build: impl FnOnce() -> Vec<crate::ui::perf_rail::RailRow>,
    ) -> Rc<Vec<crate::ui::perf_rail::RailRow>> {
        if let Some(entry) = self.performance_rail.borrow().as_ref()
            && entry.fingerprint == fingerprint
        {
            return Rc::clone(&entry.rows);
        }
        let rows = Rc::new(build());
        *self.performance_rail.borrow_mut() = Some(RailProjectionEntry {
            fingerprint,
            rows: Rc::clone(&rows),
        });
        rows
    }
}

impl super::IcedApp {
    /// Shared split-direction windows (read, write) for the disk Performance
    /// page's two-series graph. The pair is cached as one entry per disk
    /// identity and viewed generation; see
    /// [`IcedProjectionCaches::dual_device_series`].
    #[must_use]
    pub(crate) fn cached_disk_split_series(
        &self,
        device_id: &str,
        generation: u64,
    ) -> (Rc<[f32]>, Rc<[f32]>) {
        self.projection_caches.dual_device_series(
            &self.shell,
            DualDeviceSeriesFamily::DiskReadWrite,
            device_id,
            generation,
            |shell| {
                (
                    shell
                        .history
                        .disk_read_bytes_per_sec_for(device_id, generation),
                    shell
                        .history
                        .disk_write_bytes_per_sec_for(device_id, generation),
                )
            },
        )
    }

    /// Shared split-direction windows (rx, tx) for the network Performance
    /// page's two-series graph; see [`IcedProjectionCaches::dual_device_series`].
    #[must_use]
    pub(crate) fn cached_network_split_series(
        &self,
        device_id: &str,
        generation: u64,
    ) -> (Rc<[f32]>, Rc<[f32]>) {
        self.projection_caches.dual_device_series(
            &self.shell,
            DualDeviceSeriesFamily::NetworkRxTx,
            device_id,
            generation,
            |shell| {
                (
                    shell
                        .history
                        .network_rx_bytes_per_sec_for(device_id, generation),
                    shell
                        .history
                        .network_tx_bytes_per_sec_for(device_id, generation),
                )
            },
        )
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/projection_caches_tests.rs"]
mod tests;
