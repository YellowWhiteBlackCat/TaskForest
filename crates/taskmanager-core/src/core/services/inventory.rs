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
            relations: ServiceRelationGraph::default(),
        }
    }

    /// Attach the canonical relationship assembly in one operation.
    #[must_use]
    pub fn with_relations(mut self, relations: ServiceRelationGraph) -> Self {
        self.relations = relations;
        self
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
            relations,
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_services_inventory_tests.rs"]
mod tests;
