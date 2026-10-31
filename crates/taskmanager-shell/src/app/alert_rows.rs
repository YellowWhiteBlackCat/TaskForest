//! Renderer-neutral alert row identity, geometry anchors, and family revisions.
//!
//! The application `AlertCenter` is the only list owner. This module turns its
//! rule, active-alert, and event-history rows into family-qualified identities
//! and rejects stale indexed geometry after a projection revision changes.

use std::collections::{HashMap, HashSet};

use taskforest_core::core::alerts::{AlertEventId, AlertInstanceId, AlertRuleId};
use taskmanager_application::{AlertCenter, AlertProjectionChanges};

use super::SystemProjectionStore;

/// Alert projection family whose owner rows have independent revisions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AlertRowFamily {
    #[default]
    Rules,
    Active,
    Events,
}

/// Stable identity of a row from one alert projection family.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AlertRowId {
    Rule(AlertRuleId),
    Active(AlertInstanceId),
    Event(AlertEventId),
}

impl AlertRowId {
    #[must_use]
    pub const fn family(&self) -> AlertRowFamily {
        match self {
            Self::Rule(_) => AlertRowFamily::Rules,
            Self::Active(_) => AlertRowFamily::Active,
            Self::Event(_) => AlertRowFamily::Events,
        }
    }
}

/// Monotonic owner-data revision for one alert row family.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AlertRowDataRevision(u64);

impl AlertRowDataRevision {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// Monotonic ordering revision, advanced when stable row order changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AlertRowOrderRevision(u64);

impl AlertRowOrderRevision {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// Current data and order revisions for one alert row family.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AlertRowProjectionGeneration {
    family: AlertRowFamily,
    data: AlertRowDataRevision,
    order: AlertRowOrderRevision,
}

impl AlertRowProjectionGeneration {
    #[must_use]
    pub const fn family(self) -> AlertRowFamily {
        self.family
    }

    #[must_use]
    pub const fn data_revision(self) -> AlertRowDataRevision {
        self.data
    }

    #[must_use]
    pub const fn order_revision(self) -> AlertRowOrderRevision {
        self.order
    }
}

/// Delayed row input tied to the family projection that produced its geometry.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AlertRowAnchor {
    id: AlertRowId,
    generation: AlertRowProjectionGeneration,
    index: usize,
}

impl AlertRowAnchor {
    #[must_use]
    pub const fn id(&self) -> &AlertRowId {
        &self.id
    }

    #[must_use]
    pub const fn generation(&self) -> AlertRowProjectionGeneration {
        self.generation
    }

    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }
}

#[derive(Clone, Debug, Default)]
struct AlertRowFamilyRevisionState {
    data: AlertRowDataRevision,
    order: AlertRowOrderRevision,
    ordered_identities: Vec<AlertRowId>,
}

#[derive(Clone, Debug)]
pub(crate) struct AlertRowRevisionState {
    families: HashMap<AlertRowFamily, AlertRowFamilyRevisionState>,
}

impl Default for AlertRowRevisionState {
    fn default() -> Self {
        let initial_rules = unique_identities(
            AlertCenter::default()
                .managed_rules()
                .iter()
                .map(|managed| AlertRowId::Rule(managed.rule.id.clone()))
                .collect(),
        );
        Self {
            families: HashMap::from([(
                AlertRowFamily::Rules,
                AlertRowFamilyRevisionState {
                    ordered_identities: initial_rules,
                    ..AlertRowFamilyRevisionState::default()
                },
            )]),
        }
    }
}

impl AlertRowRevisionState {
    fn generation(&self, family: AlertRowFamily) -> AlertRowProjectionGeneration {
        let state = self.families.get(&family).cloned().unwrap_or_default();
        AlertRowProjectionGeneration {
            family,
            data: state.data,
            order: state.order,
        }
    }
}

impl SystemProjectionStore {
    /// Stable identities in the application owner order for one family.
    #[must_use]
    pub fn alert_row_ids(&self, family: AlertRowFamily) -> Vec<AlertRowId> {
        let candidates = match family {
            AlertRowFamily::Rules => self
                .alert_center
                .managed_rules()
                .iter()
                .map(|managed| AlertRowId::Rule(managed.rule.id.clone()))
                .collect(),
            AlertRowFamily::Active => self
                .alert_center
                .active_alerts()
                .iter()
                .map(|alert| AlertRowId::Active(alert.instance_id.clone()))
                .collect(),
            AlertRowFamily::Events => self
                .alert_center
                .event_history()
                .iter()
                .map(|event| AlertRowId::Event(event.id))
                .collect(),
        };
        unique_identities(candidates)
    }

    /// Identity at an index in the current alert projection.
    #[must_use]
    pub fn alert_row_identity_at(
        &self,
        family: AlertRowFamily,
        index: usize,
    ) -> Option<AlertRowId> {
        self.alert_row_ids(family).get(index).cloned()
    }

    /// Current identity plus data/order revisions for an indexed alert row.
    #[must_use]
    pub fn alert_row_anchor_at(
        &self,
        family: AlertRowFamily,
        index: usize,
    ) -> Option<AlertRowAnchor> {
        Some(AlertRowAnchor {
            id: self.alert_row_identity_at(family, index)?,
            generation: self.alert_row_projection_generation(family),
            index,
        })
    }

    /// Resolve a stable row identity after a reorder.
    #[must_use]
    pub fn reconcile_alert_row_identity(&self, identity: &AlertRowId) -> Option<usize> {
        self.alert_row_ids(identity.family())
            .iter()
            .position(|current| current == identity)
    }

    /// Resolve an anchor only while its identity and both revisions remain current.
    #[must_use]
    pub fn resolve_alert_row_anchor(&self, anchor: &AlertRowAnchor) -> Option<usize> {
        let family = anchor.generation.family();
        (anchor.generation == self.alert_row_projection_generation(family)
            && self.alert_row_identity_at(family, anchor.index) == Some(anchor.id.clone()))
        .then_some(anchor.index)
    }

    /// Current source-data and row-order revisions for one alert family.
    #[must_use]
    pub fn alert_row_projection_generation(
        &self,
        family: AlertRowFamily,
    ) -> AlertRowProjectionGeneration {
        self.alert_row_revisions.generation(family)
    }

    pub(crate) fn record_alert_projection_changes(&mut self, changes: AlertProjectionChanges) {
        for (changed, family) in [
            (changes.rules, AlertRowFamily::Rules),
            (changes.active, AlertRowFamily::Active),
            (changes.events, AlertRowFamily::Events),
        ] {
            if changed {
                self.note_alert_row_family_changed(family);
            }
        }
    }

    fn note_alert_row_family_changed(&mut self, family: AlertRowFamily) {
        let identities = self.alert_row_ids(family);
        let state = self.alert_row_revisions.families.entry(family).or_default();
        state.data = state.data.next();
        if state.ordered_identities != identities {
            state.order = state.order.next();
            state.ordered_identities = identities;
        }
    }
}

fn unique_identities(candidates: Vec<AlertRowId>) -> Vec<AlertRowId> {
    let mut counts = HashMap::new();
    for identity in &candidates {
        *counts.entry(identity.clone()).or_insert(0_usize) += 1;
    }
    let mut emitted = HashSet::new();
    candidates
        .into_iter()
        .filter(|identity| counts.get(identity) == Some(&1) && emitted.insert(identity.clone()))
        .collect()
}

#[cfg(test)]
#[path = "../../tests/headless/shell_app_alert_rows.rs"]
mod tests;
