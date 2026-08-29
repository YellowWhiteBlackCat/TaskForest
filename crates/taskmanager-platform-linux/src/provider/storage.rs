//! Storage-health and SMART provider implementations.

use taskmanager_core::core::source::SourceStatus;
use taskmanager_core::{
    DeviceState, DeviceStatus, FilesystemHealthSnapshot, SmartSelfTestIntent, SmartSelfTestReport,
    StorageDeviceTarget,
};
use taskmanager_platform_contract::{CompositeSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::{
    FilesystemHealthProvider, SmartSelfTestControlProvider, SmartSelfTestObservationProvider,
};

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
        let snapshot = crate::engine::storage_health::collect_filesystem_health(observed_at_ms);
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
        crate::engine::smart::self_test::start_smart_self_test_for_connection(
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
        )
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
        crate::engine::smart::self_test::read_smart_self_test_status_for_connection(
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
