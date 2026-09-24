//! Linux CPU MSR-readout provider (capability `telemetry.cpu.msr`) — the
//! privileged-lane twin of the package-power provider.
//!
//! Each [`MsrReadoutProvider::read_msr_readouts`] call performs exactly ONE
//! bounded `invoke_msr_helper` invocation (the audited polkit/pkexec seam in
//! `taskmanager-escalation`, ADR-023/048 Boundary 2): the helper reads the
//! root-only `/dev/cpu/N/msr` registers once. The call blocks (plus the
//! OS-native prompt on first use) and therefore only ever runs on the
//! dedicated msr-readout lane thread — never a UI thread. Request pacing
//! belongs to the frontend; this provider holds no timer.
//!
//! # Honesty
//!
//! The live privileged read remains on-box-unverified: the helper JSON
//! contract parsing/classification is unit-tested in
//! `taskmanager-escalation::polkit`, but the live privileged read needs sudo
//! and is an integrator on-box receipt item. That receipt now also covers the
//! `IA32_THERM_STATUS` (0x19C) thermal-status / PROCHOT bits. No fabricated
//! register values exist on any path.

use taskmanager_core::FailureKind;
use taskmanager_core::{MsrPackageReadout, MsrReadoutSnapshot, MsrThermalStatusReadout};
use taskmanager_escalation::polkit::PolkitGate;
use taskmanager_escalation::polkit::{MsrHelperErrorKind, MsrHelperOutcome, invoke_msr_helper};
use taskmanager_escalation::{
    EscalationAvailability, EscalationDenialReason, EscalationFeature, PrivilegeGate,
};
use taskmanager_platform_contract::{CapabilityStatus, ProviderFailure};
use taskmanager_platform_provider::MsrReadoutProvider;

pub(super) struct NativeMsrReadoutProvider {
    probe: fn() -> EscalationAvailability,
    invoke: fn() -> MsrHelperOutcome,
}

impl NativeMsrReadoutProvider {
    pub(super) const fn new() -> Self {
        Self::with_crossing(probe_msr_crossing, invoke_msr_helper)
    }

    pub(super) fn initial_status(&self) -> CapabilityStatus {
        capability_status_from_availability((self.probe)())
    }

    const fn with_crossing(
        probe: fn() -> EscalationAvailability,
        invoke: fn() -> MsrHelperOutcome,
    ) -> Self {
        Self { probe, invoke }
    }
}

impl Default for NativeMsrReadoutProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MsrReadoutProvider for NativeMsrReadoutProvider {
    fn read_msr_readouts(&mut self) -> Result<MsrReadoutSnapshot, ProviderFailure> {
        match (self.probe)() {
            EscalationAvailability::Available
            | EscalationAvailability::RequiresEscalation(EscalationFeature::CpuMsr) => {
                result_from_outcome((self.invoke)())
            }
            EscalationAvailability::RequiresEscalation(_) => Err(ProviderFailure::ProviderFault),
            EscalationAvailability::Denied { reason } => Err(provider_failure_from_denial(reason)),
        }
    }
}

fn probe_msr_crossing() -> EscalationAvailability {
    PolkitGate::new().probe(EscalationFeature::CpuMsr)
}

fn capability_status_from_availability(availability: EscalationAvailability) -> CapabilityStatus {
    match availability {
        EscalationAvailability::Available => CapabilityStatus::Available,
        // Escalation is available for exactly this feature: the capability is
        // escalatable, a distinct state from a plain permission gate.
        EscalationAvailability::RequiresEscalation(EscalationFeature::CpuMsr) => {
            CapabilityStatus::RequiresEscalation
        }
        EscalationAvailability::RequiresEscalation(_) => CapabilityStatus::TemporarilyUnavailable,
        EscalationAvailability::Denied { reason } => match reason {
            EscalationDenialReason::Unsupported => CapabilityStatus::Unsupported,
            EscalationDenialReason::PermissionDenied => {
                CapabilityStatus::Degraded(FailureKind::PermissionDenied)
            }
            EscalationDenialReason::AuthorizationUnavailable => {
                CapabilityStatus::TemporarilyUnavailable
            }
            EscalationDenialReason::HelperUnavailable => CapabilityStatus::MissingDependency,
            EscalationDenialReason::HelperProtocolViolation => {
                CapabilityStatus::Degraded(FailureKind::ProviderFault)
            }
        },
    }
}

/// Map one typed helper outcome into a system-scoped snapshot — the single
/// honest crossing from the escalation crate's MSR contract into the typed
/// lane. `Success` copies every register field Option-by-Option (an
/// unimplemented register stays `None`) and attaches the `IA32_THERM_STATUS`
/// thermal-status rows to the same snapshot; `HelperError` (the helper ran,
/// typed ERROR) stays a typed provider failure; `Unavailable` keeps its typed
/// denial reason.
fn result_from_outcome(outcome: MsrHelperOutcome) -> Result<MsrReadoutSnapshot, ProviderFailure> {
    match outcome {
        MsrHelperOutcome::Success(success) => {
            let mut packages = Vec::with_capacity(success.packages.len());
            let mut thermal = Vec::with_capacity(success.packages.len());
            for reading in &success.packages {
                packages.push(MsrPackageReadout {
                    cpu: reading.cpu,
                    bclk_mhz: reading.bclk_mhz,
                    temperature_c: reading.temperature_c,
                    multiplier: reading.multiplier,
                    multiplier_min: reading.multiplier_min,
                    multiplier_max: reading.multiplier_max,
                    vcore_v: reading.vcore_v,
                });
                thermal.push(MsrThermalStatusReadout {
                    cpu: reading.cpu,
                    thermal_status: reading.thermal_status,
                    thermal_status_log: reading.thermal_status_log,
                    prochot_event: reading.prochot_event,
                    prochot_event_log: reading.prochot_event_log,
                });
            }
            Ok(MsrReadoutSnapshot::success(packages).with_thermal(thermal))
        }
        MsrHelperOutcome::HelperError(error) => Err(match error.kind {
            MsrHelperErrorKind::PermissionDenied => ProviderFailure::PermissionDenied,
            MsrHelperErrorKind::NoMsr => ProviderFailure::Unsupported,
            MsrHelperErrorKind::OpenFailed | MsrHelperErrorKind::ReadFailed => {
                ProviderFailure::ProviderFault
            }
        }),
        MsrHelperOutcome::Unavailable { reason, .. } => Err(provider_failure_from_denial(reason)),
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
#[path = "../../tests/headless/linux_provider_msr_readout_tests.rs"]
mod tests;
