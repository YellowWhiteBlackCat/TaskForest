//! Per-container aggregated CPU + memory rollup.
//!
//! Container *detection* (an [`IsolationKind`] badge + a container-id string
//! surfaced per process) already exists in the process-insights panel. This
//! module owns the *rollup*: aggregated cgroup-v2 CPU% and memory for each
//! discovered container cgroup, plus the member process ids that belong to it.
//!
//! Honesty contract (the project red line): the rollup never fabricates a
//! container or a zero. A system without cgroup-v2, or with no containers
//! running, yields an empty [`ContainerRollup::containers`] list with a typed
//! [`DeviceState`] explaining why (cgroup-v1 hosts are
//! `DeviceStatus::Unsupported`; a host with no container cgroups is a healthy
//! empty list). Per-field failures land as typed
//! [`ScalarObservation::unavailable`] rather than invented numbers.

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;
use crate::core::metrics::{ScalarAvailability, ScalarObservation};

use super::isolation::IsolationKind;

/// One aggregated container cgroup.
///
/// Identity (`id`, `cgroup_path`) is stable across samples for the same
/// runtime-placed cgroup so a UI row can retain its selection. `cpu_percentage`
/// is single-core-equivalent (mirrors the per-process CPU facet: a container
/// burning two whole cores reports `200.0`), and is `None` until a second
/// sample establishes a delta. `memory_bytes` is `memory.current`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerSummary {
    /// Stable rollup key: the cgroup-relative locator (for example
    /// `/docker/<id>` or `/system.slice/foo.service`). Two summaries never share
    /// an `id` within one rollup.
    pub id: String,
    /// Human-friendly label (the trailing path segment, or the container id when
    /// the runtime exposes one). Display-only — never used as an identity key.
    pub name: String,
    /// Runtime family inferred from the cgroup path / environment, when known.
    /// `None` means the cgroup looks like a container but no recognized runtime
    /// signature matched (typed "unknown container" rather than a guess).
    pub runtime: Option<IsolationKind>,
    /// Native cgroup-v2 locator relative to the unified mount root.
    pub cgroup_path: String,
    /// Aggregated CPU usage as a single-core-equivalent percentage. `None` on
    /// the first sample (no delta yet) or when `cpu.stat` was unreadable.
    pub cpu_percentage: ScalarObservation<f32>,
    /// `memory.current` for the cgroup, in bytes. Typed unavailable when the
    /// file is absent or unreadable — never a fabricated zero.
    pub memory_bytes: ScalarObservation<u64>,
    /// Member process ids whose `/proc/<pid>/cgroup` resolved to this cgroup.
    /// Empty when membership could not be enumerated (the cgroup was still
    /// rolled up from its own `cpu.stat`/`memory.current`).
    pub member_pids: Vec<u32>,
}

/// The full container rollup: the aggregated cgroup list plus a typed overall
/// state.
///
/// `state` describes the *collection* of containers as a whole: a cgroup-v1
/// host is `DeviceStatus::Unsupported`, a permission failure is
/// `DeviceStatus::PermissionDenied`, and a healthy host with no containers is
/// `DeviceStatus::Healthy` with an empty `containers` list.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ContainerRollup {
    /// Typed health of the rollup source (cgroup-v2 unified mount + scan).
    pub state: DeviceState,
    /// Aggregated containers, ordered by the collector (descending CPU% is the
    /// established convention). Empty is a real, healthy state on a
    /// container-free host.
    pub containers: Vec<ContainerSummary>,
}

impl ContainerRollup {
    /// A healthy rollup over no containers — the honest representation of a
    /// cgroup-v2 host where nothing is containerized.
    #[must_use]
    pub fn empty_healthy(now_ms: u64) -> Self {
        Self {
            state: DeviceState::healthy(now_ms),
            containers: Vec::new(),
        }
    }

    /// A rollup whose source is typed-unavailable (cgroup-v1, EACCES on the
    /// unified mount, ...). The container list is always empty here: a failed
    /// source must never retain fabricated rows.
    #[must_use]
    pub fn unavailable(state: DeviceState) -> Self {
        Self {
            state,
            containers: Vec::new(),
        }
    }

    /// True when at least one container has a current CPU or memory reading.
    /// The frontend uses this to distinguish "no containers" from "containers
    /// present but every field unavailable".
    #[must_use]
    pub fn has_current_reading(&self) -> bool {
        self.containers.iter().any(|container| {
            matches!(
                container.cpu_percentage.availability(),
                ScalarAvailability::Available | ScalarAvailability::Partial(_)
            ) || matches!(
                container.memory_bytes.availability(),
                ScalarAvailability::Available | ScalarAvailability::Partial(_)
            )
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_telemetry_containers_tests.rs"]
mod tests;
