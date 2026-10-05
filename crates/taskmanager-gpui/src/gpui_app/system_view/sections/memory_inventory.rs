//! Pure SMBIOS memory-inventory projection for the System page's memory
//! subsection (the `telemetry.memory.smbios` request lane).
//!
//! Unlike the periodic memory facts in [`super::memory_section`] (fed by the
//! unprivileged udev + world-readable DMI merge), these rows come from the
//! application-owned request session backed by the privileged SMBIOS helper
//! (ADR-023, permission-model Boundary 2). The projection is render-neutral
//! and unit-tested: every non-ready variant is a typed placeholder, never a
//! fabricated slot or module row.

use taskmanager_application::SmbiosMemoryRequestFailure;
use taskmanager_application::SmbiosMemoryState;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_platform_contract::CapabilityStatus;
use taskmanager_shell::presentation::smbios_memory_inventory_rows;

/// Render-entry inputs for the subsection: the shared session state plus the
/// runtime capability catalog entry for the lane.
pub struct MemoryInventoryInputs<'a> {
    pub state: &'a SmbiosMemoryState,
    pub capability: Option<CapabilityStatus>,
}

/// Pure projection of the memory-inventory lane for the System page.
#[derive(Debug, PartialEq)]
pub(crate) enum MemoryInventoryModel {
    /// No live session and no registered lane: the subsection renders
    /// nothing at all.
    Hidden,
    /// Real inventory rows: the slots used/total row followed by one row per
    /// populated module. A populated slot stays visible even when its
    /// per-module facts are all `None` (an honest dash, never a dropped row).
    Inventory(Vec<(String, String)>),
    /// A request is in flight and no accepted payload exists yet.
    Reading,
    /// The lane is escalation-backed: the central Settings permission center
    /// renders the typed hint plus the authorize affordance. No slot or module
    /// row may render in this state.
    AuthorizationRequired,
    /// A typed failure; the value is the localized message key.
    Unavailable(&'static str),
}

#[must_use]
pub(crate) fn memory_inventory_model(
    inputs: &MemoryInventoryInputs<'_>,
    units: UnitPreferences,
) -> MemoryInventoryModel {
    match inputs.state {
        SmbiosMemoryState::Ready(ready) => {
            MemoryInventoryModel::Inventory(smbios_memory_inventory_rows(&ready.snapshot, units))
        }
        SmbiosMemoryState::Loading {
            last_good: Some(ready),
            ..
        } => MemoryInventoryModel::Inventory(smbios_memory_inventory_rows(&ready.snapshot, units)),
        SmbiosMemoryState::Loading {
            last_good: None, ..
        } => MemoryInventoryModel::Reading,
        SmbiosMemoryState::Failed(failed) => model_from_failure(failure_kind(&failed.failure)),
        SmbiosMemoryState::Closed => match inputs.capability {
            // The runtime catalog proves an escalation-backed lane exists:
            // offer the one explicit authorization entry. `RequiresEscalation`
            // is the escalatable state (the per-feature OS-native prompt can
            // grant it); `PermissionRequired` is a permission gate with no
            // escalation offer. Both still require one explicit decision.
            Some(
                CapabilityStatus::Available
                | CapabilityStatus::PermissionRequired
                | CapabilityStatus::RequiresEscalation,
            ) => MemoryInventoryModel::AuthorizationRequired,
            Some(CapabilityStatus::MissingDependency) => {
                MemoryInventoryModel::Unavailable("system.memory_inventory_helper")
            }
            Some(CapabilityStatus::Degraded(kind)) => model_from_failure(kind),
            Some(CapabilityStatus::Unsupported)
            | Some(CapabilityStatus::TemporarilyUnavailable)
            | Some(CapabilityStatus::Stale)
            | None => MemoryInventoryModel::Hidden,
        },
    }
}

/// Both failure spellings carry one `FailureKind`; the provider's detail
/// string is host-specific and never parsed here.
fn failure_kind(failure: &SmbiosMemoryRequestFailure) -> FailureKind {
    match failure {
        SmbiosMemoryRequestFailure::Submission(kind) => *kind,
        SmbiosMemoryRequestFailure::Provider(failed) => failed.kind,
    }
}

fn model_from_failure(kind: FailureKind) -> MemoryInventoryModel {
    match kind {
        FailureKind::RequiresEscalation => MemoryInventoryModel::AuthorizationRequired,
        FailureKind::PermissionDenied => {
            MemoryInventoryModel::Unavailable("system.memory_inventory_denied")
        }
        FailureKind::MissingDependency => {
            MemoryInventoryModel::Unavailable("system.memory_inventory_helper")
        }
        FailureKind::Unsupported => {
            MemoryInventoryModel::Unavailable("system.memory_inventory_unsupported")
        }
        FailureKind::TimedOut
        | FailureKind::TemporarilyUnavailable
        | FailureKind::IdentityChanged
        | FailureKind::Rejected
        | FailureKind::ProviderFault => {
            MemoryInventoryModel::Unavailable("system.memory_inventory_unavailable")
        }
    }
}
