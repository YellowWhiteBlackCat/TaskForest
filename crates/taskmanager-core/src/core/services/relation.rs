//! Platform-neutral service relationship metadata.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::core::ServiceId;

/// A relationship from the selected service to another service target.
///
/// Known variants cover relationships shared by current native adapters.
/// `Unknown` preserves a future or provider-specific wire name so an older
/// reader does not discard a relation it does not yet understand.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ServiceRelationKind {
    Requires,
    Wants,
    Requisite,
    BindsTo,
    PartOf,
    Conflicts,
    Before,
    After,
    WantedBy,
    RequiredBy,
    UpheldBy,
    Unknown(String),
}

impl ServiceRelationKind {
    #[must_use]
    pub fn from_wire_name(name: impl Into<String>) -> Self {
        let name = name.into();
        match name.as_str() {
            "requires" => Self::Requires,
            "wants" => Self::Wants,
            "requisite" => Self::Requisite,
            "binds_to" => Self::BindsTo,
            "part_of" => Self::PartOf,
            "conflicts" => Self::Conflicts,
            "before" => Self::Before,
            "after" => Self::After,
            "wanted_by" => Self::WantedBy,
            "required_by" => Self::RequiredBy,
            "upheld_by" => Self::UpheldBy,
            _ => Self::Unknown(name),
        }
    }

    #[must_use]
    pub fn as_wire_name(&self) -> &str {
        match self {
            Self::Requires => "requires",
            Self::Wants => "wants",
            Self::Requisite => "requisite",
            Self::BindsTo => "binds_to",
            Self::PartOf => "part_of",
            Self::Conflicts => "conflicts",
            Self::Before => "before",
            Self::After => "after",
            Self::WantedBy => "wanted_by",
            Self::RequiredBy => "required_by",
            Self::UpheldBy => "upheld_by",
            Self::Unknown(name) => name,
        }
    }
}

impl fmt::Display for ServiceRelationKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_wire_name())
    }
}

impl Serialize for ServiceRelationKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_wire_name())
    }
}

impl<'de> Deserialize<'de> for ServiceRelationKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from_wire_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServiceRelationEdge {
    pub kind: ServiceRelationKind,
    pub target: ServiceId,
}

impl ServiceRelationEdge {
    #[must_use]
    pub fn new(kind: ServiceRelationKind, target: impl Into<ServiceId>) -> Self {
        Self {
            kind,
            target: target.into(),
        }
    }
}

/// Adjacency edges for one selected service.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceRelationGraph {
    #[serde(default)]
    edges: Vec<ServiceRelationEdge>,
}

impl ServiceRelationGraph {
    #[must_use]
    pub fn from_edges(edges: impl IntoIterator<Item = ServiceRelationEdge>) -> Self {
        let mut graph = Self::default();
        for edge in edges {
            graph.push(edge);
        }
        graph
    }

    pub fn push(&mut self, edge: ServiceRelationEdge) {
        if !self.edges.contains(&edge) {
            self.edges.push(edge);
        }
    }

    pub fn replace_targets(
        &mut self,
        kind: ServiceRelationKind,
        targets: impl IntoIterator<Item = ServiceId>,
    ) {
        self.edges.retain(|edge| edge.kind != kind);
        for target in targets {
            self.push(ServiceRelationEdge::new(kind.clone(), target));
        }
    }

    #[must_use]
    pub fn edges(&self) -> &[ServiceRelationEdge] {
        &self.edges
    }

    pub fn targets<'a>(
        &'a self,
        kind: &'a ServiceRelationKind,
    ) -> impl Iterator<Item = &'a ServiceId> + 'a {
        self.edges
            .iter()
            .filter(move |edge| &edge.kind == kind)
            .map(|edge| &edge.target)
    }

    #[must_use]
    pub fn contains_kind(&self, kind: &ServiceRelationKind) -> bool {
        self.edges.iter().any(|edge| &edge.kind == kind)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.edges.len()
    }

    pub(super) fn joined_targets(&self, kind: &ServiceRelationKind) -> String {
        self.targets(kind)
            .map(ServiceId::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Canonical service relationship metadata.
///
/// The typed graph is the only domain authority. The four historical string
/// fields exist only in the private serde wire DTO and are projected from this
/// graph when older readers need them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceDeps {
    relations: ServiceRelationGraph,
}

impl ServiceDeps {
    #[must_use]
    pub fn from_relations(relations: ServiceRelationGraph) -> Self {
        Self { relations }
    }

    /// Read-only access to the canonical relationship graph.
    #[must_use]
    pub const fn relations(&self) -> &ServiceRelationGraph {
        &self.relations
    }

    /// Read-only typed projection for frontends and other consumers.
    pub fn relation_targets<'a>(
        &'a self,
        kind: &'a ServiceRelationKind,
    ) -> impl Iterator<Item = &'a ServiceId> + 'a {
        self.relations.targets(kind)
    }

    /// Space-separated compatibility projection derived from the graph.
    ///
    /// Prefer [`Self::relation_targets`] for new domain and UI logic. This
    /// projection is useful at text/wire boundaries and can never become a
    /// second writable authority.
    #[must_use]
    pub fn relation_projection(&self, kind: &ServiceRelationKind) -> String {
        self.relations.joined_targets(kind)
    }

    pub fn replace_relation_targets(
        &mut self,
        kind: ServiceRelationKind,
        targets: impl IntoIterator<Item = ServiceId>,
    ) {
        self.relations.replace_targets(kind, targets);
    }
}

/// Shared compatibility projection for both dependency-detail and inventory
/// payloads. Keeping the four historical strings in one private helper makes
/// typed-wins merge behavior identical at both wire boundaries.
#[derive(Default, Serialize, Deserialize)]
pub(super) struct LegacyServiceRelationsWire {
    #[serde(default)]
    requires: String,
    #[serde(default)]
    wants: String,
    #[serde(default)]
    wanted_by: String,
    #[serde(default)]
    after: String,
}

impl LegacyServiceRelationsWire {
    pub(super) fn from_relations(relations: &ServiceRelationGraph) -> Self {
        Self {
            requires: relations.joined_targets(&ServiceRelationKind::Requires),
            wants: relations.joined_targets(&ServiceRelationKind::Wants),
            wanted_by: relations.joined_targets(&ServiceRelationKind::WantedBy),
            after: relations.joined_targets(&ServiceRelationKind::After),
        }
    }

    pub(super) fn hydrate_missing_kinds(self, relations: &mut ServiceRelationGraph) {
        for (kind, projection) in [
            (ServiceRelationKind::Requires, self.requires),
            (ServiceRelationKind::Wants, self.wants),
            (ServiceRelationKind::WantedBy, self.wanted_by),
            (ServiceRelationKind::After, self.after),
        ] {
            if !relations.contains_kind(&kind) {
                relations.replace_targets(kind, projection.split_whitespace().map(ServiceId::new));
            }
        }
    }
}

/// Compatibility-only JSON shape. Keeping legacy fields here prevents wire
/// compatibility from leaking writable strings back into the domain model.
#[derive(Serialize, Deserialize)]
struct ServiceDepsWire {
    #[serde(flatten)]
    legacy: LegacyServiceRelationsWire,
    #[serde(default, skip_serializing_if = "ServiceRelationGraph::is_empty")]
    relations: ServiceRelationGraph,
}

impl Serialize for ServiceDeps {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ServiceDepsWire {
            legacy: LegacyServiceRelationsWire::from_relations(&self.relations),
            relations: self.relations.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ServiceDeps {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ServiceDepsWire::deserialize(deserializer)?;
        let mut relations = wire.relations;
        wire.legacy.hydrate_missing_kinds(&mut relations);
        Ok(Self { relations })
    }
}
