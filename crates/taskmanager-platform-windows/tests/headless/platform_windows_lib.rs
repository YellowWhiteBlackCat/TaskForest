use super::*;

/// The full composition edge must construct headlessly on real Windows:
/// all eight lane groups spawn and the handle reports capabilities from
/// the registered providers (no deadlock, no panic).
#[test]
fn spawn_composes_the_complete_runtime() {
    let handle = WindowsPlatformRuntime::spawn().expect("runtime composes");
    let snapshot = handle.capabilities().snapshot();
    // Safe-implemented domains must be present in the catalog.
    for capability in [
        taskmanager_platform_contract::CapabilityId::TELEMETRY_CPU,
        taskmanager_platform_contract::CapabilityId::TELEMETRY_MEMORY,
        taskmanager_platform_contract::CapabilityId::PROCESS_LIST,
    ] {
        assert!(
            snapshot.get(&capability).is_some(),
            "catalog must contain {capability:?}"
        );
    }
}
