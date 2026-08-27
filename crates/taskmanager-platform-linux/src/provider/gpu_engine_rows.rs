//! Linux per-engine GPU utilization provider (capability
//! `telemetry.gpu.engines`) — the typed-lane successor to the GPUI-private
//! PMU poll loop.
//!
//! Each [`GpuEngineRowsProvider::read_engine_rows`] call performs exactly ONE
//! bounded `invoke_perf_helper` invocation (the audited polkit/pkexec seam in
//! `taskmanager-escalation`, ADR-023 Boundary 2) and maps the typed outcome
//! into a device-scoped snapshot. The call blocks (~1 s sample plus the
//! OS-native prompt on first use) and therefore only ever runs on the
//! dedicated engine-rows lane thread — never a UI thread. Request pacing
//! belongs to the frontend; this provider holds no timer.
//!
//! # Honesty
//!
//! The real `perf_event_open` read remains on-box-unverified: the CLI failure
//! path and the helper JSON contract parsing/classification are unit-tested
//! in `taskmanager-escalation::polkit` (see its `mod tests`), but the live
//! privileged read needs sudo and is an integrator on-box receipt item. No
//! fabricated rows exist on any path: failures stay typed failures.

use taskmanager_core::{
    DeviceId, FailureKind, GpuEngineKind, GpuEngineMetric, GpuEngineRowsSnapshot,
};
use taskmanager_escalation::polkit::{PerfHelperOutcome, PolkitGate, invoke_perf_helper};
use taskmanager_escalation::{
    EscalationAvailability, EscalationDenialReason, EscalationFeature, PrivilegeGate,
};
use taskmanager_platform_contract::{CapabilityStatus, ProviderFailure};
use taskmanager_platform_provider::GpuEngineRowsProvider;

pub(super) struct NativeGpuEngineRowsProvider {
    probe: fn() -> EscalationAvailability,
    invoke: fn() -> PerfHelperOutcome,
}

impl NativeGpuEngineRowsProvider {
    pub(super) const fn new() -> Self {
        Self::with_crossing(probe_intel_pmu_crossing, invoke_perf_helper)
    }

    pub(super) fn initial_status(&self) -> CapabilityStatus {
        capability_status_from_availability((self.probe)())
    }

    const fn with_crossing(
        probe: fn() -> EscalationAvailability,
        invoke: fn() -> PerfHelperOutcome,
    ) -> Self {
        Self { probe, invoke }
    }
}

impl Default for NativeGpuEngineRowsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuEngineRowsProvider for NativeGpuEngineRowsProvider {
    fn read_engine_rows(
        &mut self,
        device_id: &DeviceId,
    ) -> Result<GpuEngineRowsSnapshot, taskmanager_platform_contract::ProviderFailure> {
        match (self.probe)() {
            EscalationAvailability::Available
            | EscalationAvailability::RequiresEscalation(EscalationFeature::IntelPmu) => {
                result_from_outcome((self.invoke)(), device_id.clone())
            }
            EscalationAvailability::RequiresEscalation(_) => Err(ProviderFailure::ProviderFault),
            EscalationAvailability::Denied { reason } => Err(provider_failure_from_denial(reason)),
        }
    }
}

fn probe_intel_pmu_crossing() -> EscalationAvailability {
    PolkitGate::new().probe(EscalationFeature::IntelPmu)
}

fn capability_status_from_availability(availability: EscalationAvailability) -> CapabilityStatus {
    match availability {
        EscalationAvailability::Available => CapabilityStatus::Available,
        EscalationAvailability::RequiresEscalation(EscalationFeature::IntelPmu) => {
            CapabilityStatus::PermissionRequired
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

/// Map one typed helper outcome into a device-scoped snapshot — the single
/// honest crossing from the escalation crate's PMU contract into the typed
/// lane. Mirrors the panel's previous `from_outcome` mapping exactly:
/// `Success` carries rows; `HelperError` (the helper ran, typed ERROR) stays
/// a failure with `ProviderFault`; `Unavailable` keeps its typed denial
/// reason (`PermissionDenied` / `MissingDependency` / `Unsupported`).
fn result_from_outcome(
    outcome: PerfHelperOutcome,
    device_id: DeviceId,
) -> Result<GpuEngineRowsSnapshot, ProviderFailure> {
    match outcome {
        PerfHelperOutcome::Success(success) => Ok(GpuEngineRowsSnapshot::success(
            device_id,
            success
                .engines
                .iter()
                .map(|reading| GpuEngineMetric {
                    name: reading.name.clone(),
                    // The helper reports i915-style engine-class strings
                    // (`rcs`/`vcs`); unmapped labels stay `Unknown` rather
                    // than receiving a guessed semantic (the documented
                    // `from_display_name` contract).
                    kind: GpuEngineKind::from_display_name(&reading.class),
                    utilization_pct: reading.busy_pct,
                })
                .collect(),
        )),
        PerfHelperOutcome::HelperError(error) => Err(match error.kind {
            taskmanager_escalation::polkit::PerfHelperErrorKind::PermissionDenied => {
                ProviderFailure::PermissionDenied
            }
            taskmanager_escalation::polkit::PerfHelperErrorKind::NoPmu => {
                ProviderFailure::Unsupported
            }
            taskmanager_escalation::polkit::PerfHelperErrorKind::OpenFailed
            | taskmanager_escalation::polkit::PerfHelperErrorKind::ReadFailed => {
                ProviderFailure::ProviderFault
            }
        }),
        PerfHelperOutcome::Unavailable { reason, .. } => Err(provider_failure_from_denial(reason)),
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
#[path = "../../tests/headless/linux_provider_gpu_engine_rows_tests.rs"]
mod tests;
