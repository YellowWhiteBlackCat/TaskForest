//! source-inspection: static-policy
//!
//! Public-boundary guard for the native adapter/provider SPI dependency.

use std::fs;
use std::path::Path;

const PROVIDER_TRAITS: &[&str] = &[
    "CommandLaunchProvider",
    "CpuTelemetryProvider",
    "DesktopAppearanceProvider",
    "FilesystemHealthProvider",
    "GpuTelemetryProvider",
    "HardwareInventoryProvider",
    "HostTelemetryProvider",
    "MemoryTelemetryProvider",
    "NetworkTelemetryProvider",
    "PowerSupplyProvider",
    "ProcessAffinityControlProvider",
    "ProcessAffinityProvider",
    "ProcessControlProvider",
    "ProcessGpuProvider",
    "ProcessIsolationProvider",
    "ProcessListProvider",
    "ProcessNetworkProvider",
    "ProcessResourcesProvider",
    "ResourceRevealProvider",
    "SensorProvider",
    "ServiceControlProvider",
    "ServiceDependenciesProvider",
    "ServiceInventoryProvider",
    "ServiceLogSnapshotProvider",
    "ServiceLogStreamProvider",
    "SessionControlProvider",
    "SessionInventoryProvider",
    "SmartSelfTestControlProvider",
    "SmartSelfTestObservationProvider",
    "StartupControlProvider",
    "StartupEvidenceProvider",
    "StartupInventoryProvider",
    "StorageTelemetryProvider",
    "UrlOpenProvider",
];

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Linux adapter must remain under the workspace crates directory")
}

fn declares_identifier(source: &str, identifier: &str) -> bool {
    source
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .any(|token| token == identifier)
}

#[test]
fn provider_spi_has_one_public_crate_authority() {
    let linux_facade =
        fs::read_to_string(repository().join("crates/taskmanager-platform-linux/src/lib.rs"))
            .expect("Linux adapter facade should be readable");
    let linux_backend =
        fs::read_to_string(repository().join("crates/taskmanager-platform-linux/src/backend.rs"))
            .expect("Linux composition facade should be readable");
    let provider_spi =
        fs::read_to_string(repository().join("crates/taskmanager-platform-provider/src/lib.rs"))
            .expect("provider SPI facade should be readable");

    assert!(
        !linux_facade.contains("taskmanager_platform_provider"),
        "Linux adapter facade must not create a second public provider SPI import path"
    );
    assert!(
        !linux_backend.contains("pub use taskmanager_platform_provider"),
        "Linux composition facade must not aggregate the provider SPI"
    );
    for provider in PROVIDER_TRAITS {
        assert!(
            !declares_identifier(&linux_facade, provider),
            "Linux adapter must not re-export shared provider SPI trait {provider}"
        );
        assert!(
            !declares_identifier(&linux_backend, provider),
            "Linux composition facade must import provider traits only in the domain that uses them: {provider}"
        );
        assert!(
            declares_identifier(&provider_spi, provider),
            "shared provider SPI facade must remain the public authority for {provider}"
        );
    }
}
