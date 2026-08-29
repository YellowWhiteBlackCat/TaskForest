//! Inventory source-status projection for independent page recovery.

use super::SystemProjectionStore;
use taskmanager_application::source_status_from_operation_failure;
use taskmanager_core::core::services::ServiceItem;
use taskmanager_core::core::session::SessionItem;
use taskmanager_core::core::startup::StartupEntry;
use taskmanager_platform_contract::{CapabilityId, OperationFailure, PartialSourceSnapshot};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct InventorySourceChanges {
    pub hardware: bool,
    pub services: bool,
    pub startup: bool,
    pub sessions: bool,
}

impl InventorySourceChanges {
    pub const fn any(self) -> bool {
        self.hardware || self.services || self.startup || self.sessions
    }
}

impl SystemProjectionStore {
    pub(super) fn apply_service_snapshot(
        &mut self,
        snapshot: PartialSourceSnapshot<ServiceItem>,
    ) -> bool {
        self.services = Some(snapshot.items);
        self.services_source = Some(snapshot.sources);
        true
    }

    pub(super) fn apply_startup_snapshot(
        &mut self,
        snapshot: PartialSourceSnapshot<StartupEntry>,
    ) -> bool {
        self.startup_entries = Some(snapshot.items);
        self.startup_source = Some(snapshot.sources);
        true
    }

    pub(super) fn apply_session_snapshot(
        &mut self,
        snapshot: PartialSourceSnapshot<SessionItem>,
    ) -> bool {
        self.sessions = Some(snapshot.items);
        self.sessions_source = Some(snapshot.sources);
        true
    }

    /// Seed typed source failures before snapshots are folded. A successful
    /// snapshot later in the same batch remains authoritative.
    pub(super) fn apply_inventory_failures(
        &mut self,
        failures: &[OperationFailure],
    ) -> InventorySourceChanges {
        let mut changes = InventorySourceChanges::default();
        for failure in failures {
            if failure.capability == CapabilityId::HARDWARE_INVENTORY {
                changes.hardware = true;
            } else if failure.capability == CapabilityId::SERVICES {
                changes.services = true;
            } else if failure.capability == CapabilityId::STARTUP {
                changes.startup = true;
            } else if failure.capability == CapabilityId::SESSIONS {
                changes.sessions = true;
            }
            self.apply_inventory_failure(failure);
        }
        changes
    }

    /// Record an inventory failure only on the domain that owns the capability.
    /// Existing rows stay available so the frontend can distinguish partial
    /// data from a genuine empty result.
    pub(super) fn apply_inventory_failure(&mut self, failure: &OperationFailure) {
        let (source, item_count) = if failure.capability == CapabilityId::HARDWARE_INVENTORY {
            (
                &mut self.hardware_source,
                usize::from(self.hardware.is_some()),
            )
        } else if failure.capability == CapabilityId::SERVICES {
            (
                &mut self.services_source,
                self.services.as_ref().map_or(0, Vec::len),
            )
        } else if failure.capability == CapabilityId::STARTUP {
            (
                &mut self.startup_source,
                self.startup_entries.as_ref().map_or(0, Vec::len),
            )
        } else if failure.capability == CapabilityId::SESSIONS {
            (
                &mut self.sessions_source,
                self.sessions.as_ref().map_or(0, Vec::len),
            )
        } else {
            return;
        };
        *source = Some(vec![source_status_from_operation_failure(
            failure, item_count,
        )]);
    }
}
