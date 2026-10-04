//! One permission-state fold and identity-frozen action for every frontend.

use taskmanager_application::{
    GpuEngineRowsState, MsrReadoutRequestFailure, MsrReadoutState, PlatformEffect,
    RaplPowerRequestFailure, RaplPowerState, SmbiosMemoryRequestFailure, SmbiosMemoryState,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_platform_contract::{CapabilityId, CapabilityStatus};

use super::gpu_engine_rows::{GpuEngineRowsPresentation, present_gpu_engine_rows};
use crate::ShellApp;

/// Immutable inputs shared by the direct track and the shell track.
pub struct PrivilegeCenterInputs<'a> {
    pub gpu_engine_state: &'a GpuEngineRowsState,
    pub gpu_engine_capability: Option<CapabilityStatus>,
    pub gpu_engine_device_id: Option<DeviceId>,
    pub smbios_state: &'a SmbiosMemoryState,
    pub smbios_capability: Option<CapabilityStatus>,
    pub rapl_state: &'a RaplPowerState,
    pub rapl_capability: Option<CapabilityStatus>,
    pub msr_state: &'a MsrReadoutState,
    pub msr_capability: Option<CapabilityStatus>,
}

impl<'a> PrivilegeCenterInputs<'a> {
    #[must_use]
    pub fn from_shell(shell: &'a ShellApp) -> Self {
        let projection = shell.projection();
        Self {
            gpu_engine_state: shell.gpu_engine_rows_state(),
            gpu_engine_capability: projection
                .capability_status(&CapabilityId::TELEMETRY_GPU_ENGINES),
            gpu_engine_device_id: projection
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.gpu.first())
                .filter(|gpu| !gpu.device_id.trim().is_empty())
                .map(|gpu| DeviceId::new(gpu.device_id.clone())),
            smbios_state: shell.smbios_memory_state(),
            smbios_capability: projection.capability_status(&CapabilityId::TELEMETRY_MEMORY_SMBIOS),
            rapl_state: shell.rapl_power_state(),
            rapl_capability: projection
                .capability_status(&CapabilityId::TELEMETRY_CPU_PACKAGE_POWER),
            msr_state: shell.msr_readout_state(),
            msr_capability: projection.capability_status(&CapabilityId::TELEMETRY_CPU_MSR),
        }
    }

    #[must_use]
    pub fn rows(&self) -> Vec<PrivilegeRow> {
        let mut rows = Vec::new();
        if let Some(id) = self.gpu_engine_device_id.as_ref() {
            let state = match present_gpu_engine_rows(
                self.gpu_engine_state,
                id,
                self.gpu_engine_capability,
            ) {
                GpuEngineRowsPresentation::PermissionRequired => {
                    PrivilegeRowState::NeedsAuthorization
                }
                GpuEngineRowsPresentation::Loading => PrivilegeRowState::Authorizing,
                GpuEngineRowsPresentation::Active(_) => PrivilegeRowState::Enabled,
                GpuEngineRowsPresentation::PermissionDenied => PrivilegeRowState::Denied,
                GpuEngineRowsPresentation::MissingDependency
                | GpuEngineRowsPresentation::AuthorizationUnavailable => {
                    PrivilegeRowState::Unavailable
                }
                GpuEngineRowsPresentation::Unsupported => PrivilegeRowState::Unsupported,
                GpuEngineRowsPresentation::Failed => PrivilegeRowState::Failed,
            };
            rows.push(PrivilegeRow::new(
                "gpu-engines",
                "gpu.per_engine_title",
                state,
                self.gpu_engine_capability,
                PrivilegeAction::GpuEngines(id.clone()),
            ));
        } else {
            rows.push(PrivilegeRow {
                id: "gpu-engines",
                label_key: "gpu.per_engine_title",
                state: capability_state(self.gpu_engine_capability)
                    .unwrap_or(PrivilegeRowState::Unavailable),
                action: None,
            });
        }
        {
            let state = smbios_state(self.smbios_state, self.smbios_capability)
                .unwrap_or(PrivilegeRowState::Unavailable);
            rows.push(PrivilegeRow::new(
                "smbios-memory",
                "system.memory_inventory",
                state,
                self.smbios_capability,
                PrivilegeAction::SmbiosMemory,
            ));
        }
        {
            let state = rapl_state(self.rapl_state, self.rapl_capability)
                .unwrap_or(PrivilegeRowState::Unavailable);
            rows.push(PrivilegeRow::new(
                "rapl-power",
                "cpu.package_power",
                state,
                self.rapl_capability,
                PrivilegeAction::RaplPower,
            ));
        }
        {
            let state = msr_state(self.msr_state, self.msr_capability)
                .unwrap_or(PrivilegeRowState::Unavailable);
            rows.push(PrivilegeRow::new(
                "msr-readouts",
                "cpu.msr_readouts",
                state,
                self.msr_capability,
                PrivilegeAction::MsrReadouts,
            ));
        }
        rows
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrivilegeAction {
    GpuEngines(DeviceId),
    SmbiosMemory,
    RaplPower,
    MsrReadouts,
}

impl PrivilegeAction {
    #[must_use]
    pub fn effect(&self) -> PlatformEffect {
        match self {
            Self::GpuEngines(id) => ShellApp::request_gpu_engine_rows(id.clone()),
            Self::SmbiosMemory => ShellApp::request_smbios_memory(),
            Self::RaplPower => ShellApp::request_rapl_power(),
            Self::MsrReadouts => ShellApp::request_msr_readouts(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivilegeRowState {
    NeedsAuthorization,
    Authorizing,
    Enabled,
    Denied,
    Unavailable,
    Unsupported,
    Failed,
}

impl PrivilegeRowState {
    #[must_use]
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::NeedsAuthorization => "settings.privileges_authorize_hint",
            Self::Authorizing => "settings.privileges_authorizing",
            Self::Enabled => "settings.privileges_enabled",
            Self::Denied => "settings.privileges_denied",
            Self::Unavailable => "settings.privileges_unavailable",
            Self::Unsupported => "settings.privileges_unsupported",
            Self::Failed => "settings.privileges_failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivilegeRow {
    pub id: &'static str,
    pub label_key: &'static str,
    pub state: PrivilegeRowState,
    pub action: Option<PrivilegeAction>,
}

impl PrivilegeRow {
    fn new(
        id: &'static str,
        label_key: &'static str,
        state: PrivilegeRowState,
        capability: Option<CapabilityStatus>,
        action: PrivilegeAction,
    ) -> Self {
        // A permission gate alone does not offer an OS authorization seam.
        let requestable = matches!(
            capability,
            Some(
                CapabilityStatus::Available
                    | CapabilityStatus::RequiresEscalation
                    | CapabilityStatus::Degraded(FailureKind::RequiresEscalation)
            )
        );
        Self {
            id,
            label_key,
            state,
            action: (requestable
                && matches!(
                    state,
                    PrivilegeRowState::NeedsAuthorization | PrivilegeRowState::Denied
                ))
            .then_some(action),
        }
    }
}

fn smbios_state(
    state: &SmbiosMemoryState,
    capability: Option<CapabilityStatus>,
) -> Option<PrivilegeRowState> {
    match state {
        SmbiosMemoryState::Ready(_) => Some(PrivilegeRowState::Enabled),
        SmbiosMemoryState::Loading { .. } => Some(PrivilegeRowState::Authorizing),
        SmbiosMemoryState::Failed(failed) => {
            Some(state_from_failure(smbios_failure_kind(&failed.failure)))
        }
        SmbiosMemoryState::Closed => capability_state(capability),
    }
}

fn rapl_state(
    state: &RaplPowerState,
    capability: Option<CapabilityStatus>,
) -> Option<PrivilegeRowState> {
    match state {
        RaplPowerState::Ready(_) => Some(PrivilegeRowState::Enabled),
        RaplPowerState::Loading { .. } => Some(PrivilegeRowState::Authorizing),
        RaplPowerState::Failed(failed) => {
            Some(state_from_failure(rapl_failure_kind(&failed.failure)))
        }
        RaplPowerState::Closed => capability_state(capability),
    }
}

fn msr_state(
    state: &MsrReadoutState,
    capability: Option<CapabilityStatus>,
) -> Option<PrivilegeRowState> {
    match state {
        MsrReadoutState::Ready(_) => Some(PrivilegeRowState::Enabled),
        MsrReadoutState::Loading { .. } => Some(PrivilegeRowState::Authorizing),
        MsrReadoutState::Failed(failed) => {
            Some(state_from_failure(msr_failure_kind(&failed.failure)))
        }
        MsrReadoutState::Closed => capability_state(capability),
    }
}

fn capability_state(status: Option<CapabilityStatus>) -> Option<PrivilegeRowState> {
    match status? {
        // `RequiresEscalation` is the escalatable state; `PermissionRequired`
        // is a permission gate. Both need one explicit authorization decision,
        // so both expose the same affordance in this center.
        CapabilityStatus::Available
        | CapabilityStatus::PermissionRequired
        | CapabilityStatus::RequiresEscalation => Some(PrivilegeRowState::NeedsAuthorization),
        CapabilityStatus::Degraded(kind) => Some(state_from_failure(kind)),
        CapabilityStatus::Unsupported => Some(PrivilegeRowState::Unsupported),
        CapabilityStatus::MissingDependency
        | CapabilityStatus::TemporarilyUnavailable
        | CapabilityStatus::Stale => Some(PrivilegeRowState::Unavailable),
    }
}

const fn state_from_failure(kind: FailureKind) -> PrivilegeRowState {
    match kind {
        FailureKind::RequiresEscalation => PrivilegeRowState::NeedsAuthorization,
        FailureKind::PermissionDenied => PrivilegeRowState::Denied,
        FailureKind::Unsupported => PrivilegeRowState::Unsupported,
        FailureKind::ProviderFault | FailureKind::Rejected => PrivilegeRowState::Failed,
        FailureKind::MissingDependency
        | FailureKind::TimedOut
        | FailureKind::TemporarilyUnavailable
        | FailureKind::IdentityChanged => PrivilegeRowState::Unavailable,
    }
}

const fn smbios_failure_kind(failure: &SmbiosMemoryRequestFailure) -> FailureKind {
    match failure {
        SmbiosMemoryRequestFailure::Submission(kind) => *kind,
        SmbiosMemoryRequestFailure::Provider(failure) => failure.kind,
    }
}

const fn rapl_failure_kind(failure: &RaplPowerRequestFailure) -> FailureKind {
    match failure {
        RaplPowerRequestFailure::Submission(kind) => *kind,
        RaplPowerRequestFailure::Provider(failure) => failure.kind,
    }
}

const fn msr_failure_kind(failure: &MsrReadoutRequestFailure) -> FailureKind {
    match failure {
        MsrReadoutRequestFailure::Submission(kind) => *kind,
        MsrReadoutRequestFailure::Provider(failure) => failure.kind,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/presentation_privilege_center.rs"]
mod tests;
