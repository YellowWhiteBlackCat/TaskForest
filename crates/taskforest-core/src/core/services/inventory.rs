//! Provider-neutral service inventory contracts.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::relation::LegacyServiceRelationsWire;
use crate::core::{ServiceId, ServiceRelationGraph, ServiceRelationKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ServiceStatus {
    Active,
    Inactive,
    Failed,
    #[default]
    Unknown,
}

impl ServiceStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Inactive => "Inactive",
            Self::Failed => "Failed",
            Self::Unknown => "Unknown",
        }
    }
}

impl std::fmt::Display for ServiceStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<&str> for ServiceStatus {
    fn from(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "active" | "running" | "reloading" | "activating" | "started" => Self::Active,
            "inactive" | "dead" | "deactivating" | "stopped" => Self::Inactive,
            "failed" | "crashed" => Self::Failed,
            _ => Self::Unknown,
        }
    }
}

/// Optional diagnostic properties reported by a native service supervisor.
/// Each property is independently optional so a denied or older supervisor
/// cannot be mistaken for a clean service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ServiceDiagnostics {
    pub unit_file_state: Option<String>,
    pub result: Option<String>,
    pub exec_main_code: Option<String>,
    pub exec_main_status: Option<i32>,
    pub oom_killed: Option<bool>,
    /// Unit cgroup memory ceiling (`MemoryMax=`), when finite and observed.
    #[serde(default)]
    pub memory_max_bytes: Option<u64>,
    /// Unit cgroup current memory charge (`MemoryCurrent=`), when observed.
    #[serde(default)]
    pub memory_current_bytes: Option<u64>,
    pub start_limit_hit: Option<bool>,
    /// Whether the supervisor reports unit files changed since its last
    /// manager reload. This is manager-level evidence, not an inferred mtime.
    #[serde(default)]
    pub daemon_reload_required: Option<bool>,
    /// Next timer activation in the supervisor's display-ready time format.
    #[serde(default)]
    pub next_trigger_realtime: Option<String>,
    /// Next monotonic timer activation, in microseconds, when exposed.
    #[serde(default)]
    pub next_trigger_monotonic_usec: Option<u64>,
    /// Number of restarts observed by the supervisor.
    #[serde(default)]
    pub restart_count: Option<u64>,
    /// Start-limit window and burst budget, when systemd exposes them.
    #[serde(default)]
    pub start_limit_interval_usec: Option<u64>,
    #[serde(default)]
    pub start_limit_burst: Option<u32>,
    pub timeout_usec: Option<u64>,
    pub user: Option<String>,
    pub triggers: Vec<String>,
    pub triggered_by: Vec<String>,
}

impl ServiceDiagnostics {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// Return the most specific observed failure cause, preserving evidence
    /// order instead of inferring a failure from `ServiceStatus` alone.
    #[must_use]
    pub fn failure_cause(&self) -> Option<ServiceFailureCause> {
        if self.oom_killed == Some(true) {
            return Some(ServiceFailureCause::OomKilled);
        }
        if self.start_limit_hit == Some(true) || self.result.as_deref() == Some("start-limit-hit") {
            return Some(ServiceFailureCause::StartLimitHit);
        }
        if self
            .result
            .as_deref()
            .is_some_and(|result| matches!(result, "timeout" | "watchdog"))
        {
            return Some(ServiceFailureCause::Timeout);
        }
        if let Some(status) = self.exec_main_status.filter(|status| *status != 0) {
            return Some(ServiceFailureCause::ExitCode(status));
        }
        self.result
            .as_deref()
            .filter(|result| !result.is_empty() && *result != "success")
            .map(|result| ServiceFailureCause::Result(result.to_owned()))
    }

    #[must_use]
    pub fn has_socket_activation(&self) -> bool {
        self.triggers
            .iter()
            .chain(self.triggered_by.iter())
            .any(|unit| unit.ends_with(".socket"))
    }

    #[must_use]
    pub fn has_timer_activation(&self) -> bool {
        self.triggers
            .iter()
            .chain(self.triggered_by.iter())
            .any(|unit| unit.ends_with(".timer"))
    }

    /// Attribute an OOM only when the unit-limit evidence proves it. A
    /// system-wide kill cannot be inferred from `OOMKilled=yes` alone and is
    /// therefore returned as `Unknown` until a native supervisor supplies
    /// stronger evidence.
    #[must_use]
    pub fn oom_cause(&self) -> Option<ServiceOomCause> {
        if self.oom_killed != Some(true) {
            return None;
        }
        if self
            .memory_max_bytes
            .zip(self.memory_current_bytes)
            .is_some_and(|(limit, current)| limit > 0 && current >= limit)
        {
            Some(ServiceOomCause::UnitLimit)
        } else {
            Some(ServiceOomCause::Unknown)
        }
    }
}

/// Evidence-backed OOM scope for a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceOomCause {
    /// The observed cgroup charge reached the unit's finite MemoryMax.
    UnitLimit,
    /// A native supervisor supplied a system-wide kill attribution.
    SystemWide,
    /// OOMKilled is known, but the available facts do not prove its scope.
    Unknown,
}

/// Typed failure causes used by service details and alert explanations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceFailureCause {
    OomKilled,
    StartLimitHit,
    Timeout,
    ExitCode(i32),
    Result(String),
}

/// POSIX `sysexits.h` label for a service exit status. Unknown values retain
/// their numeric code and intentionally return `None`.
#[must_use]
pub const fn service_exit_status_label(status: i32) -> Option<&'static str> {
    match status {
        64 => Some("EX_USAGE"),
        65 => Some("EX_DATAERR"),
        66 => Some("EX_NOINPUT"),
        67 => Some("EX_NOUSER"),
        68 => Some("EX_NOHOST"),
        69 => Some("EX_UNAVAILABLE"),
        70 => Some("EX_SOFTWARE"),
        71 => Some("EX_OSERR"),
        72 => Some("EX_OSFILE"),
        73 => Some("EX_CANTCREAT"),
        74 => Some("EX_IOERR"),
        75 => Some("EX_TEMPFAIL"),
        76 => Some("EX_PROTOCOL"),
        77 => Some("EX_NOPERM"),
        78 => Some("EX_CONFIG"),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServiceItem {
    /// Provider-issued opaque authority target. Old serialized snapshots may
    /// default this field to empty for read-only display, but an empty target
    /// must never authorize a native operation.
    pub id: ServiceId,
    pub name: String,
    pub status: ServiceStatus,
    pub description: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    /// Optional supervisor diagnostics collected independently from the
    /// coarse inventory state.
    pub diagnostics: ServiceDiagnostics,
    relations: ServiceRelationGraph,
}

impl ServiceItem {
    /// Construct one provider-issued inventory row with no relationship facts.
    /// Attach a complete typed graph with [`Self::with_relations`] when the
    /// inventory provider observed relationships in the same refresh.
    #[must_use]
    pub fn from_inventory(
        id: impl Into<ServiceId>,
        name: impl Into<String>,
        status: ServiceStatus,
        description: impl Into<String>,
        load_state: impl Into<String>,
        active_state: impl Into<String>,
        sub_state: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            status,
            description: description.into(),
            load_state: load_state.into(),
            active_state: active_state.into(),
            sub_state: sub_state.into(),
            diagnostics: ServiceDiagnostics::default(),
            relations: ServiceRelationGraph::default(),
        }
    }

    /// Attach the canonical relationship assembly in one operation.
    #[must_use]
    pub fn with_relations(mut self, relations: ServiceRelationGraph) -> Self {
        self.relations = relations;
        self
    }

    /// Replace the relationship graph after a provider enrichment pass. The
    /// inventory row remains the same stable service identity; only the
    /// provider-confirmed adjacency facts are refreshed.
    pub fn replace_relations(&mut self, relations: ServiceRelationGraph) {
        self.relations = relations;
    }

    /// Attach optional supervisor diagnostics to this inventory row.
    #[must_use]
    pub fn with_diagnostics(mut self, diagnostics: ServiceDiagnostics) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &ServiceDiagnostics {
        &self.diagnostics
    }

    /// Read-only access to canonical inventory relationships.
    #[must_use]
    pub const fn relations(&self) -> &ServiceRelationGraph {
        &self.relations
    }

    /// Iterate canonical relationship targets without allocating a legacy
    /// compatibility string.
    pub fn relation_targets<'a>(
        &'a self,
        kind: &'a ServiceRelationKind,
    ) -> impl Iterator<Item = &'a ServiceId> + 'a {
        self.relations.targets(kind)
    }

    /// Read-only text projection for presentation and compatibility boundaries.
    #[must_use]
    pub fn relation_projection(&self, kind: &ServiceRelationKind) -> String {
        self.relations.joined_targets(kind)
    }
}

#[derive(Serialize, Deserialize)]
struct ServiceItemWire {
    #[serde(default)]
    id: ServiceId,
    name: String,
    status: ServiceStatus,
    description: String,
    load_state: String,
    active_state: String,
    sub_state: String,
    #[serde(flatten)]
    legacy: LegacyServiceRelationsWire,
    #[serde(default, skip_serializing_if = "ServiceRelationGraph::is_empty")]
    relations: ServiceRelationGraph,
    #[serde(default, skip_serializing_if = "ServiceDiagnostics::is_empty")]
    diagnostics: ServiceDiagnostics,
}

impl Serialize for ServiceItem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ServiceItemWire {
            id: self.id.clone(),
            name: self.name.clone(),
            status: self.status,
            description: self.description.clone(),
            load_state: self.load_state.clone(),
            active_state: self.active_state.clone(),
            sub_state: self.sub_state.clone(),
            legacy: LegacyServiceRelationsWire::from_relations(&self.relations),
            relations: self.relations.clone(),
            diagnostics: self.diagnostics.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ServiceItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ServiceItemWire::deserialize(deserializer)?;
        let mut relations = wire.relations;
        wire.legacy.hydrate_missing_kinds(&mut relations);
        Ok(Self {
            id: wire.id,
            name: wire.name,
            status: wire.status,
            description: wire.description,
            load_state: wire.load_state,
            active_state: wire.active_state,
            sub_state: wire.sub_state,
            diagnostics: wire.diagnostics,
            relations,
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_services_inventory_tests.rs"]
mod tests;
