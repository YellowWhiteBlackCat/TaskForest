use super::*;
use taskmanager_platform_contract::CapabilityDescriptor;

/// The fresh-runtime shape with M3.2 semantics: every declared lane is
/// registered with its typed initial status and provider, and every other
/// product-expected identity answers with its typed absence.
fn fresh_snapshot(declared: &[(&'static str, &'static str)]) -> CapabilitySnapshot {
    let mut descriptors: Vec<CapabilityDescriptor> = Vec::new();
    for (capability, provider) in declared {
        descriptors.push(CapabilityDescriptor {
            id: CapabilityId::borrowed(capability),
            status: CapabilityStatus::TemporarilyUnavailable,
            providers: vec![ProviderId::borrowed(provider)],
            observed_at_ms: 1,
            last_success_at_ms: None,
        });
    }
    for expected in CapabilityId::EXPECTED_SURFACE {
        if declared
            .iter()
            .any(|(capability, _)| *capability == expected.as_str())
        {
            continue;
        }
        descriptors.push(CapabilityDescriptor::typed_absence(expected));
    }
    CapabilitySnapshot::from_descriptors(descriptors)
}

#[test]
fn surface_descriptors_accept_the_declared_face_with_typed_absences() {
    let declared = [
        ("telemetry.cpu", "fixture.system.cpu"),
        ("process.list", "fixture.process.list"),
    ];
    assert_eq!(
        assert_fresh_surface_descriptors(&fresh_snapshot(&declared), &declared, "fixture."),
        Ok(())
    );
}

#[test]
fn surface_descriptors_reject_availability_before_observation() {
    let declared = [("telemetry.cpu", "fixture.system.cpu")];
    let mut snapshot = fresh_snapshot(&declared);
    let mut descriptors: Vec<CapabilityDescriptor> = snapshot.iter().cloned().collect();
    let claimed = descriptors
        .iter_mut()
        .find(|descriptor| descriptor.id == CapabilityId::TELEMETRY_CPU)
        .expect("declared lane");
    claimed.status = CapabilityStatus::Available;
    snapshot = CapabilitySnapshot::from_descriptors(descriptors);

    assert!(
        assert_fresh_surface_descriptors(&snapshot, &declared, "fixture.").is_err(),
        "a fresh lane must not claim availability before its first observation"
    );
}

/// A lane registered but absent from the declared face is a silent
/// registration, not a permitted extra.
#[test]
fn surface_descriptors_reject_an_undeclared_registration() {
    let declared = [("telemetry.cpu", "fixture.system.cpu")];
    let mut descriptors: Vec<CapabilityDescriptor> =
        fresh_snapshot(&declared).iter().cloned().collect();
    descriptors.push(CapabilityDescriptor {
        id: CapabilityId::TELEMETRY_MEMORY,
        status: CapabilityStatus::TemporarilyUnavailable,
        providers: vec![ProviderId::borrowed("fixture.system.memory")],
        observed_at_ms: 1,
        last_success_at_ms: None,
    });

    assert!(
        assert_fresh_surface_descriptors(
            &CapabilitySnapshot::from_descriptors(descriptors),
            &declared,
            "fixture.",
        )
        .is_err(),
        "an undeclared registration must never pass as the declared face"
    );
}

/// A declared lane that silently vanished (replaced by a typed absence) is the
/// forbidden hidden gap.
#[test]
fn surface_descriptors_reject_a_hidden_declared_lane() {
    let declared = [
        ("telemetry.cpu", "fixture.system.cpu"),
        ("process.list", "fixture.process.list"),
    ];
    let mut descriptors: Vec<CapabilityDescriptor> =
        fresh_snapshot(&declared).iter().cloned().collect();
    descriptors.retain(|descriptor| descriptor.id != CapabilityId::PROCESS_LIST);
    descriptors.push(CapabilityDescriptor::typed_absence(
        CapabilityId::PROCESS_LIST,
    ));

    assert!(
        assert_fresh_surface_descriptors(
            &CapabilitySnapshot::from_descriptors(descriptors),
            &declared,
            "fixture.",
        )
        .is_err(),
        "a declared lane must be registered, never silently typed-absent"
    );
}

/// The product-expected surface cannot disappear: an unregistered expected
/// identity without its typed absence is a silent omission.
#[test]
fn surface_descriptors_reject_a_missing_typed_absence() {
    let declared = [("telemetry.cpu", "fixture.system.cpu")];
    let snapshot = CapabilitySnapshot::from_descriptors([CapabilityDescriptor {
        id: CapabilityId::TELEMETRY_CPU,
        status: CapabilityStatus::TemporarilyUnavailable,
        providers: vec![ProviderId::borrowed("fixture.system.cpu")],
        observed_at_ms: 1,
        last_success_at_ms: None,
    }]);

    assert!(
        assert_fresh_surface_descriptors(&snapshot, &declared, "fixture.").is_err(),
        "every unregistered expected capability must stay addressable"
    );
}

/// A typed absence is a product-surface answer only: a fabricated absence for
/// an identity the product never promised is reported, not admitted.
#[test]
fn surface_descriptors_reject_a_fabricated_absence() {
    let declared = [("telemetry.cpu", "fixture.system.cpu")];
    let mut descriptors: Vec<CapabilityDescriptor> =
        fresh_snapshot(&declared).iter().cloned().collect();
    descriptors.push(CapabilityDescriptor::typed_absence(CapabilityId::owned(
        "vendor.probe",
    )));

    assert!(
        assert_fresh_surface_descriptors(
            &CapabilitySnapshot::from_descriptors(descriptors),
            &declared,
            "fixture.",
        )
        .is_err(),
        "a vendor identity is never silently absent on the product surface"
    );
}

/// Provider attribution and the no-false-success timestamp are pinned even
/// though the lane is declared.
#[test]
fn surface_descriptors_reject_fabricated_attribution_and_success() {
    let declared = [("telemetry.cpu", "fixture.system.cpu")];
    let mut descriptors: Vec<CapabilityDescriptor> =
        fresh_snapshot(&declared).iter().cloned().collect();
    if let Some(descriptor) = descriptors.first_mut() {
        descriptor.providers = vec![ProviderId::borrowed("other.system.cpu")];
    }
    assert!(
        assert_fresh_surface_descriptors(
            &CapabilitySnapshot::from_descriptors(descriptors),
            &declared,
            "fixture.",
        )
        .is_err()
    );

    let mut descriptors: Vec<CapabilityDescriptor> =
        fresh_snapshot(&declared).iter().cloned().collect();
    if let Some(descriptor) = descriptors.first_mut() {
        descriptor.last_success_at_ms = Some(1);
    }
    assert!(
        assert_fresh_surface_descriptors(
            &CapabilitySnapshot::from_descriptors(descriptors),
            &declared,
            "fixture.",
        )
        .is_err()
    );
}

/// The layer-B declaration and the live catalog must agree in both directions:
/// the declared-present lane is registered, the registered lane is declared,
/// and a registered-pending lane is declared with its typed absence.
#[test]
fn catalog_surface_accepts_a_declaration_that_mirrors_the_catalog() {
    let registered = [
        ("telemetry.cpu", "fixture.system.cpu"),
        ("process.list", "fixture.process.list"),
    ];
    let surface = PlatformCapabilitySurface::declaring(
        PlatformAxis::Linux,
        &[
            (CapabilityId::TELEMETRY_CPU, PlatformSource::Present),
            (
                CapabilityId::PROCESS_LIST,
                PlatformSource::absent(CapabilityStatus::Unsupported).expect("admissible absence"),
            ),
        ],
    );
    assert_eq!(
        assert_capability_surface_matches_catalog(
            &fresh_snapshot(&registered),
            &surface,
            PlatformAxis::Linux,
            "fixture.",
        ),
        Ok(())
    );

    // Every expected identity is declared: no silent absence is left.
    for expected in CapabilityId::EXPECTED_SURFACE {
        assert_ne!(
            surface.source(PlatformAxis::Linux, &expected),
            PlatformSource::Undeclared
        );
    }
}

/// A `Present` declaration without a registered descriptor is the forbidden
/// fabricated source.
#[test]
fn catalog_surface_rejects_a_present_declaration_without_a_registration() {
    let registered = [("telemetry.cpu", "fixture.system.cpu")];
    let surface = PlatformCapabilitySurface::declaring(
        PlatformAxis::Linux,
        &[
            (CapabilityId::TELEMETRY_CPU, PlatformSource::Present),
            (CapabilityId::TELEMETRY_PRESSURE, PlatformSource::Present),
        ],
    );
    assert!(
        assert_capability_surface_matches_catalog(
            &fresh_snapshot(&registered),
            &surface,
            PlatformAxis::Linux,
            "fixture.",
        )
        .is_err(),
        "a declared source must exist in the catalog"
    );
}

/// An empty declaration is the silent-absence shape M3.2 forbids.
#[test]
fn catalog_surface_rejects_an_undeclared_expected_capability() {
    assert!(
        assert_capability_surface_matches_catalog(
            &fresh_snapshot(&[]),
            &PlatformCapabilitySurface::new(),
            PlatformAxis::Linux,
            "fixture.",
        )
        .is_err(),
        "Undeclared must never pass as the declared surface"
    );
}

/// A registration owned by another platform identity is not this adapter's
/// source, and a fabricated vendor typed absence is still rejected.
#[test]
fn catalog_surface_rejects_foreign_attribution_and_fabricated_absences() {
    let registered = [("telemetry.cpu", "other.system.cpu")];
    let surface = PlatformCapabilitySurface::declaring(
        PlatformAxis::Linux,
        &[(CapabilityId::TELEMETRY_CPU, PlatformSource::Present)],
    );
    assert!(
        assert_capability_surface_matches_catalog(
            &fresh_snapshot(&registered),
            &surface,
            PlatformAxis::Linux,
            "fixture.",
        )
        .is_err(),
        "a lane must be attributed to its own adapter prefix"
    );

    let mut descriptors: Vec<CapabilityDescriptor> = fresh_snapshot(&[]).iter().cloned().collect();
    descriptors.push(CapabilityDescriptor::typed_absence(CapabilityId::owned(
        "vendor.probe",
    )));
    assert!(
        assert_capability_surface_matches_catalog(
            &CapabilitySnapshot::from_descriptors(descriptors),
            &PlatformCapabilitySurface::declaring(PlatformAxis::Linux, &[]),
            PlatformAxis::Linux,
            "fixture.",
        )
        .is_err(),
        "a vendor identity is never silently absent on the product surface"
    );
}
