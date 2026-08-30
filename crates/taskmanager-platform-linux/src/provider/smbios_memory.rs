//! Linux SMBIOS memory-inventory provider (capability
//! `telemetry.memory.smbios`) — the privileged-lane twin of the per-engine
//! GPU provider.
//!
//! Each [`SmbiosMemoryProvider::read_memory_smbios`] call performs exactly ONE
//! bounded `invoke_smbios_helper` invocation (the audited polkit/pkexec seam in
//! `taskmanager-escalation`, ADR-023 Boundary 2) and maps the typed outcome
//! into a system-scoped snapshot. The call blocks (the helper walks the DMI
//! entries plus the OS-native prompt on first use) and therefore only ever runs
//! on the dedicated smbios-memory lane thread — never a UI thread. Request
//! pacing belongs to the frontend; this provider holds no timer.
//!
//! # Honesty
//!
//! The live privileged read remains on-box-unverified: the helper JSON contract
//! parsing/classification is unit-tested in `taskmanager-escalation::polkit`,
//! but the live privileged read needs sudo and is an integrator on-box receipt
//! item. No fabricated rows exist on any path: failures stay typed failures.

use taskmanager_core::{DmiIdentityFacts, SmbiosMemorySnapshot, SmbiosModuleRow};
use taskmanager_escalation::polkit::{
    PolkitGate, SmbiosHelperErrorKind, SmbiosHelperOutcome, invoke_smbios_helper,
};
use taskmanager_escalation::{
    EscalationAvailability, EscalationDenialReason, EscalationFeature, PrivilegeGate,
};
use taskmanager_platform_contract::{CapabilityStatus, ProviderFailure};
use taskmanager_platform_provider::SmbiosMemoryProvider;

pub(super) struct NativeSmbiosMemoryProvider {
    probe: fn() -> EscalationAvailability,
    invoke: fn() -> SmbiosHelperOutcome,
}

impl NativeSmbiosMemoryProvider {
    pub(super) const fn new() -> Self {
        Self::with_crossing(probe_smbios_crossing, invoke_smbios_helper)
    }

    pub(super) fn initial_status(&self) -> CapabilityStatus {
        capability_status_from_availability((self.probe)())
    }

    const fn with_crossing(
        probe: fn() -> EscalationAvailability,
        invoke: fn() -> SmbiosHelperOutcome,
    ) -> Self {
        Self { probe, invoke }
    }
}

impl Default for NativeSmbiosMemoryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SmbiosMemoryProvider for NativeSmbiosMemoryProvider {
    fn read_memory_smbios(&mut self) -> Result<SmbiosMemorySnapshot, ProviderFailure> {
        match (self.probe)() {
            EscalationAvailability::Available
            | EscalationAvailability::RequiresEscalation(EscalationFeature::MemorySmbios) => {
                result_from_outcome((self.invoke)())
            }
            EscalationAvailability::RequiresEscalation(_) => Err(ProviderFailure::ProviderFault),
            EscalationAvailability::Denied { reason } => Err(provider_failure_from_denial(reason)),
        }
    }
}

fn probe_smbios_crossing() -> EscalationAvailability {
    PolkitGate::new().probe(EscalationFeature::MemorySmbios)
}

fn capability_status_from_availability(availability: EscalationAvailability) -> CapabilityStatus {
    match availability {
        EscalationAvailability::Available => CapabilityStatus::Available,
        EscalationAvailability::RequiresEscalation(EscalationFeature::MemorySmbios) => {
            CapabilityStatus::PermissionRequired
        }
        EscalationAvailability::RequiresEscalation(_) => CapabilityStatus::TemporarilyUnavailable,
        EscalationAvailability::Denied { reason } => match reason {
            EscalationDenialReason::Unsupported => CapabilityStatus::Unsupported,
            EscalationDenialReason::PermissionDenied => {
                CapabilityStatus::Degraded(taskmanager_core::FailureKind::PermissionDenied)
            }
            EscalationDenialReason::AuthorizationUnavailable => {
                CapabilityStatus::TemporarilyUnavailable
            }
            EscalationDenialReason::HelperUnavailable => CapabilityStatus::MissingDependency,
            EscalationDenialReason::HelperProtocolViolation => {
                CapabilityStatus::Degraded(taskmanager_core::FailureKind::ProviderFault)
            }
        },
    }
}

/// Map one typed helper outcome into a system-scoped snapshot — the single
/// honest crossing from the escalation crate's SMBIOS contract into the typed
/// lane. `Success` carries the slot inventory and populated-module rows;
/// `HelperError` (the helper ran, typed ERROR) stays a typed provider failure;
/// `Unavailable` keeps its typed denial reason.
fn result_from_outcome(
    outcome: SmbiosHelperOutcome,
) -> Result<SmbiosMemorySnapshot, ProviderFailure> {
    match outcome {
        SmbiosHelperOutcome::Success(success) => Ok(SmbiosMemorySnapshot::success(
            success.slots_total,
            success.slots_used,
            success
                .modules
                .iter()
                .map(|reading| SmbiosModuleRow {
                    slot: reading.slot,
                    size_mb: reading.size_mb,
                    speed_mts: reading.speed_mts,
                    configured_speed_mts: reading.configured_speed_mts,
                    manufacturer: reading.manufacturer.clone(),
                    serial_number: reading.serial_number.clone(),
                    part_number: reading.part_number.clone(),
                    form_factor: reading.form_factor.clone(),
                    memory_type: reading.memory_type.clone(),
                    locator: reading.locator.clone(),
                })
                .collect(),
            success.identity.as_ref().map(identity_row),
        )),
        SmbiosHelperOutcome::HelperError(error) => Err(match error.kind {
            SmbiosHelperErrorKind::PermissionDenied => ProviderFailure::PermissionDenied,
            SmbiosHelperErrorKind::NoDmi => ProviderFailure::Unsupported,
            SmbiosHelperErrorKind::OpenFailed | SmbiosHelperErrorKind::ReadFailed => {
                ProviderFailure::ProviderFault
            }
        }),
        SmbiosHelperOutcome::Unavailable { reason, .. } => {
            Err(provider_failure_from_denial(reason))
        }
    }
}

/// Map the escalation seam's parsed identity struct onto the core fact,
/// field-by-field (one fact, one authority: core owns the typed fact; the
/// escalation crate owns only the wire shape).
fn identity_row(identity: &taskmanager_escalation::polkit::DmiIdentityFacts) -> DmiIdentityFacts {
    let taskmanager_escalation::polkit::DmiIdentityFacts {
        bios_vendor,
        bios_version,
        bios_date,
        board_manufacturer,
        board_product,
        board_serial,
        board_asset_tag,
        system_manufacturer,
        system_product,
        system_serial,
        system_uuid,
        system_sku,
        system_family,
    } = identity;
    DmiIdentityFacts {
        bios_vendor: bios_vendor.clone(),
        bios_version: bios_version.clone(),
        bios_date: bios_date.clone(),
        board_manufacturer: board_manufacturer.clone(),
        board_product: board_product.clone(),
        board_serial: board_serial.clone(),
        board_asset_tag: board_asset_tag.clone(),
        system_manufacturer: system_manufacturer.clone(),
        system_product: system_product.clone(),
        system_serial: system_serial.clone(),
        system_uuid: system_uuid.clone(),
        system_sku: system_sku.clone(),
        system_family: system_family.clone(),
    }
}

const fn provider_failure_from_denial(reason: EscalationDenialReason) -> ProviderFailure {
    match reason {
        EscalationDenialReason::PermissionDenied => ProviderFailure::PermissionDenied,
        EscalationDenialReason::AuthorizationUnavailable => ProviderFailure::TemporarilyUnavailable,
        EscalationDenialReason::HelperUnavailable => ProviderFailure::MissingDependency,
        EscalationDenialReason::HelperProtocolViolation => ProviderFailure::ProviderFault,
        EscalationDenialReason::Unsupported => ProviderFailure::Unsupported,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/linux_provider_smbios_memory_tests.rs"]
mod tests;
