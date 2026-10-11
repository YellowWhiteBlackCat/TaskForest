//! Renderer-neutral identity and stale-geometry rules for flat inventories.
//!
//! Selection follows provider-owned identities across sorting and refreshes.
//! A row anchor additionally carries the data revision and active sort that
//! produced its geometry, so delayed events from an older projection fail
//! closed. Display names and row positions never become identity.

use taskmanager_core::core::services::ServiceItem;
use taskmanager_core::core::session::SessionItem;
use taskmanager_core::core::startup::{StartupEntry, StartupEntryId};
use taskmanager_core::core::target::{ServiceId, SessionId};

use super::ShellApp;
use super::sorting::{InfoSortCol, InfoTable, SortDir};

/// One of the flat inventories with a shell-owned sort and data revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InventoryRowDomain {
    Services,
    Startup,
    Sessions,
}

impl InventoryRowDomain {
    /// The shell table whose canonical ordering belongs to this domain.
    #[must_use]
    pub const fn info_table(self) -> InfoTable {
        match self {
            Self::Services => InfoTable::Services,
            Self::Startup => InfoTable::Startup,
            Self::Sessions => InfoTable::Users,
        }
    }
}

/// Stable selection identity copied from the owning inventory row.
///
/// Empty legacy identities remain displayable, but do not produce a stable
/// row identity. No variant is derived from a label, locator, or row index.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InventoryRowId {
    Service(ServiceId),
    Startup(StartupEntryId),
    Session(SessionId),
}

impl InventoryRowId {
    /// Recover a service row identity only from its provider-issued target.
    #[must_use]
    pub fn from_service(row: &ServiceItem) -> Option<Self> {
        (!row.id.as_str().is_empty()).then(|| Self::Service(row.id.clone()))
    }

    /// Recover a startup row identity only from its provider-issued ID.
    #[must_use]
    pub fn from_startup(row: &StartupEntry) -> Option<Self> {
        (!row.id.is_empty()).then(|| Self::Startup(row.id.clone()))
    }

    /// Recover a session row identity only from its provider-issued target.
    #[must_use]
    pub fn from_session(row: &SessionItem) -> Option<Self> {
        (!row.id.as_str().is_empty()).then(|| Self::Session(row.id.clone()))
    }

    /// The owning inventory domain.
    #[must_use]
    pub const fn domain(&self) -> InventoryRowDomain {
        match self {
            Self::Service(_) => InventoryRowDomain::Services,
            Self::Startup(_) => InventoryRowDomain::Startup,
            Self::Session(_) => InventoryRowDomain::Sessions,
        }
    }

    /// Whether this identity has the minimum owner-issued value needed for
    /// reconciliation. Public enum constructors cannot make an empty ID
    /// accidentally usable.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Service(id) => !id.as_str().is_empty(),
            Self::Startup(id) => !id.is_empty(),
            Self::Session(id) => !id.as_str().is_empty(),
        }
    }
}

/// Revision of one flat inventory projection, including its active order.
///
/// Stable selection identities are reconciled across revisions. This value
/// is only the geometry token: a data refresh or sort change makes anchors
/// from the prior projection stale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InventoryProjectionGeneration {
    domain: InventoryRowDomain,
    revision: u64,
    sort: Option<(InfoSortCol, SortDir)>,
}

impl InventoryProjectionGeneration {
    /// The inventory domain that produced this projection.
    #[must_use]
    pub const fn domain(self) -> InventoryRowDomain {
        self.domain
    }

    /// The accepted data revision for this inventory.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
}

/// Stable inventory row identity paired with the projection that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryRowAnchor {
    id: InventoryRowId,
    generation: InventoryProjectionGeneration,
}

impl InventoryRowAnchor {
    /// Bind a valid owner identity to a generation from the same domain.
    #[must_use]
    pub fn new(id: InventoryRowId, generation: InventoryProjectionGeneration) -> Option<Self> {
        (id.is_valid() && id.domain() == generation.domain).then_some(Self { id, generation })
    }

    /// The stable owner identity, suitable for selection reconciliation.
    #[must_use]
    pub const fn id(&self) -> &InventoryRowId {
        &self.id
    }

    /// The data revision and ordering that produced this row geometry.
    #[must_use]
    pub const fn generation(&self) -> InventoryProjectionGeneration {
        self.generation
    }

    /// Whether this event belongs to the currently committed projection.
    #[must_use]
    pub fn belongs_to(&self, generation: InventoryProjectionGeneration) -> bool {
        self.generation == generation
    }
}

impl ShellApp {
    /// Current geometry generation for one sorted inventory projection.
    #[must_use]
    pub const fn inventory_projection_generation(
        &self,
        domain: InventoryRowDomain,
    ) -> InventoryProjectionGeneration {
        let (revision, sort) = match domain {
            InventoryRowDomain::Services => (self.data.services_revision, self.services_sort),
            InventoryRowDomain::Startup => (self.data.startup_revision, self.startup_sort),
            InventoryRowDomain::Sessions => (self.data.sessions_revision, self.sessions_sort),
        };
        InventoryProjectionGeneration {
            domain,
            revision,
            sort,
        }
    }

    /// Stable identity at one row in the shell's canonical sorted order.
    #[must_use]
    pub fn inventory_row_identity_at(
        &self,
        domain: InventoryRowDomain,
        index: usize,
    ) -> Option<InventoryRowId> {
        match domain {
            InventoryRowDomain::Services => self
                .sorted_service_at(index)
                .and_then(InventoryRowId::from_service),
            InventoryRowDomain::Startup => self
                .sorted_startup_entry_at(index)
                .and_then(InventoryRowId::from_startup),
            InventoryRowDomain::Sessions => self
                .sorted_session_at(index)
                .and_then(InventoryRowId::from_session),
        }
    }

    /// Current row anchor for delayed renderer events.
    #[must_use]
    pub fn inventory_row_anchor_at(
        &self,
        domain: InventoryRowDomain,
        index: usize,
    ) -> Option<InventoryRowAnchor> {
        let id = self.inventory_row_identity_at(domain, index)?;
        InventoryRowAnchor::new(id, self.inventory_projection_generation(domain))
    }

    /// Bind a row identity already obtained from the current renderer-neutral
    /// row projection to that projection's geometry generation.
    #[must_use]
    pub fn anchor_inventory_row(&self, id: &InventoryRowId) -> Option<InventoryRowAnchor> {
        InventoryRowAnchor::new(
            id.clone(),
            self.inventory_projection_generation(id.domain()),
        )
    }

    /// Reconcile stable selection after data or ordering changes.
    ///
    /// The returned position is the exact matching row in the current
    /// canonical order. Missing, invalid, cross-domain, or duplicated owner
    /// identities return `None`; there is no positional-neighbor fallback.
    #[must_use]
    pub fn reconcile_inventory_row_identity(&self, id: &InventoryRowId) -> Option<usize> {
        if !id.is_valid() {
            return None;
        }
        let domain = id.domain();
        let row_ids = match domain {
            InventoryRowDomain::Services => self
                .sorted_services()
                .into_iter()
                .map(InventoryRowId::from_service)
                .collect::<Vec<_>>(),
            InventoryRowDomain::Startup => self
                .sorted_startup_entries()
                .into_iter()
                .map(InventoryRowId::from_startup)
                .collect::<Vec<_>>(),
            InventoryRowDomain::Sessions => self
                .sorted_sessions()
                .into_iter()
                .map(InventoryRowId::from_session)
                .collect::<Vec<_>>(),
        };
        let mut matches = row_ids
            .into_iter()
            .enumerate()
            .filter_map(|(index, candidate)| (candidate.as_ref() == Some(id)).then_some(index));
        let index = matches.next()?;
        if matches.next().is_some() {
            None
        } else {
            Some(index)
        }
    }

    /// Resolve a pointer or delayed row event only against its exact current
    /// projection. Selection may instead use
    /// [`Self::reconcile_inventory_row_identity`] to survive reorder.
    #[must_use]
    pub fn resolve_inventory_row_anchor(&self, anchor: &InventoryRowAnchor) -> Option<usize> {
        if !anchor.belongs_to(self.inventory_projection_generation(anchor.id.domain())) {
            return None;
        }
        self.reconcile_inventory_row_identity(anchor.id())
    }
}

#[cfg(test)]
#[path = "../../tests/headless/shell_app_inventory_rows.rs"]
mod tests;
