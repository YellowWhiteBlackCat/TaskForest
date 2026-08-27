use taskmanager_core::{DeviceGeneration, StorageDeviceKind, StorageInterconnect, StorageProtocol};

use super::*;

fn disk(id: &str, generation: u64, locator: &str) -> DiskMetrics {
    taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .device_id(id.into())
        .device_generation(DeviceGeneration::new(generation))
        .name(locator.into())
        .connection(StorageConnection::new(
            StorageProtocol::Nvme,
            StorageInterconnect::Usb,
            StorageDeviceKind::Physical,
        ))
        .identity_stability(StorageIdentityStability::Persistent)
        .build()
}

fn target(id: &str, generation: u64, locator: &str) -> StorageDeviceTarget {
    StorageDeviceTarget {
        device_id: DeviceId::new(id),
        device_generation: DeviceGeneration::new(generation),
        locator: StorageDeviceKey::new(locator),
    }
}

#[test]
fn authoritative_snapshot_resolves_all_identity_axes_and_connection() {
    let registry = StorageTargetRegistry::new();
    let resolver = registry.resolver();
    registry.publish(
        &[disk("disk:wwid:fixture", 3, "/dev/nvme0n1")],
        SourceOutcome::Available,
        SourceOutcome::Available,
    );

    let resolved = resolver
        .resolve(&target("disk:wwid:fixture", 3, "/dev/nvme0n1"))
        .expect("current target should resolve");

    assert_eq!(resolved.device_generation, DeviceGeneration::new(3));
    assert_eq!(resolved.locator.as_str(), "/dev/nvme0n1");
    assert_eq!(resolved.connection.protocol, StorageProtocol::Nvme);
    assert_eq!(resolved.connection.interconnect, StorageInterconnect::Usb);
}

#[test]
fn partial_or_unavailable_discovery_discards_last_good_targets() {
    let registry = StorageTargetRegistry::new();
    let resolver = registry.resolver();
    let requested = target("disk:wwid:fixture", 1, "/dev/sda");
    registry.publish(
        &[disk("disk:wwid:fixture", 1, "/dev/sda")],
        SourceOutcome::Available,
        SourceOutcome::Available,
    );
    assert!(resolver.resolve(&requested).is_ok());

    registry.publish(
        &[disk("disk:wwid:fixture", 1, "/dev/sda")],
        SourceOutcome::Partial(FailureKind::PermissionDenied),
        SourceOutcome::Available,
    );
    assert_eq!(
        resolver.resolve(&requested),
        Err(ProviderFailure::PermissionDenied)
    );

    registry.publish(
        &[],
        SourceOutcome::Available,
        SourceOutcome::Partial(FailureKind::TimedOut),
    );
    assert_eq!(resolver.resolve(&requested), Err(ProviderFailure::TimedOut));

    registry.publish(
        &[],
        SourceOutcome::Available,
        SourceOutcome::Unavailable(FailureKind::MissingDependency),
    );
    assert_eq!(
        resolver.resolve(&requested),
        Err(ProviderFailure::MissingDependency)
    );
}

#[test]
fn missing_generation_and_locator_drift_are_identity_changes() {
    let registry = StorageTargetRegistry::new();
    let resolver = registry.resolver();
    registry.publish(
        &[disk("disk:wwid:fixture", 4, "/dev/sdb")],
        SourceOutcome::Available,
        SourceOutcome::Available,
    );

    for stale in [
        target("disk:wwid:missing", 4, "/dev/sdb"),
        target("disk:wwid:fixture", 3, "/dev/sdb"),
        target("disk:wwid:fixture", 4, "/dev/sda"),
    ] {
        assert_eq!(
            resolver.resolve(&stale),
            Err(ProviderFailure::IdentityChanged)
        );
    }
}

#[test]
fn duplicate_stable_ids_and_shared_locators_are_rejected_as_ambiguous() {
    let registry = StorageTargetRegistry::new();
    let resolver = registry.resolver();
    registry.publish(
        &[
            disk("disk:wwid:duplicate", 1, "/dev/sda"),
            disk("disk:wwid:duplicate", 1, "/dev/sdb"),
            disk("disk:wwid:left", 1, "/dev/shared"),
            disk("disk:wwid:right", 1, "/dev/shared"),
        ],
        SourceOutcome::Available,
        SourceOutcome::Available,
    );

    for ambiguous in [
        target("disk:wwid:duplicate", 1, "/dev/sda"),
        target("disk:wwid:left", 1, "/dev/shared"),
        target("disk:wwid:right", 1, "/dev/shared"),
    ] {
        assert_eq!(resolver.resolve(&ambiguous), Err(ProviderFailure::Rejected));
    }
}

#[test]
fn attachment_scoped_identity_cannot_authorize_a_mutation() {
    let registry = StorageTargetRegistry::new();
    let resolver = registry.resolver();
    let mut attachment = disk("disk:path:sda", 1, "/dev/sda");
    attachment.identity_stability = StorageIdentityStability::Attachment;
    registry.publish(
        &[attachment],
        SourceOutcome::Available,
        SourceOutcome::Available,
    );

    assert_eq!(
        resolver.resolve(&target("disk:path:sda", 1, "/dev/sda")),
        Err(ProviderFailure::Rejected)
    );
}

#[test]
fn logical_or_virtual_storage_cannot_authorize_a_physical_self_test() {
    for (device_kind, locator) in [
        (StorageDeviceKind::Virtual, "/dev/dm-0"),
        (StorageDeviceKind::Aggregate, "/dev/md0"),
    ] {
        let registry = StorageTargetRegistry::new();
        let resolver = registry.resolver();
        let mut logical = disk("disk:wwid:logical", 1, locator);
        logical.apply_connection(StorageConnection::new(
            StorageProtocol::Unknown,
            StorageInterconnect::Platform,
            device_kind,
        ));
        registry.publish(
            &[logical],
            SourceOutcome::Available,
            SourceOutcome::Available,
        );

        assert_eq!(
            resolver.resolve(&target("disk:wwid:logical", 1, locator)),
            Err(ProviderFailure::Rejected),
            "{device_kind:?} storage must not become a physical SMART command target"
        );
    }
}

#[test]
fn live_verifier_rejects_same_locator_after_physical_identity_changes() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-storage-target-verifier-{}",
        std::process::id()
    ));
    let device = root.join("sda/device");
    fs::create_dir_all(&device).expect("create verifier fixture");
    fs::write(root.join("sda/wwid"), "fixture-a\n").expect("write first identity");
    let verifier = LiveStorageTargetVerifier::with_root(&root);
    let resolved = ResolvedStorageTarget {
        device_id: DeviceId::new("disk:wwid:fixture-a"),
        device_generation: DeviceGeneration::INITIAL,
        locator: StorageDeviceKey::new("/dev/sda"),
        connection: StorageConnection::default(),
        identity_stability: StorageIdentityStability::Persistent,
    };

    assert_eq!(verifier.verify(&resolved), Ok(()));
    fs::write(root.join("sda/wwid"), "fixture-b\n").expect("replace identity");
    assert_eq!(
        verifier.verify(&resolved),
        Err(ProviderFailure::IdentityChanged)
    );

    fs::remove_dir_all(root).expect("remove verifier fixture");
}

#[test]
fn live_metadata_failures_preserve_the_strongest_actionable_reason() {
    assert_eq!(
        stronger_provider_failure(
            Some(ProviderFailure::TimedOut),
            ProviderFailure::PermissionDenied,
        ),
        ProviderFailure::PermissionDenied
    );
    assert_eq!(
        stronger_provider_failure(
            Some(ProviderFailure::PermissionDenied),
            ProviderFailure::TimedOut,
        ),
        ProviderFailure::PermissionDenied
    );
    assert_eq!(
        stronger_provider_failure(None, ProviderFailure::Unsupported),
        ProviderFailure::Unsupported
    );
}
