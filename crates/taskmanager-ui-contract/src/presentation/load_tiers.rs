//! The load-tier axis: the utilization band edges a per-core paint reads, and
//! the clamp-and-bucket fold every tier paint shares.
//!
//! Iced's per-core matrix and the TUI grid each carried a private copy of the
//! same `60`/`85` band with inclusive edges — one fact at two addresses. Bevy
//! paints its per-core sidebar bars with a wider `80`/`95` band whose edges are
//! exclusive, a difference that until now lived only as two loose constants.
//! GPUI tints per-core cells by CPU type and never by a utilization band, so it
//! declares the axis absent with the mechanism that replaces it instead of
//! silently spending someone else's number.
//!
//! Declared here as data, exactly like [`crate::presentation::thresholds`]
//! declares layout bands: the band edges, and which side an edge value falls
//! on. What is NOT owned here is which ink each tier paints with and how an
//! unobserved or non-finite reading looks. Those are visible-paint facts per
//! shape, and converging them needs capture evidence (TODO-101 discipline),
//! not a silent constant change.

use crate::keybindings::FrontendShape;
use taskforest_core::core::percent::MAX_PERCENT;

/// How a reading that sits exactly on a band edge is counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeMembership {
    /// The edge value already belongs to the higher band: `pct >= edge`.
    AtEdgeIsHigher,
    /// A reading must exceed the edge to reach the higher band: `pct > edge`.
    AboveEdgeOnly,
}

/// The two band edges, in percent, one shape's load-tier paint reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoadTierEdges {
    /// The edge at which a reading stops being calm.
    pub warn: f32,
    /// The edge at which a reading becomes an alarm.
    pub danger: f32,
    /// Which side of an edge the edge value itself falls on.
    pub membership: EdgeMembership,
}

impl LoadTierEdges {
    /// The reference band: calm below `60`, warning from `60`, alarm from `85`,
    /// each edge value belonging to the higher band.
    pub const SHARED: Self = Self {
        warn: 60.0,
        danger: 85.0,
        membership: EdgeMembership::AtEdgeIsHigher,
    };
}

/// One shape's answer for the load-tier axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoadTierDecision {
    /// The shape paints [`LoadTierEdges::SHARED`]. No reason needed: this is
    /// the shared fact, owned here.
    Shared,
    /// The shape paints its own band, with the architecture driver.
    Own {
        edges: LoadTierEdges,
        reason: &'static str,
    },
    /// The shape has no load-tier paint, with the mechanism that replaces it.
    Absent { reason: &'static str },
}

/// One declared load-tier answer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoadTierDeclaration {
    pub frontend: FrontendShape,
    pub decision: LoadTierDecision,
}

/// Every shape's declaration. This table IS the authority: a frontend reads
/// its band here and never restates a number.
pub const DECLARATIONS: &[LoadTierDeclaration] = &[
    LoadTierDeclaration {
        frontend: FrontendShape::Gpui,
        decision: LoadTierDecision::Absent {
            reason: "the GPUI per-core matrix tints each cell by CPU type \
                     (performance, efficient and low-power ink) and never by a \
                     utilization band",
        },
    },
    LoadTierDeclaration {
        frontend: FrontendShape::Iced,
        decision: LoadTierDecision::Shared,
    },
    LoadTierDeclaration {
        frontend: FrontendShape::Tui,
        decision: LoadTierDecision::Shared,
    },
    LoadTierDeclaration {
        frontend: FrontendShape::Bevy,
        decision: LoadTierDecision::Own {
            edges: LoadTierEdges {
                warn: 80.0,
                danger: 95.0,
                membership: EdgeMembership::AboveEdgeOnly,
            },
            reason: "one two-metric bar per core saturates visually far earlier than the \
                     dense cell matrix, so its band starts higher and an edge value keeps \
                     the lower band until a capture proves the difference away",
        },
    },
];

/// The declaration for one shape.
#[must_use]
pub fn load_tier_declaration(frontend: FrontendShape) -> Option<LoadTierDecision> {
    DECLARATIONS
        .iter()
        .find(|candidate| candidate.frontend == frontend)
        .map(|candidate| candidate.decision)
}

/// The band one shape paints, or `None` when the shape paints no load tier.
/// An absent shape yields `None` rather than the shared band, so a surface
/// that never tiers cannot accidentally borrow the rule it declared away.
#[must_use]
pub fn load_tier_edges(frontend: FrontendShape) -> Option<LoadTierEdges> {
    match load_tier_declaration(frontend)? {
        LoadTierDecision::Shared => Some(LoadTierEdges::SHARED),
        LoadTierDecision::Own { edges, .. } => Some(edges),
        LoadTierDecision::Absent { .. } => None,
    }
}

/// The load tier one reading falls into within one resolved band.
///
/// The clamp onto the percent scale is part of the fold, so no shape re-spells
/// it: a reading outside `[0, MAX_PERCENT]` buckets as if it sat on the nearer
/// edge. A non-finite reading has no tier — that is an absence, and the shape
/// keeps its own missing-value ink for it.
#[must_use]
pub fn tier_within(edges: LoadTierEdges, percent: f32) -> Option<LoadTier> {
    if !percent.is_finite() {
        return None;
    }
    let value = percent.clamp(0.0, MAX_PERCENT);
    let crossed = |edge: f32| match edges.membership {
        EdgeMembership::AtEdgeIsHigher => value >= edge,
        EdgeMembership::AboveEdgeOnly => value > edge,
    };
    Some(if crossed(edges.danger) {
        LoadTier::Alarm
    } else if crossed(edges.warn) {
        LoadTier::Warning
    } else {
        LoadTier::Calm
    })
}

/// The load tier one reading falls into under the shape's registered band.
/// A shape that declares the axis absent resolves no tier at all.
#[must_use]
pub fn load_tier(frontend: FrontendShape, percent: f32) -> Option<LoadTier> {
    load_tier_edges(frontend).and_then(|edges| tier_within(edges, percent))
}

/// The three bands a load-tier paint chooses between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadTier {
    /// Below the warning edge.
    Calm,
    /// Between the warning and alarm edges.
    Warning,
    /// At or beyond the alarm edge, as the shape's membership rule decides.
    Alarm,
}

#[cfg(test)]
#[path = "../../tests/headless/ui_presentation_load_tiers.rs"]
mod tests;
