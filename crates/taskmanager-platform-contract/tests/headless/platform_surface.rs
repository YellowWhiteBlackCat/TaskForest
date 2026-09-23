//! Static registration-surface tests: the typed absence constructor, the
//! undeclared default, and deterministic declaration order.
//!
//! These pin the owner half of layer B: a static absence can only carry one of
//! the four projections `ProviderFailure::capability_status` produces, an
//! undeclared pair is a typed visible fact instead of a default success, and a
//! redeclaration never duplicates an entry.

use super::*;
use taskmanager_core::FailureKind;

/// The typed constructor accepts exactly the four absence projections and
/// refuses every runtime state: a transient is never a static commitment.
#[test]
fn absent_accepts_the_absence_projections_and_refuses_runtime_states() {
    for status in [
        CapabilityStatus::Unsupported,
        CapabilityStatus::PermissionRequired,
        CapabilityStatus::RequiresEscalation,
        CapabilityStatus::MissingDependency,
    ] {
        assert!(PlatformSource::is_absence_projection(status), "{status:?}");
        assert_eq!(
            PlatformSource::absent(status).expect("absence projection"),
            PlatformSource::Absent(status)
        );
    }
    for status in [
        CapabilityStatus::Available,
        CapabilityStatus::Degraded(FailureKind::TimedOut),
        CapabilityStatus::TemporarilyUnavailable,
        CapabilityStatus::Stale,
    ] {
        assert!(!PlatformSource::is_absence_projection(status), "{status:?}");
        assert_eq!(
            PlatformSource::absent(status).expect_err("runtime state"),
            PlatformSourceError { status }
        );
    }
}

/// An empty surface answers `Undeclared` - the typed silent-absence fact - and
/// never a default success.
#[test]
fn an_undeclared_pair_is_typed_rather_than_defaulted() {
    let surface = PlatformCapabilitySurface::new();
    for platform in PlatformAxis::ALL {
        assert_eq!(
            surface.source(platform, &CapabilityId::TELEMETRY_CPU),
            PlatformSource::Undeclared
        );
    }
    assert_eq!(surface.entries().count(), 0);
}

/// Redeclaring the same pair replaces it and returns the previous declaration;
/// entries are unique and ordered by `(platform, capability)`.
#[test]
fn declarations_replace_and_stay_deterministic() {
    let mut surface = PlatformCapabilitySurface::new();
    assert_eq!(
        surface.declare(
            PlatformAxis::Linux,
            CapabilityId::TELEMETRY_CPU,
            PlatformSource::Present,
        ),
        None
    );
    assert_eq!(
        surface.declare(
            PlatformAxis::Linux,
            CapabilityId::TELEMETRY_CPU,
            PlatformSource::Absent(CapabilityStatus::Unsupported),
        ),
        Some(PlatformSource::Present)
    );
    surface.declare(
        PlatformAxis::Macos,
        CapabilityId::TELEMETRY_CPU,
        PlatformSource::Present,
    );
    surface.declare(
        PlatformAxis::Linux,
        CapabilityId::ACCELERATOR_NPU,
        PlatformSource::Absent(CapabilityStatus::RequiresEscalation),
    );

    let entries: Vec<_> = surface.entries().collect();
    assert_eq!(entries.len(), 3);
    assert_eq!(
        entries[0].0,
        (PlatformAxis::Linux, CapabilityId::ACCELERATOR_NPU.clone())
    );
    assert_eq!(
        entries[1].0,
        (PlatformAxis::Linux, CapabilityId::TELEMETRY_CPU.clone())
    );
    assert_eq!(
        entries[2].0,
        (PlatformAxis::Macos, CapabilityId::TELEMETRY_CPU.clone())
    );
    assert_eq!(
        surface.source(PlatformAxis::Linux, &CapabilityId::TELEMETRY_CPU),
        PlatformSource::Absent(CapabilityStatus::Unsupported)
    );
}

/// `declaring` mirrors one platform's registrations and never leaves an
/// expected identity undeclared: unregistered lanes answer the unsupported
/// absence (the M3.2 "Undeclared 归零" shape), a registered-pending lane keeps
/// its explicit absence, and a vendor registration stays visible verbatim.
#[test]
fn declaring_pads_the_expected_surface_and_keeps_registered_lanes() {
    let vendor = CapabilityId::owned("vendor.diagnostic");
    let registered = [
        (CapabilityId::TELEMETRY_CPU, PlatformSource::Present),
        (
            CapabilityId::ACCELERATOR_NPU,
            PlatformSource::absent(CapabilityStatus::Unsupported).expect("admissible absence"),
        ),
        (vendor.clone(), PlatformSource::Present),
    ];
    let surface = PlatformCapabilitySurface::declaring(PlatformAxis::Linux, &registered);

    assert_eq!(
        surface.source(PlatformAxis::Linux, &CapabilityId::TELEMETRY_CPU),
        PlatformSource::Present
    );
    assert_eq!(
        surface.source(PlatformAxis::Linux, &CapabilityId::ACCELERATOR_NPU),
        PlatformSource::Absent(CapabilityStatus::Unsupported)
    );
    assert_eq!(
        surface.source(PlatformAxis::Linux, &CapabilityId::TELEMETRY_PRESSURE),
        PlatformSource::Absent(CapabilityStatus::Unsupported),
        "an unregistered expected identity answers the typed absence, never Undeclared"
    );
    assert_eq!(
        surface.source(PlatformAxis::Linux, &vendor),
        PlatformSource::Present,
        "a vendor registration stays visible"
    );
    assert_eq!(surface.len(), CapabilityId::EXPECTED_SURFACE.len() + 1);
    assert_eq!(surface.present_count(PlatformAxis::Linux), 2);
    assert_eq!(surface.present_count(PlatformAxis::Windows), 0);
    assert!(!surface.is_empty());

    for expected in CapabilityId::EXPECTED_SURFACE {
        assert_ne!(
            surface.source(PlatformAxis::Linux, &expected),
            PlatformSource::Undeclared,
            "an expected identity on the declared platform must never be Undeclared"
        );
        assert_eq!(
            surface.source(PlatformAxis::Windows, &expected),
            PlatformSource::Undeclared,
            "another platform stays undeclared: a surface is one platform's declaration"
        );
    }
}
