//! Platform-neutral service relationship metadata and dependency edge definitions.
//!
//! Provides typed definitions for service relationships (`Requires`, `Wants`,
//! `Before`, `After`, etc.), directional edge semantics, and graph algorithms
//! for cycle detection across service ordering and requirement networks.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::core::ServiceId;

mod algorithms;
pub use algorithms::{
    detect_directed_cycles, detect_ordering_cycles, detect_requirement_cycles, is_ordering_acyclic,
    is_requirement_acyclic,
};

/// Category of service relationship edge for graph analysis and cycle detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServiceEdgeCategory {
    /// Start/stop sequencing constraint (`Before`, `After`).
    Ordering,
    /// Activation dependency constraint (`Requires`, `Wants`, `Requisite`, `BindsTo`, `PartOf`, `WantedBy`, `RequiredBy`, `UpheldBy`).
    Requirement,
    /// Mutual exclusion constraint (`Conflicts`).
    Conflict,
    /// Unclassified or provider-specific relation.
    Other,
}

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
    /// Wire-vocabulary decode for the private serde ingress only; the domain
    /// speaks the typed kind, never the dependency-string.
    #[must_use]
    pub(crate) fn from_wire_name(name: impl Into<String>) -> Self {
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
    pub(crate) fn as_wire_name(&self) -> &str {
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

    /// Classification of this relationship kind into its structural graph role.
    #[must_use]
    pub fn category(&self) -> ServiceEdgeCategory {
        match self {
            Self::Before | Self::After => ServiceEdgeCategory::Ordering,
            Self::Requires
            | Self::Wants
            | Self::Requisite
            | Self::BindsTo
            | Self::PartOf
            | Self::WantedBy
            | Self::RequiredBy
            | Self::UpheldBy => ServiceEdgeCategory::Requirement,
            Self::Conflicts => ServiceEdgeCategory::Conflict,
            Self::Unknown(_) => ServiceEdgeCategory::Other,
        }
    }

    /// Whether this relationship specifies an execution/startup ordering constraint (`Before`, `After`).
    #[must_use]
    pub fn is_ordering(&self) -> bool {
        matches!(self, Self::Before | Self::After)
    }

    /// Whether this relationship specifies an activation requirement (`Requires`, `Wants`, etc.).
    #[must_use]
    pub fn is_requirement(&self) -> bool {
        matches!(
            self,
            Self::Requires
                | Self::Wants
                | Self::Requisite
                | Self::BindsTo
                | Self::PartOf
                | Self::WantedBy
                | Self::RequiredBy
                | Self::UpheldBy
        )
    }

    /// Whether this relationship is a strict constraint that cannot be ignored.
    ///
    /// Ordering constraints (`Before`, `After`) and hard requirements (`Requires`,
    /// `Requisite`, `BindsTo`, `RequiredBy`) are strict. `Wants` and `WantedBy`
    /// are advisory/weak.
    #[must_use]
    pub fn is_strict(&self) -> bool {
        match self {
            Self::Before
            | Self::After
            | Self::Requires
            | Self::Requisite
            | Self::BindsTo
            | Self::RequiredBy => true,
            Self::Wants
            | Self::WantedBy
            | Self::PartOf
            | Self::UpheldBy
            | Self::Conflicts
            | Self::Unknown(_) => false,
        }
    }

    /// Whether this relationship is weak/advisory (`Wants`, `WantedBy`).
    #[must_use]
    pub fn is_weak(&self) -> bool {
        matches!(self, Self::Wants | Self::WantedBy)
    }

    /// Whether this relationship is `Before`.
    #[must_use]
    pub fn is_before(&self) -> bool {
        matches!(self, Self::Before)
    }

    /// Whether this relationship is `After`.
    #[must_use]
    pub fn is_after(&self) -> bool {
        matches!(self, Self::After)
    }

    /// Whether this relationship is `Requires`.
    #[must_use]
    pub fn is_requires(&self) -> bool {
        matches!(self, Self::Requires)
    }

    /// Whether this relationship is `Wants`.
    #[must_use]
    pub fn is_wants(&self) -> bool {
        matches!(self, Self::Wants)
    }

    /// Dual/reciprocal relationship on the opposite endpoint, if defined.
    ///
    /// For instance, `A Before B` corresponds to `B After A`, and `A Requires B`
    /// corresponds to `B RequiredBy A`.
    #[must_use]
    pub fn inverse(&self) -> Option<Self> {
        match self {
            Self::Before => Some(Self::After),
            Self::After => Some(Self::Before),
            Self::Requires => Some(Self::RequiredBy),
            Self::RequiredBy => Some(Self::Requires),
            Self::Wants => Some(Self::WantedBy),
            Self::WantedBy => Some(Self::Wants),
            Self::Conflicts => Some(Self::Conflicts),
            Self::Requisite | Self::BindsTo | Self::PartOf | Self::UpheldBy | Self::Unknown(_) => {
                None
            }
        }
    }

    /// Determine the directed start-order precedence `(earlier, later)` where `earlier`
    /// must start before `later`.
    ///
    /// Returns `None` if this relation is not an ordering constraint (`Before` / `After`).
    #[must_use]
    pub fn ordering_precedence<'a>(
        &self,
        origin: &'a ServiceId,
        target: &'a ServiceId,
    ) -> Option<(&'a ServiceId, &'a ServiceId)> {
        match self {
            Self::Before => Some((origin, target)),
            Self::After => Some((target, origin)),
            _ => None,
        }
    }

    /// Determine the directed requirement dependency `(dependent, prerequisite)`
    /// where `dependent` requires or wants `prerequisite` to be activated.
    ///
    /// Returns `None` if this relation is not a requirement constraint.
    #[must_use]
    pub fn requirement_dependency<'a>(
        &self,
        origin: &'a ServiceId,
        target: &'a ServiceId,
    ) -> Option<(&'a ServiceId, &'a ServiceId)> {
        match self {
            Self::Requires | Self::Wants | Self::Requisite | Self::BindsTo | Self::PartOf => {
                Some((origin, target))
            }
            Self::RequiredBy | Self::WantedBy | Self::UpheldBy => Some((target, origin)),
            _ => None,
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

    /// Construct an ordering edge asserting that the origin service starts before `target`.
    #[must_use]
    pub fn before(target: impl Into<ServiceId>) -> Self {
        Self::new(ServiceRelationKind::Before, target)
    }

    /// Construct an ordering edge asserting that the origin service starts after `target`.
    #[must_use]
    pub fn after(target: impl Into<ServiceId>) -> Self {
        Self::new(ServiceRelationKind::After, target)
    }

    /// Construct a strict activation requirement edge on `target`.
    #[must_use]
    pub fn requires(target: impl Into<ServiceId>) -> Self {
        Self::new(ServiceRelationKind::Requires, target)
    }

    /// Construct an advisory/weak requirement edge on `target`.
    #[must_use]
    pub fn wants(target: impl Into<ServiceId>) -> Self {
        Self::new(ServiceRelationKind::Wants, target)
    }

    #[must_use]
    pub const fn kind(&self) -> &ServiceRelationKind {
        &self.kind
    }

    #[must_use]
    pub const fn target(&self) -> &ServiceId {
        &self.target
    }

    #[must_use]
    pub fn is_ordering(&self) -> bool {
        self.kind.is_ordering()
    }

    #[must_use]
    pub fn is_requirement(&self) -> bool {
        self.kind.is_requirement()
    }

    #[must_use]
    pub fn is_strict(&self) -> bool {
        self.kind.is_strict()
    }

    #[must_use]
    pub fn is_weak(&self) -> bool {
        self.kind.is_weak()
    }

    #[must_use]
    pub fn is_before(&self) -> bool {
        self.kind.is_before()
    }

    #[must_use]
    pub fn is_after(&self) -> bool {
        self.kind.is_after()
    }

    #[must_use]
    pub fn is_requires(&self) -> bool {
        self.kind.is_requires()
    }

    #[must_use]
    pub fn is_wants(&self) -> bool {
        self.kind.is_wants()
    }

    /// Compute directed start-order precedence `(earlier, later)` where `earlier`
    /// must start before `later`. Returns `None` if this edge is not an ordering constraint.
    #[must_use]
    pub fn ordering_precedence<'a>(
        &'a self,
        origin: &'a ServiceId,
    ) -> Option<(&'a ServiceId, &'a ServiceId)> {
        self.kind.ordering_precedence(origin, &self.target)
    }

    /// Compute directed activation requirement dependency `(dependent, prerequisite)`
    /// where `dependent` requires or wants `prerequisite`. Returns `None` if this edge
    /// is not a requirement constraint.
    #[must_use]
    pub fn requirement_dependency<'a>(
        &'a self,
        origin: &'a ServiceId,
    ) -> Option<(&'a ServiceId, &'a ServiceId)> {
        self.kind.requirement_dependency(origin, &self.target)
    }

    /// Convert this edge into a canonical directed ordering edge, oriented so `source` precedes `target`.
    #[must_use]
    pub fn to_directed_ordering_edge(&self, origin: &ServiceId) -> Option<DirectedServiceEdge> {
        self.ordering_precedence(origin)
            .map(|(src, tgt)| DirectedServiceEdge::new(src.clone(), tgt.clone(), self.kind.clone()))
    }

    /// Convert this edge into a canonical directed requirement edge, oriented so `source` depends on `target`.
    #[must_use]
    pub fn to_directed_requirement_edge(&self, origin: &ServiceId) -> Option<DirectedServiceEdge> {
        self.requirement_dependency(origin)
            .map(|(src, tgt)| DirectedServiceEdge::new(src.clone(), tgt.clone(), self.kind.clone()))
    }

    /// Compute the reciprocal relation on `target` pointing back to `origin`, if an inverse exists.
    #[must_use]
    pub fn reciprocal(&self, origin: &ServiceId) -> Option<(ServiceId, Self)> {
        self.kind
            .inverse()
            .map(|inv| (self.target.clone(), Self::new(inv, origin.clone())))
    }
}

/// A canonical directed edge between two services for graph traversal and cycle detection.
///
/// In an ordering graph, `source` must precede `target` in execution/start sequence.
/// In a requirement graph, `source` depends on `target` being activated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DirectedServiceEdge {
    pub source: ServiceId,
    pub target: ServiceId,
    pub kind: ServiceRelationKind,
}

impl DirectedServiceEdge {
    #[must_use]
    pub fn new(
        source: impl Into<ServiceId>,
        target: impl Into<ServiceId>,
        kind: ServiceRelationKind,
    ) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            kind,
        }
    }

    /// Whether this directed edge represents an execution ordering precedence constraint.
    #[must_use]
    pub fn is_ordering(&self) -> bool {
        self.kind.is_ordering()
    }

    /// Whether this directed edge represents an activation requirement dependency.
    #[must_use]
    pub fn is_requirement(&self) -> bool {
        self.kind.is_requirement()
    }

    /// Whether this directed edge expresses a strict requirement or ordering constraint.
    #[must_use]
    pub fn is_strict(&self) -> bool {
        self.kind.is_strict()
    }

    /// Whether this directed edge is a self-loop (`source == target`).
    #[must_use]
    pub fn is_self_loop(&self) -> bool {
        self.source == self.target
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

    /// Whether an exact edge with the given kind and target exists in this graph.
    #[must_use]
    pub fn contains_edge(&self, kind: &ServiceRelationKind, target: &ServiceId) -> bool {
        self.edges
            .iter()
            .any(|e| &e.kind == kind && &e.target == target)
    }

    /// Check if this service contains both `Before` and `After` relationships to the same target,
    /// which constitutes an immediate contradiction / 2-node ordering cycle.
    #[must_use]
    pub fn has_immediate_ordering_cycle(&self, target: &ServiceId) -> bool {
        self.contains_edge(&ServiceRelationKind::Before, target)
            && self.contains_edge(&ServiceRelationKind::After, target)
    }

    /// Check if this service defines any ordering or requirement dependency on itself.
    #[must_use]
    pub fn has_self_cycle(&self, origin: &ServiceId) -> bool {
        self.edges
            .iter()
            .any(|e| &e.target == origin && (e.is_ordering() || e.is_requirement()))
    }

    /// Canonical directed ordering edges induced by this service's relationships.
    pub fn directed_ordering_edges<'a>(
        &'a self,
        origin: &'a ServiceId,
    ) -> impl Iterator<Item = DirectedServiceEdge> + 'a {
        self.edges
            .iter()
            .filter_map(move |edge| edge.to_directed_ordering_edge(origin))
    }

    /// Canonical directed requirement edges induced by this service's relationships.
    pub fn directed_requirement_edges<'a>(
        &'a self,
        origin: &'a ServiceId,
    ) -> impl Iterator<Item = DirectedServiceEdge> + 'a {
        self.edges
            .iter()
            .filter_map(move |edge| edge.to_directed_requirement_edge(origin))
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

    /// Check if this service contains both `Before` and `After` relationships to the same target.
    #[must_use]
    pub fn has_immediate_ordering_cycle(&self, target: &ServiceId) -> bool {
        self.relations.has_immediate_ordering_cycle(target)
    }

    /// Check if this service defines an ordering or requirement dependency on itself.
    #[must_use]
    pub fn has_self_cycle(&self, origin: &ServiceId) -> bool {
        self.relations.has_self_cycle(origin)
    }

    /// Canonical directed ordering edges induced by this service's relationships.
    pub fn directed_ordering_edges<'a>(
        &'a self,
        origin: &'a ServiceId,
    ) -> impl Iterator<Item = DirectedServiceEdge> + 'a {
        self.relations.directed_ordering_edges(origin)
    }

    /// Canonical directed requirement edges induced by this service's relationships.
    pub fn directed_requirement_edges<'a>(
        &'a self,
        origin: &'a ServiceId,
    ) -> impl Iterator<Item = DirectedServiceEdge> + 'a {
        self.relations.directed_requirement_edges(origin)
    }
}

/// A detected cycle in a service dependency or ordering graph.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ServiceCycle {
    /// Sequence of service IDs forming the closed loop.
    /// The first and last elements are identical, e.g. `[A, B, C, A]`.
    pub path: Vec<ServiceId>,
}

impl ServiceCycle {
    #[must_use]
    pub fn new(path: Vec<ServiceId>) -> Self {
        Self { path }
    }

    /// Number of edges (steps) in the cycle.
    #[must_use]
    pub fn len(&self) -> usize {
        self.path.len().saturating_sub(1)
    }

    /// Whether the cycle has no edges.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if a service participates in this cycle.
    #[must_use]
    pub fn contains(&self, id: &ServiceId) -> bool {
        self.path.iter().any(|s| s == id)
    }
}

impl fmt::Display for ServiceCycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let formatted = self
            .path
            .iter()
            .map(ServiceId::as_str)
            .collect::<Vec<_>>()
            .join(" -> ");
        write!(f, "{formatted}")
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
