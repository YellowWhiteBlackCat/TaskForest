use super::*;
use taskmanager_platform_contract::CapabilityId;

/// The full composition edge must construct headlessly on real Windows:
/// all eight lane groups spawn and the handle reports capabilities from
/// the registered providers (no deadlock, no panic).
#[test]
fn spawn_composes_the_complete_runtime() {
    let handle = WindowsPlatformRuntime::spawn().expect("runtime composes");
    let snapshot = handle.capabilities().snapshot();
    // Safe-implemented domains must be present in the catalog.
    for capability in [
        CapabilityId::TELEMETRY_CPU,
        CapabilityId::TELEMETRY_MEMORY,
        CapabilityId::PROCESS_LIST,
    ] {
        assert!(
            snapshot.get(&capability).is_some(),
            "catalog must contain {capability:?}"
        );
    }
}
