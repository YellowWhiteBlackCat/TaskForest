//! Linux CPU package-power provider (capability
//! `telemetry.cpu.package_power`) — the privileged-lane twin of the per-engine
//! GPU provider.
//!
//! Each [`RaplPowerProvider::read_package_power`] call performs exactly ONE
//! bounded `invoke_rapl_helper` invocation (the audited polkit/pkexec seam in
//! `taskmanager-escalation`, ADR-023 Boundary 2): the helper samples the
//! root-only RAPL `energy_uj` counters over one fixed window and derives
//! per-package watts. The call blocks (the sample window plus the OS-native
//! prompt on first use) and therefore only ever runs on the dedicated
//! rapl-power lane thread — never a UI thread. Request pacing belongs to the
//! frontend; this provider holds no timer.
//!
//! # Honesty
//!
//! The live privileged read remains on-box-unverified: the helper JSON contract
//! parsing/classification is unit-tested in `taskmanager-escalation::polkit`,
//! but the live privileged read needs sudo and is an integrator on-box receipt
//! item. No fabricated watt figures exist on any path.

use taskmanager_core::{RaplPackageRow, RaplPowerSnapshot};
use taskmanager_escalation::polkit::{
    PolkitGate, RaplHelperErrorKind, RaplHelperOutcome, invoke_rapl_helper,
};
use taskmanager_escalation::{
    EscalationAvailability, EscalationDenialReason, EscalationFeature, PrivilegeGate,
};
use taskmanager_platform_contract::{CapabilityStatus, ProviderFailure};
use taskmanager_platform_provider::RaplPowerProvider;

pub(super) struct NativeRaplPowerProvider {
    probe: fn() -> EscalationAvailability,
    invoke: fn() -> RaplHelperOutcome,
}

impl NativeRaplPowerProvider {
    pub(super) const fn new() -> Self {
        Self::with_crossing(probe_rapl_crossing, invoke_rapl_helper)
    }

    pub(super) fn initial_status(&self) -> CapabilityStatus {
        capability_status_from_availability((self.probe)())
    }

    const fn with_crossing(
        probe: fn() -> EscalationAvailability,
        invoke: fn() -> RaplHelperOutcome,
    ) -> Self {
        Self { probe, invoke }
    }
}

impl Default for NativeRaplPowerProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl RaplPowerProvider for NativeRaplPowerProvider {
    fn read_package_power(&mut self) -> Result<RaplPowerSnapshot, ProviderFailure> {
        match (self.probe)() {
            EscalationAvailability::Available
            | EscalationAvailability::RequiresEscalation(EscalationFeature::PackagePowerRapl) => {
                result_from_outcome((self.invoke)())
            }
            EscalationAvailability::RequiresEscalation(_) => Err(ProviderFailure::ProviderFault),
            EscalationAvailability::Denied { reason } => Err(provider_failure_from_denial(reason)),
        }
    }
}

fn probe_rapl_crossing() -> EscalationAvailability {
    PolkitGate::new().probe(EscalationFeature::PackagePowerRapl)
}

fn capability_status_from_availability(availability: EscalationAvailability) -> CapabilityStatus {
    match availability {
        EscalationAvailability::Available => CapabilityStatus::Available,
        EscalationAvailability::RequiresEscalation(EscalationFeature::PackagePowerRapl) => {
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
/// honest crossing from the escalation crate's RAPL contract into the typed
/// lane. `Success` carries the sample window and per-package watt figures;
/// `HelperError` (the helper ran, typed ERROR) stays a typed provider failure;
/// `Unavailable` keeps its typed denial reason.
fn result_from_outcome(outcome: RaplHelperOutcome) -> Result<RaplPowerSnapshot, ProviderFailure> {
    match outcome {
        RaplHelperOutcome::Success(success) => Ok(RaplPowerSnapshot::success(
            success.sample_ms,
            success
                .packages
                .iter()
                .map(|reading| RaplPackageRow {
                    name: reading.name.clone(),
                    power_w: reading.power_w,
                    energy_delta_uj: reading.energy_delta_uj,
                })
                .collect(),
        )),
        RaplHelperOutcome::HelperError(error) => Err(match error.kind {
            RaplHelperErrorKind::PermissionDenied => ProviderFailure::PermissionDenied,
            RaplHelperErrorKind::NoRapl => ProviderFailure::Unsupported,
            RaplHelperErrorKind::OpenFailed | RaplHelperErrorKind::ReadFailed => {
                ProviderFailure::ProviderFault
            }
        }),
        RaplHelperOutcome::Unavailable { reason, .. } => Err(provider_failure_from_denial(reason)),
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
#[path = "../../tests/headless/linux_provider_rapl_power_tests.rs"]
mod tests;
