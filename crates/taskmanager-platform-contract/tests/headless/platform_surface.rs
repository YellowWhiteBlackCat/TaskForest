//! Static registration-surface tests: the typed absence constructor, the
//! undeclared default, and deterministic declaration order.
//!
//! These pin the owner half of layer B: a static absence can only carry one of
//! the four projections `ProviderFailure::capability_status` produces, an
//! undeclared pair is a typed visible fact instead of a default success, and a
//! redeclaration never duplicates an entry.

use super::*;

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
        CapabilityStatus::Degraded(taskmanager_core::FailureKind::TimedOut),
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
