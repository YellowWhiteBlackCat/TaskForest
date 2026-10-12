//! Storage-health and SMART provider implementations.

use taskmanager_core::core::smart::{SmartSelfTestFailure, SmartSelfTestKind, SmartSelfTestPhase};
use taskmanager_core::core::source::SourceStatus;
use taskmanager_core::{
    DeviceState, DeviceStatus, FilesystemHealthSnapshot, SmartSelfTestIntent, SmartSelfTestReport,
    StorageDeviceTarget, StorageInterconnect, StorageProtocol,
};
use taskmanager_escalation::polkit::{
    PolkitGate, SmartSelfTestHelperFailure, SmartSelfTestHelperOutcome, SmartSelfTestHelperRequest,
    invoke_smart_self_test_helper,
};
use taskmanager_escalation::{
    EscalationAvailability, EscalationDenialReason, EscalationFeature, PrivilegeGate,
};
use taskmanager_platform_contract::{CompositeSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::{
    FilesystemHealthProvider, SmartSelfTestControlProvider, SmartSelfTestObservationProvider,
};

use crate::engine::smart::self_test::{
    failed_report, read_smart_self_test_status_for_connection, start_smart_self_test_for_connection,
};
use crate::engine::storage_health::collect_filesystem_health;
use crate::engine::storage_target::{
    LiveStorageTargetVerifier, ResolvedStorageTarget, StorageTargetResolver,
};
use crate::provider::source_status::source_status_from_device_state;

pub(super) struct NativeFilesystemHealthProvider;

impl FilesystemHealthProvider for NativeFilesystemHealthProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<CompositeSourceSnapshot<FilesystemHealthSnapshot>, ProviderFailure> {
        let snapshot = collect_filesystem_health(observed_at_ms);
        let sources = filesystem_health_sources(&snapshot);
        Ok(CompositeSourceSnapshot::new(snapshot, sources))
    }
}

pub(super) struct NativeSmartSelfTestControlProvider {
    pub(super) target_resolver: StorageTargetResolver,
    pub(super) target_verifier: LiveStorageTargetVerifier,
}

impl SmartSelfTestControlProvider for NativeSmartSelfTestControlProvider {
    fn start(
        &mut self,
        intent: &SmartSelfTestIntent,
        observed_at_ms: u64,
    ) -> Result<SmartSelfTestReport, ProviderFailure> {
        let target = intent.target();
        let resolved = self.target_resolver.resolve(&target)?;
        let report = start_smart_self_test_for_connection(
            resolved.locator.as_str(),
            resolved.connection,
            intent.kind,
            observed_at_ms,
            || {
                revalidate_storage_target(
                    &self.target_resolver,
                    &self.target_verifier,
                    &target,
                    &resolved,
                )
            },
        )?;
        if report.failure != Some(SmartSelfTestFailure::PermissionDenied) {
            return Ok(report);
        }

        revalidate_storage_target(
            &self.target_resolver,
            &self.target_verifier,
            &target,
            &resolved,
        )?;
        match PolkitGate::new().probe(EscalationFeature::StorageSelfTest) {
            EscalationAvailability::RequiresEscalation(EscalationFeature::StorageSelfTest)
            | EscalationAvailability::Available => {}
            EscalationAvailability::RequiresEscalation(_) => {
                return Ok(failed_report(
                    SmartSelfTestFailure::RequiresEscalation,
                    observed_at_ms,
                ));
            }
            EscalationAvailability::Denied { reason } => {
                return Ok(failed_report(
                    failure_from_authorization_denial(reason),
                    observed_at_ms,
                ));
            }
        }

        let request = SmartSelfTestHelperRequest {
            device_id: target.device_id.as_str(),
            device_generation: target.device_generation.get(),
            locator: target.locator.as_str(),
            protocol: storage_protocol_key(resolved.connection.protocol),
            interconnect: storage_interconnect_key(resolved.connection.interconnect),
            kind: smart_self_test_kind_key(intent.kind),
        };
        match invoke_smart_self_test_helper(request) {
            SmartSelfTestHelperOutcome::Started => Ok(SmartSelfTestReport {
                state: DeviceState::healthy(observed_at_ms),
                phase: SmartSelfTestPhase::Running,
                kind: Some(intent.kind),
                progress_pct: Some(0.0),
                ..Default::default()
            }),
            SmartSelfTestHelperOutcome::HelperError(failure) => {
                helper_failure_report(failure, observed_at_ms)
            }
            SmartSelfTestHelperOutcome::Unavailable { reason, .. } => Ok(failed_report(
                failure_from_authorization_denial(reason),
                observed_at_ms,
            )),
        }
    }
}

fn helper_failure_report(
    failure: SmartSelfTestHelperFailure,
    now_ms: u64,
) -> Result<SmartSelfTestReport, ProviderFailure> {
    if failure == SmartSelfTestHelperFailure::IdentityChanged {
        return Err(ProviderFailure::IdentityChanged);
    }
    let failure = match failure {
        SmartSelfTestHelperFailure::InvalidTarget => SmartSelfTestFailure::InvalidDevice,
        SmartSelfTestHelperFailure::SupportUnknown => SmartSelfTestFailure::SupportUnknown,
        SmartSelfTestHelperFailure::IdentityChanged => SmartSelfTestFailure::InvalidDevice,
        SmartSelfTestHelperFailure::MissingTool => SmartSelfTestFailure::MissingTool,
        SmartSelfTestHelperFailure::PermissionDenied => SmartSelfTestFailure::PermissionDenied,
        SmartSelfTestHelperFailure::TimedOut => SmartSelfTestFailure::TimedOut,
        SmartSelfTestHelperFailure::Unsupported => SmartSelfTestFailure::Unsupported,
        SmartSelfTestHelperFailure::Rejected => SmartSelfTestFailure::Rejected,
        SmartSelfTestHelperFailure::ProviderUnavailable => {
            SmartSelfTestFailure::ProviderUnavailable
        }
    };
    Ok(failed_report(failure, now_ms))
}

fn failure_from_authorization_denial(reason: EscalationDenialReason) -> SmartSelfTestFailure {
    match reason {
        EscalationDenialReason::Unsupported => SmartSelfTestFailure::Unsupported,
        EscalationDenialReason::PermissionDenied => SmartSelfTestFailure::PermissionDenied,
        EscalationDenialReason::AuthorizationUnavailable => {
            SmartSelfTestFailure::AuthorizationUnavailable
        }
        EscalationDenialReason::HelperUnavailable => SmartSelfTestFailure::HelperUnavailable,
        EscalationDenialReason::HelperProtocolViolation => {
            SmartSelfTestFailure::HelperProtocolViolation
        }
    }
}

const fn smart_self_test_kind_key(kind: SmartSelfTestKind) -> &'static str {
    match kind {
        SmartSelfTestKind::Short => "short",
        SmartSelfTestKind::Extended => "extended",
        SmartSelfTestKind::Conveyance => "conveyance",
    }
}

const fn storage_protocol_key(protocol: StorageProtocol) -> &'static str {
    match protocol {
        StorageProtocol::Nvme => "nvme",
        StorageProtocol::Ata => "ata",
        StorageProtocol::Scsi => "scsi",
        StorageProtocol::Mmc => "mmc",
        StorageProtocol::Sd => "sd",
        StorageProtocol::Ufs => "ufs",
        StorageProtocol::Other => "other",
        StorageProtocol::Unknown => "unknown",
    }
}

const fn storage_interconnect_key(interconnect: StorageInterconnect) -> &'static str {
    match interconnect {
        StorageInterconnect::Pcie => "pcie",
        StorageInterconnect::Sata => "sata",
        StorageInterconnect::Sas => "sas",
        StorageInterconnect::Usb => "usb",
        StorageInterconnect::Mmc => "mmc",
        StorageInterconnect::Sd => "sd",
        StorageInterconnect::Ufs => "ufs",
        StorageInterconnect::Ide => "ide",
        StorageInterconnect::Virtio => "virtio",
        StorageInterconnect::FibreChannel => "fibre_channel",
        StorageInterconnect::Iscsi => "iscsi",
        StorageInterconnect::Network => "network",
        StorageInterconnect::PcieTunnel => "pcie_tunnel",
        StorageInterconnect::FireWire => "fire_wire",
        StorageInterconnect::Platform => "platform",
        StorageInterconnect::Other => "other",
        StorageInterconnect::Unknown => "unknown",
    }
}

pub(super) struct NativeSmartSelfTestObservationProvider {
    pub(super) target_resolver: StorageTargetResolver,
    pub(super) target_verifier: LiveStorageTargetVerifier,
}

impl SmartSelfTestObservationProvider for NativeSmartSelfTestObservationProvider {
    fn refresh(
        &mut self,
        target: &StorageDeviceTarget,
        previous: DeviceState,
        observed_at_ms: u64,
    ) -> Result<SmartSelfTestReport, ProviderFailure> {
        let resolved = self.target_resolver.resolve(target)?;
        read_smart_self_test_status_for_connection(
            resolved.locator.as_str(),
            resolved.connection,
            previous,
            observed_at_ms,
            || {
                revalidate_storage_target(
                    &self.target_resolver,
                    &self.target_verifier,
                    target,
                    &resolved,
                )
            },
        )
    }
}

fn revalidate_storage_target(
    resolver: &StorageTargetResolver,
    verifier: &LiveStorageTargetVerifier,
    target: &StorageDeviceTarget,
    expected: &ResolvedStorageTarget,
) -> Result<(), ProviderFailure> {
    let current = resolver.resolve(target)?;
    if &current != expected {
        return Err(ProviderFailure::IdentityChanged);
    }
    verifier.verify(&current)
}

fn filesystem_health_sources(snapshot: &FilesystemHealthSnapshot) -> Vec<SourceStatus> {
    let mut sources = vec![source_status_from_device_state(
        "linux.storage.mountinfo",
        snapshot.state.status,
        snapshot.filesystems.len(),
        snapshot.filesystems.len(),
    )];
    for (filesystem_type, provider) in [
        ("ext4", "linux.storage.integrity.ext4"),
        ("btrfs", "linux.storage.integrity.btrfs"),
        ("xfs", "linux.storage.integrity.xfs"),
    ] {
        let relevant = snapshot
            .filesystems
            .iter()
            .filter(|filesystem| filesystem.fs_type == filesystem_type)
            .collect::<Vec<_>>();
        let healthy = relevant
            .iter()
            .filter(|filesystem| filesystem.integrity_state.status == DeviceStatus::Healthy)
            .count();
        let status = aggregate_device_status(
            relevant
                .iter()
                .map(|filesystem| filesystem.integrity_state.status),
        );
        sources.push(source_status_from_device_state(
            provider,
            status,
            healthy,
            relevant.len(),
        ));
    }
    sources
}

fn aggregate_device_status(statuses: impl Iterator<Item = DeviceStatus>) -> DeviceStatus {
    let statuses = statuses.collect::<Vec<_>>();
    if statuses.is_empty() {
        DeviceStatus::Unsupported
    } else if statuses.contains(&DeviceStatus::PermissionDenied) {
        DeviceStatus::PermissionDenied
    } else if statuses.contains(&DeviceStatus::MissingTool) {
        DeviceStatus::MissingTool
    } else if statuses.contains(&DeviceStatus::Stale) {
        DeviceStatus::Stale
    } else if statuses.contains(&DeviceStatus::Healthy) {
        DeviceStatus::Healthy
    } else {
        DeviceStatus::Unsupported
    }
}

#[cfg(test)]
#[path = "../../tests/headless/linux_provider_storage_tests.rs"]
mod tests;
