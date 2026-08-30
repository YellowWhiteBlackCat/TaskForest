//! Pure SMBIOS memory-inventory projection for the System page's memory
//! subsection (the `telemetry.memory.smbios` request lane).
//!
//! Unlike the periodic memory facts in [`super::memory_section`] (fed by the
//! unprivileged udev + world-readable DMI merge), these rows come from the
//! application-owned request session backed by the privileged SMBIOS helper
//! (ADR-023, permission-model Boundary 2). The projection is render-neutral
//! and unit-tested: every non-ready variant is a typed placeholder, never a
//! fabricated slot or module row.

use taskmanager_application::SmbiosMemoryState;
use taskmanager_application::i18n;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{SmbiosMemorySnapshot, SmbiosModuleRow};
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_platform_contract::CapabilityStatus;

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
    /// The lane is escalation-backed: render the typed hint plus the
    /// authorize affordance. No slot or module row may render in this state.
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
            MemoryInventoryModel::Inventory(inventory_rows(&ready.snapshot, units))
        }
        SmbiosMemoryState::Loading {
            last_good: Some(ready),
            ..
        } => MemoryInventoryModel::Inventory(inventory_rows(&ready.snapshot, units)),
        SmbiosMemoryState::Loading {
            last_good: None, ..
        } => MemoryInventoryModel::Reading,
        SmbiosMemoryState::Failed(failed) => model_from_failure(failure_kind(&failed.failure)),
        SmbiosMemoryState::Closed => match inputs.capability {
            // The runtime catalog proves an escalation-backed lane exists:
            // offer the one explicit authorization entry.
            Some(CapabilityStatus::Available | CapabilityStatus::PermissionRequired) => {
                MemoryInventoryModel::AuthorizationRequired
            }
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

/// Slots used/total followed by one row per populated module, in the order
/// the provider sorted them.
fn inventory_rows(
    snapshot: &SmbiosMemorySnapshot,
    units: UnitPreferences,
) -> Vec<(String, String)> {
    let mut rows = vec![(
        i18n::t("system.memory_slots").to_string(),
        format!(
            "{} / {} {}",
            snapshot.slots_used,
            snapshot.slots_total,
            i18n::t("common.used")
        ),
    )];
    rows.extend(
        snapshot
            .modules
            .iter()
            .map(|module| module_row(module, units)),
    );
    rows
}

/// One populated module: locator (or slot index) on the left, then the facts
/// the SMBIOS record actually carried — part number, capacity, configured
/// speed — joined with the page's ` · ` separator. Absent facts drop out of
/// the value; an entirely empty record keeps its row with the shared dash.
fn module_row(module: &SmbiosModuleRow, units: UnitPreferences) -> (String, String) {
    let label = module
        .locator
        .as_deref()
        .map(str::trim)
        .filter(|locator| !locator.is_empty())
        .map_or_else(
            || format!("{} {}", i18n::t("system.memory_module"), module.slot),
            str::to_string,
        );
    let mut facts = Vec::new();
    if let Some(part) = module
        .part_number
        .as_deref()
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        facts.push(part.to_string());
    }
    // Module capacity is MB (SMBIOS type 17 semantics); `format_quantity`
    // expects bytes, so scale first — the same fold as the installed row.
    if let Some(bytes) = module
        .size_mb
        .and_then(|mb| u64::from(mb).checked_mul(1024 * 1024))
    {
        facts.push(units.format_quantity(bytes, QuantityFamily::Memory, false));
    }
    if let Some(speed) = module.configured_speed_mts.filter(|speed| *speed > 0) {
        facts.push(format!("{speed} MT/s"));
    }
    if facts.is_empty() {
        facts.push(crate::gpui_app::formatting::missing_value());
    }
    (label, facts.join(" · "))
}

/// Both failure spellings carry one `FailureKind`; the provider's detail
/// string is host-specific and never parsed here.
fn failure_kind(failure: &taskmanager_application::SmbiosMemoryRequestFailure) -> FailureKind {
    match failure {
        taskmanager_application::SmbiosMemoryRequestFailure::Submission(kind) => *kind,
        taskmanager_application::SmbiosMemoryRequestFailure::Provider(failed) => failed.kind,
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
