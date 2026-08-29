use super::*;
use taskmanager_platform_contract::CapabilityDescriptor;

#[test]
fn surface_descriptors_accepts_the_exact_declared_table() {
    let snapshot = CapabilitySnapshot::from_descriptors([
        CapabilityDescriptor {
            id: CapabilityId::TELEMETRY_CPU,
            status: CapabilityStatus::TemporarilyUnavailable,
            providers: vec![ProviderId::borrowed("fixture.system.cpu")],
            observed_at_ms: 1,
            last_success_at_ms: None,
        },
        CapabilityDescriptor {
            id: CapabilityId::PROCESS_LIST,
            status: CapabilityStatus::TemporarilyUnavailable,
            providers: vec![ProviderId::borrowed("fixture.process.list")],
            observed_at_ms: 1,
            last_success_at_ms: None,
        },
    ]);

    assert_eq!(
        assert_fresh_surface_descriptors(
            &snapshot,
            &[
                ("telemetry.cpu", "fixture.system.cpu"),
                ("process.list", "fixture.process.list"),
            ],
            "fixture.",
        ),
        Ok(())
    );
}

#[test]
fn surface_descriptors_rejects_availability_before_observation() {
    let snapshot = CapabilitySnapshot::from_descriptors([CapabilityDescriptor {
        id: CapabilityId::TELEMETRY_CPU,
        status: CapabilityStatus::Available,
        providers: vec![ProviderId::borrowed("fixture.system.cpu")],
        observed_at_ms: 1,
        last_success_at_ms: None,
    }]);

    assert!(
        assert_fresh_surface_descriptors(
            &snapshot,
            &[("telemetry.cpu", "fixture.system.cpu")],
            "fixture.",
        )
        .is_err()
    );
}
