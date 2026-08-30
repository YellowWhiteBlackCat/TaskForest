//! Memoized process and application projections shared by the Iced view and
//! keyboard paths.

use std::rc::Rc;

use taskmanager_shell::{InfoSortCol, SortDir};

use crate::ui::process_projection::{ProcessProjection, ProcessProjectionFingerprint};

use super::{IcedApp, PerfDevice};

/// The inputs that can change the owned facts/order of one inventory table.
/// A query is deliberately not part of this fingerprint: changing a filter
/// reuses the already projected facts and only rebuilds the visible indices.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct InventoryDataFingerprint {
    pub(crate) watermark: u64,
    pub(crate) source_len: usize,
    pub(crate) sort: Option<(InfoSortCol, SortDir)>,
}

/// Memoized canonical inventory rows plus their current visible index list.
/// Rows are rebuilt only when the provider watermark or sort changes; a filter
/// change scans the owned facts but does not clone or re-sort them. The
/// renderer then uses the same indices for selection and widget rows.
#[derive(Clone)]
pub(crate) struct InventoryProjection<T> {
    data_fingerprint: Option<InventoryDataFingerprint>,
    query: String,
    rows: Rc<Vec<T>>,
    visible_indices: Rc<Vec<usize>>,
    generation: u64,
}

impl<T> Default for InventoryProjection<T> {
    fn default() -> Self {
        Self {
            data_fingerprint: None,
            query: String::new(),
            rows: Rc::new(Vec::new()),
            visible_indices: Rc::new(Vec::new()),
            generation: 0,
        }
    }
}

impl<T> InventoryProjection<T> {
    pub(crate) fn matches_data(&self, data_fingerprint: InventoryDataFingerprint) -> bool {
        self.data_fingerprint == Some(data_fingerprint)
    }

    pub(crate) fn replace_rows_and_project<M>(
        &mut self,
        data_fingerprint: InventoryDataFingerprint,
        query: &str,
        rows: Vec<T>,
        matches: M,
    ) -> (Rc<Vec<T>>, Rc<Vec<usize>>, u64)
    where
        M: Fn(&T, &str) -> bool,
    {
        self.rows = Rc::new(rows);
        self.visible_indices = Rc::new((0..self.rows.len()).collect());
        self.data_fingerprint = Some(data_fingerprint);
        self.query.clear();
        self.generation = self.generation.wrapping_add(1);
        self.project_query(query, matches)
    }

    pub(crate) fn project_query<M>(
        &mut self,
        query: &str,
        matches: M,
    ) -> (Rc<Vec<T>>, Rc<Vec<usize>>, u64)
    where
        M: Fn(&T, &str) -> bool,
    {
        let query = query.trim().to_lowercase();
        if self.query != query {
            self.visible_indices = Rc::new(
                self.rows
                    .iter()
                    .enumerate()
                    .filter_map(|(index, row)| matches(row, &query).then_some(index))
                    .collect(),
            );
            self.query = query;
            self.generation = self.generation.wrapping_add(1);
        }
        (
            Rc::clone(&self.rows),
            Rc::clone(&self.visible_indices),
            self.generation,
        )
    }
}

/// The App-history page's immutable renderer model.
#[derive(Clone, Default)]
pub(crate) struct AppHistoryMemo {
    pub(crate) rows: Rc<Vec<AppHistoryRowModel>>,
    pub(crate) generation: u64,
}

/// Owned App-history row data. The application group is already an owned
/// projection; the CPU history is copied only when the App-history fingerprint
/// misses and shared by the Canvas program thereafter.
#[derive(Clone)]
pub(crate) struct AppHistoryRowModel {
    pub(crate) row: taskmanager_application::ApplicationHistoryRow,
    pub(crate) samples: Rc<[f32]>,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct AppHistoryFingerprint {
    source_request: Option<taskmanager_application::HistoryReplayRequestId>,
    status: taskmanager_application::ApplicationHistoryStatus,
    selected_window: taskmanager_core::core::history::HistoryWindow,
    rows_window: Option<taskmanager_core::core::history::HistoryWindow>,
}

/// Complete invalidation identity for the materialized Performance rail.
/// Scroll geometry outside the current row window intentionally stays out of
/// this cache because it belongs to the independent viewport state.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct RailProjectionFingerprint {
    system_revision: u64,
    history_revision: u64,
    window: (usize, usize),
    devices: Vec<PerfDevice>,
    memory_units: (bool, bool),
    drive_units: (bool, bool),
    network_units: (bool, bool),
}

impl IcedApp {
    /// Fold the typed Performance-rail observations into an owned sidebar
    /// detail. The renderer only combines this already-projected value with
    /// the localized device-family label; it never reads a live observation
    /// while building the widget tree.
    pub(crate) fn performance_sidebar_detail(&self, device: PerfDevice) -> Option<String> {
        let snapshot = self.shell.projection().snapshot.as_ref();
        match device {
            PerfDevice::Cpu => snapshot
                .and_then(|snapshot| snapshot.cpu.current_global_usage_pct())
                .map(|value| format!("{value:.0}%")),
            PerfDevice::Memory => snapshot
                .and_then(|snapshot| snapshot.memory.used_percentage_observed())
                .map(|value| format!("{value:.0}%")),
            PerfDevice::Disk(index) => snapshot
                .and_then(|snapshot| snapshot.disks.get(index))
                .map(|disk| disk.name.clone()),
            PerfDevice::Network(index) => snapshot
                .and_then(|snapshot| snapshot.networks.get(index))
                .map(|network| network.interface_name.as_ref().to_owned()),
            PerfDevice::Gpu(index) => snapshot
                .and_then(|snapshot| snapshot.gpu.get(index))
                .map(|gpu| gpu.brand.clone()),
            PerfDevice::Battery(index) => self
                .shell
                .projection()
                .power_supplies
                .as_ref()
                .and_then(|power| power.batteries.get(index))
                .map(|battery| {
                    if battery.model_name.is_empty() {
                        battery.id.clone()
                    } else {
                        battery.model_name.clone()
                    }
                }),
            PerfDevice::Fan(index) => self
                .shell
                .projection()
                .sensors
                .as_ref()
                .and_then(|sensors| {
                    sensors
                        .readings
                        .iter()
                        .filter(|reading| {
                            reading.quantity()
                                == &taskmanager_core::core::sensors::SensorQuantity::FanSpeed
                        })
                        .nth(index)
                })
                .map(|fan| fan.label().to_owned()),
        }
    }

    /// Project only the current rail window. The canonical device list stays
    /// in the caller, while captions/history facts are built for visible cards
    /// plus overscan and memoized by data/unit/range identity.
    pub(crate) fn performance_rail_rows(
        &self,
        devices: &[PerfDevice],
        window: crate::ui::VirtualWindow,
    ) -> Rc<Vec<crate::ui::perf_rail::RailRow>> {
        let fingerprint = RailProjectionFingerprint {
            system_revision: self.shell.projection().system_revision,
            history_revision: self.shell.history.revision(),
            window: window.key(),
            devices: devices.to_vec(),
            memory_units: (self.memory_use_bytes(), self.memory_use_base2()),
            drive_units: (self.drive_use_bytes(), self.drive_use_base2()),
            network_units: (self.network_use_bytes(), self.network_use_base2()),
        };

        self.projection_caches.performance_rail(fingerprint, || {
            let visible = devices.get(window.start..window.end).unwrap_or(&[]);
            let device_samples = self.performance_rail_series(visible);
            let inputs = crate::ui::perf_rail::RailInputs {
                snapshot: self.shell.projection().snapshot.as_ref(),
                power: self.shell.projection().power_supplies.as_ref(),
                sensors: self.shell.projection().sensors.as_ref(),
                shell: &self.shell,
                device_samples: Some(&device_samples),
                cpu_samples: self.cached_metric_series(
                    taskmanager_shell::presentation::trend::TrendSeries::CpuUsagePercent,
                ),
                memory_samples: self.cached_metric_series(
                    taskmanager_shell::presentation::trend::TrendSeries::MemoryUsagePercent,
                ),
                memory_units: crate::ui::UnitPrefs {
                    use_bytes: self.memory_use_bytes(),
                    use_base2: self.memory_use_base2(),
                },
                drive_units: self.drive_units(),
                network_units: self.network_units(),
            };
            crate::ui::perf_rail::rail_rows(visible, &inputs)
        })
    }

    fn performance_rail_series(&self, devices: &[PerfDevice]) -> Vec<Option<Rc<[f32]>>> {
        let snapshot = self.shell.projection().snapshot.as_ref();
        let fans = self.shell.projection().sensors.as_ref().map(|sensors| {
            sensors
                .readings
                .iter()
                .filter(|reading| {
                    reading.quantity() == &taskmanager_core::core::sensors::SensorQuantity::FanSpeed
                })
                .collect::<Vec<_>>()
        });
        devices
            .iter()
            .map(|device| match *device {
                PerfDevice::Cpu | PerfDevice::Memory => None,
                PerfDevice::Disk(index) => snapshot
                    .and_then(|snapshot| snapshot.disks.get(index))
                    .map(|disk| {
                        self.cached_disk_series(&disk.device_id, disk.device_generation.get())
                    }),
                PerfDevice::Network(index) => snapshot
                    .and_then(|snapshot| snapshot.networks.get(index))
                    .map(|network| {
                        self.cached_network_series(
                            &network.device_id,
                            network.device_generation.get(),
                        )
                    }),
                PerfDevice::Gpu(index) => snapshot
                    .and_then(|snapshot| snapshot.gpu.get(index))
                    .map(|gpu| {
                        self.cached_gpu_utilization_series(
                            &gpu.device_id,
                            gpu.device_generation.get(),
                        )
                    }),
                PerfDevice::Battery(index) => self
                    .shell
                    .projection()
                    .power_supplies
                    .as_ref()
                    .and_then(|power| power.batteries.get(index))
                    .map(|battery| self.cached_battery_series(&battery.id)),
                PerfDevice::Fan(index) => fans
                    .as_ref()
                    .and_then(|fans| fans.get(index))
                    .map(|fan| self.cached_fan_series(fan.id())),
            })
            .collect()
    }

    /// Memoized Services rows and their filter projection. The shell remains
    /// the sorting authority; this cache only avoids repeating the same owned
    /// row/facts work on every Iced repaint.
    pub(crate) fn services_projection(
        &self,
        query: &str,
    ) -> (Rc<Vec<crate::ui::tables::ServiceRow>>, Rc<Vec<usize>>, u64) {
        let shell = &self.shell;
        let fingerprint = InventoryDataFingerprint {
            watermark: shell.projection().services_revision,
            source_len: shell.projection().services.as_ref().map_or(0, Vec::len),
            sort: shell.services_sort,
        };
        self.projection_caches.services(fingerprint, query, || {
            crate::ui::tables::service_rows(shell)
        })
    }

    /// Memoized Startup rows. Startup has no page-local filter, so its visible
    /// indices remain the full canonical sorted order until data/sort changes.
    pub(crate) fn startup_projection(
        &self,
    ) -> (
        Rc<Vec<crate::ui::startup_table::StartupRow>>,
        Rc<Vec<usize>>,
        u64,
    ) {
        let shell = &self.shell;
        let fingerprint = InventoryDataFingerprint {
            watermark: shell.projection().startup_revision,
            source_len: shell
                .projection()
                .startup_entries
                .as_ref()
                .map_or(0, Vec::len),
            sort: shell.startup_sort,
        };
        self.projection_caches.startup(fingerprint, || {
            crate::ui::startup_table::startup_rows(shell)
        })
    }

    /// Memoized Users rows. Shared process search remains a highlight-only
    /// concern on this page and never filters the canonical session list.
    pub(crate) fn users_projection(
        &self,
    ) -> (Rc<Vec<crate::ui::users::UserRow>>, Rc<Vec<usize>>, u64) {
        let shell = &self.shell;
        let fingerprint = InventoryDataFingerprint {
            watermark: shell.projection().sessions_revision,
            source_len: shell.projection().sessions.as_ref().map_or(0, Vec::len),
            sort: shell.sessions_sort,
        };
        self.projection_caches
            .users(fingerprint, || crate::ui::users::user_rows(shell))
    }

    /// Return the memoized renderer adaptation of the shared durable-history
    /// projection. Metric joining and ordering already happened in application.
    #[must_use]
    pub(crate) fn projected_app_history_model(&self) -> Rc<AppHistoryMemo> {
        let history = self.application_history_projection();
        let fingerprint = AppHistoryFingerprint {
            source_request: history.source_request,
            status: history.status,
            selected_window: history.selected_window,
            rows_window: history.rows_window,
        };
        self.projection_caches.app_history(fingerprint, || {
            Rc::new(
                history
                    .rows
                    .iter()
                    .map(|row| AppHistoryRowModel {
                        row: row.clone(),
                        samples: row.cpu_usage.as_ref().map_or_else(
                            || Rc::from([]),
                            |series| Rc::from(series.gap_aware_samples().as_ref()),
                        ),
                    })
                    .collect(),
            )
        })
    }

    /// The Applications-page visible-row projection for the current shell +
    /// frontend-local view state, memoized across vsync frames by a
    /// [`ProcessProjectionFingerprint`] (round-3 perf: skip the O(N)
    /// [`ProcessProjection::project`] rebuild on idle frames — only the ~1 Hz
    /// data tick or a view-state change forces a recompute).
    ///
    /// The view calls this on every render. On a fingerprint hit the cached
    /// [`ProcessProjection`] is reused as-is; on a miss the rows are rebuilt
    /// and the cache is updated. The projection output is byte-identical to
    /// what `ProcessProjection::project` would produce for the same inputs
    /// (the cache only decides whether to call it), so the renderer, the
    /// keyboard navigation paths, and the projection unit tests all see the
    /// same rows they did before the cache was introduced.
    ///
    /// Returns an owned `Rc`; view and keyboard paths can safely keep the
    /// projection through their operation without coupling dynamic borrows.
    #[must_use]
    pub(crate) fn projected_rows(&self) -> Rc<ProcessProjection> {
        self.projected_table_model().0
    }

    /// Return the memoized projection handle and its generation for a lazy
    /// Applications body. Cloning the `Rc` is O(1); the owned row facts and
    /// preformatted cells remain shared until the projection actually misses.
    #[must_use]
    pub(crate) fn projected_table_model(&self) -> (Rc<ProcessProjection>, u64) {
        let fingerprint = ProcessProjectionFingerprint::build_with_status(
            self.shell.projection().process_revision,
            self.shell.process_status_filter,
            self.shell.process_sort,
            &self.shell.query,
            &self.process_presentation.expanded_groups,
            &self.process_presentation.expanded_tree,
        )
        .with_local_time_rules(&self.local_time_rules);
        let observed_at_ms = self.shell.projection().processes_observed_at_ms;
        self.projection_caches.process_projection(fingerprint, || {
            // The shell already memoizes the filtered/sorted raw indices.
            // Materialize row references only when this Iced cache misses.
            let visible_indices = self.shell.visible_process_indices();
            let processes = self.shell.projection().processes_slice();
            let flat: Vec<_> = visible_indices
                .iter()
                .filter_map(|&index| processes.get(index))
                .collect();
            ProcessProjection::project_with_local_time(
                &flat,
                self.shell.process_sort,
                &self.process_presentation.expanded_groups,
                &self.process_presentation.expanded_tree,
                &self.local_time_rules,
                observed_at_ms,
            )
        })
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/projection_tests.rs"]
mod tests;
