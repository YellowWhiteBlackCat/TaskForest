//! Three-axis fold tests (P5 M3.1): exhaustiveness, the mechanical
//! `Missing`/`Unsupported` split, the `NotApplicable` criterion, the
//! `Gated`/`Partial` source combinations, and the counterexample that an
//! inconsistent surface declaration is reported instead of silently folded.
//!
//! M3.1 is deliberately read-only: these tests pin the fold and the report,
//! not a hard gate. The four real frontend declarations are folded by each
//! frontend crate's own tests; here the fixtures are synthetic so the contract
//! stays the only subject under test.

use super::*;
use crate::feature_coverage::FeatureCoverageEntry;

fn declaration(
    frontend: FrontendShape,
    support: impl Fn(FeatureId) -> CapabilitySupport,
) -> FeatureCoverageDeclaration {
    FeatureCoverageDeclaration {
        frontend,
        entries: FeatureId::ALL
            .iter()
            .map(|feature| FeatureCoverageEntry {
                feature: *feature,
                support: support(*feature),
            })
            .collect(),
    }
}

fn all_ported(frontend: FrontendShape) -> FeatureCoverageDeclaration {
    declaration(frontend, |_| CapabilitySupport::Ported)
}

/// A surface that declares every capability any feature requires as
/// `Present` on all three platforms, so the fold's remaining variable is the
/// frontend axis.
fn surface_with_all_required_present() -> PlatformCapabilitySurface {
    let mut surface = PlatformCapabilitySurface::new();
    for feature in FeatureId::ALL {
        for capability in feature.platform_binding().capabilities() {
            for platform in PlatformAxis::ALL {
                surface.declare(platform, capability.clone(), PlatformSource::Present);
            }
        }
    }
    surface
}

/// One declaration folds into exactly one cell per `(feature, platform)` pair,
/// with no missing and no duplicated combination.
#[test]
fn report_covers_every_feature_and_platform_exactly_once() {
    let sources = surface_with_all_required_present();
    for frontend in FrontendShape::ALL {
        let report = feature_platform_report(&all_ported(frontend), &sources);
        assert_eq!(report.len(), FeatureId::ALL.len() * PlatformAxis::ALL.len());
        for feature in FeatureId::ALL {
            for platform in PlatformAxis::ALL {
                let count = report
                    .iter()
                    .filter(|cell| {
                        cell.feature == *feature
                            && cell.frontend == frontend
                            && cell.platform == platform
                    })
                    .count();
                assert_eq!(
                    count,
                    1,
                    "cell ({}, {}, {}) must occur exactly once",
                    feature.id(),
                    frontend.name(),
                    platform.id()
                );
                let Some(cell) = report.cell(*feature, frontend, platform) else {
                    panic!(
                        "cell ({}, {}, {}) is absent from the report",
                        feature.id(),
                        frontend.name(),
                        platform.id()
                    );
                };
                assert_eq!(
                    report.status(*feature, frontend, platform),
                    Some(&cell.status)
                );
            }
        }
    }
}

/// Four frontend declarations fold into the full 900-cell matrix with unique
/// keys - the G1 exhaustiveness shape, without copying any frontend truth into
/// this crate.
#[test]
fn four_declarations_fold_into_nine_hundred_unique_cells() {
    let sources = surface_with_all_required_present();
    let declarations: Vec<_> = FrontendShape::ALL
        .iter()
        .map(|frontend| all_ported(*frontend))
        .collect();
    let ledger = FeaturePlatformLedger::from_declarations(&declarations, &sources);
    assert_eq!(
        ledger.len(),
        FrontendShape::ALL.len() * FeatureId::ALL.len() * PlatformAxis::ALL.len()
    );
    let mut keys: Vec<_> = ledger
        .iter()
        .map(|cell| (cell.feature.id(), cell.frontend.name(), cell.platform.id()))
        .collect();
    let count = keys.len();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), count, "ledger keys must be unique");
    assert!(FeaturePlatformLedger::default().is_empty());
}

/// `Missing` is produced only by the frontend axis and `Unsupported` only by
/// the platform axis; neither is inferred from the other.
#[test]
fn missing_and_unsupported_are_produced_by_different_axes() {
    let sources = surface_with_all_required_present();

    // (1) Frontend omits the feature although every platform declares a source.
    let mut omitted = all_ported(FrontendShape::Gpui);
    omitted
        .entries
        .retain(|entry| entry.feature != FeatureId::NpuTelemetry);
    assert_eq!(
        classify(
            &omitted,
            FeatureId::NpuTelemetry,
            PlatformAxis::Linux,
            &sources
        ),
        FeaturePlatformStatus::Missing(MISSING_DECLARATION_REASON)
    );

    // (2) Frontend declares `Unsupported` - still a frontend-axis gap.
    let unsupported = declaration(FrontendShape::Gpui, |feature| {
        if feature == FeatureId::NpuTelemetry {
            CapabilitySupport::Unsupported {
                reason: "no NPU readout in this shape",
            }
        } else {
            CapabilitySupport::Ported
        }
    });
    assert_eq!(
        classify(
            &unsupported,
            FeatureId::NpuTelemetry,
            PlatformAxis::Linux,
            &sources
        ),
        FeaturePlatformStatus::Missing("no NPU readout in this shape")
    );

    // (3) Frontend declares an entry but the platform registers nothing.
    let empty = PlatformCapabilitySurface::new();
    let status = classify(
        &all_ported(FrontendShape::Gpui),
        FeatureId::NpuTelemetry,
        PlatformAxis::Macos,
        &empty,
    );
    match status {
        FeaturePlatformStatus::Unsupported(unavailable) => {
            assert_eq!(unavailable.capability, Some(CapabilityId::ACCELERATOR_NPU));
            assert_eq!(unavailable.status, CapabilityStatus::Unsupported);
            assert_eq!(unavailable.retry, RetryDisposition::Never);
        }
        other => panic!("expected a platform Unsupported cell, got {other:?}"),
    }

    // (4) A vocabulary gap is `Unregistered`, not a frontend `Missing`.
    let status = classify(
        &all_ported(FrontendShape::Gpui),
        FeatureId::PsiMultiWindowTelemetry,
        PlatformAxis::Linux,
        &sources,
    );
    match status {
        FeaturePlatformStatus::Unregistered(unavailable) => {
            assert_eq!(unavailable.capability, None);
            assert_eq!(unavailable.status, CapabilityStatus::Unsupported);
            assert_eq!(unavailable.retry, RetryDisposition::Never);
        }
        other => panic!("expected an Unregistered vocabulary gap, got {other:?}"),
    }

    // (5) The two axes never collapse: the same empty surface keeps a
    // frontend gap `Missing`.
    assert!(matches!(
        classify(
            &omitted,
            FeatureId::NpuTelemetry,
            PlatformAxis::Macos,
            &empty
        ),
        FeaturePlatformStatus::Missing(_)
    ));
}

/// `NotApplicable` appears only through the binding criterion: a declared
/// frontend entry plus a shared-derivation binding. A feature with an OS
/// source never reports it, and a frontend gap still wins over it.
#[test]
fn not_applicable_requires_the_binding_criterion() {
    let empty = PlatformCapabilitySurface::new();
    let declared = all_ported(FrontendShape::Gpui);
    let binding = FeatureId::MultiFormatExport.platform_binding();
    let PlatformBinding::NotApplicable(note) = binding else {
        panic!("MultiFormatExport must be a shared-derivation NotApplicable binding");
    };
    assert_eq!(
        classify(
            &declared,
            FeatureId::MultiFormatExport,
            PlatformAxis::Windows,
            &empty
        ),
        FeaturePlatformStatus::NotApplicable(note)
    );

    for feature in [
        FeatureId::NpuTelemetry,
        FeatureId::RaplPowerDraw,
        FeatureId::CpuCStateAnalysis,
        FeatureId::LinuxNamespaceAudit,
        FeatureId::SystemdDependencyDag,
        FeatureId::GpuEngineUtilization,
    ] {
        assert!(
            !matches!(
                classify(&declared, feature, PlatformAxis::Macos, &empty),
                FeaturePlatformStatus::NotApplicable(_)
            ),
            "{} has an OS source and must never fold to NotApplicable",
            feature.id()
        );
    }

    let mut omitted = all_ported(FrontendShape::Gpui);
    omitted
        .entries
        .retain(|entry| entry.feature != FeatureId::MultiFormatExport);
    assert!(matches!(
        classify(
            &omitted,
            FeatureId::MultiFormatExport,
            PlatformAxis::Linux,
            &empty
        ),
        FeaturePlatformStatus::Missing(_)
    ));
}

/// `Partial` and `Gated` follow the declared source combinations; the
/// escalation affordance survives as its own state.
#[test]
fn partial_and_gated_follow_the_source_combinations() {
    let declared = all_ported(FrontendShape::Gpui);

    let mut surface = PlatformCapabilitySurface::new();
    surface.declare(
        PlatformAxis::Linux,
        CapabilityId::SERVICE_LOGS,
        PlatformSource::Present,
    );
    surface.declare(
        PlatformAxis::Linux,
        CapabilityId::SERVICE_LOG_STREAM,
        PlatformSource::absent(CapabilityStatus::Unsupported).expect("admissible absence"),
    );
    assert_eq!(
        classify(
            &declared,
            FeatureId::ServiceLogStream,
            PlatformAxis::Linux,
            &surface
        ),
        FeaturePlatformStatus::Partial(PartialCause {
            platform: Some(CapabilityStatus::Unsupported),
            frontend: None,
        })
    );

    let mut surface = PlatformCapabilitySurface::new();
    surface.declare(
        PlatformAxis::Windows,
        CapabilityId::ACCELERATOR_NPU,
        PlatformSource::Absent(CapabilityStatus::RequiresEscalation),
    );
    assert_eq!(
        classify(
            &declared,
            FeatureId::NpuTelemetry,
            PlatformAxis::Windows,
            &surface
        ),
        FeaturePlatformStatus::Gated(CapabilityStatus::RequiresEscalation)
    );

    let sources = surface_with_all_required_present();
    let divergent = declaration(FrontendShape::Gpui, |feature| {
        if feature == FeatureId::GpuAdapterEnumeration {
            CapabilitySupport::Divergent {
                reason: "renders adapters without per-adapter driver text",
            }
        } else {
            CapabilitySupport::Ported
        }
    });
    assert_eq!(
        classify(
            &divergent,
            FeatureId::GpuAdapterEnumeration,
            PlatformAxis::Linux,
            &sources
        ),
        FeaturePlatformStatus::Partial(PartialCause {
            platform: None,
            frontend: Some("renders adapters without per-adapter driver text"),
        })
    );

    let mut surface = PlatformCapabilitySurface::new();
    surface.declare(
        PlatformAxis::Linux,
        CapabilityId::SERVICE_LOGS,
        PlatformSource::Present,
    );
    surface.declare(
        PlatformAxis::Linux,
        CapabilityId::SERVICE_LOG_STREAM,
        PlatformSource::Absent(CapabilityStatus::Unsupported),
    );
    let divergent = declaration(FrontendShape::Gpui, |feature| {
        if feature == FeatureId::ServiceLogStream {
            CapabilitySupport::Divergent {
                reason: "streams without level filtering",
            }
        } else {
            CapabilitySupport::Ported
        }
    });
    assert_eq!(
        classify(
            &divergent,
            FeatureId::ServiceLogStream,
            PlatformAxis::Linux,
            &surface
        ),
        FeaturePlatformStatus::Partial(PartialCause {
            platform: Some(CapabilityStatus::Unsupported),
            frontend: Some("streams without level filtering"),
        })
    );
}

/// Inconsistent inputs are reported, never silently folded into success: a
/// missing declaration is `Unsupported`, a runtime status on an `Absent`
/// source stays visible, and the typed constructor rejects it.
#[test]
fn inconsistent_surface_declarations_are_reported_never_silently_ready() {
    let declared = all_ported(FrontendShape::Gpui);

    // (1) A required capability declared on another platform only.
    let mut surface = PlatformCapabilitySurface::new();
    surface.declare(
        PlatformAxis::Linux,
        CapabilityId::ACCELERATOR_NPU,
        PlatformSource::Present,
    );
    let status = classify(
        &declared,
        FeatureId::NpuTelemetry,
        PlatformAxis::Macos,
        &surface,
    );
    assert!(!matches!(status, FeaturePlatformStatus::Ready));
    match status {
        FeaturePlatformStatus::Unsupported(unavailable) => {
            assert_eq!(unavailable.capability, Some(CapabilityId::ACCELERATOR_NPU));
        }
        other => panic!("an undeclared required source must report Unsupported, got {other:?}"),
    }

    // (2) A surface that claims `Absent` with a runtime status is a
    // contradiction; the report carries it verbatim instead of folding to
    // Ready.
    let mut surface = PlatformCapabilitySurface::new();
    surface.declare(
        PlatformAxis::Linux,
        CapabilityId::ACCELERATOR_NPU,
        PlatformSource::Absent(CapabilityStatus::Available),
    );
    let status = classify(
        &declared,
        FeatureId::NpuTelemetry,
        PlatformAxis::Linux,
        &surface,
    );
    assert!(!matches!(status, FeaturePlatformStatus::Ready));
    match status {
        FeaturePlatformStatus::Unsupported(unavailable) => {
            assert_eq!(unavailable.status, CapabilityStatus::Available);
        }
        other => panic!("an inadmissible absence must stay visible, got {other:?}"),
    }

    // (3) The typed constructor that refuses the same contradiction is owned
    // by `taskmanager-platform-contract`, next to the capability identity; the
    // fold only preserves what the surface declared.
    let error = PlatformSource::absent(CapabilityStatus::Available)
        .expect_err("a runtime status cannot be a static absence");
    assert_eq!(error.status, CapabilityStatus::Available);
}

/// With no platform declarations at all, the ledger reports zero `Ready`
/// cells and exactly the three honest baseline classes: shared derivations
/// stay `NotApplicable`, vocabulary gaps are `Unregistered`, and every
/// capability-backed feature is `Unsupported`. Absence is typed, never
/// assumed present.
#[test]
fn an_empty_surface_yields_no_ready_cells() {
    let declared = all_ported(FrontendShape::Gpui);
    let report = feature_platform_report(&declared, &PlatformCapabilitySurface::new());
    assert_eq!(report.len(), FeatureId::ALL.len() * PlatformAxis::ALL.len());
    let mut ready = 0usize;
    let mut unsupported = 0usize;
    let mut unregistered = 0usize;
    let mut not_applicable = 0usize;
    for cell in report.iter() {
        match cell.status {
            FeaturePlatformStatus::Ready => ready += 1,
            FeaturePlatformStatus::Unsupported(_) => unsupported += 1,
            FeaturePlatformStatus::Unregistered(_) => unregistered += 1,
            FeaturePlatformStatus::NotApplicable(_) => not_applicable += 1,
            FeaturePlatformStatus::Gated(_)
            | FeaturePlatformStatus::Partial(_)
            | FeaturePlatformStatus::Missing(_) => {}
        }
    }
    assert_eq!(ready, 0, "no undeclared source may ever fold to Ready");
    assert_eq!(unsupported, 53 * PlatformAxis::ALL.len());
    assert_eq!(unregistered, 14 * PlatformAxis::ALL.len());
    assert_eq!(not_applicable, 8 * PlatformAxis::ALL.len());
}

/// `Ready` is the static source commitment: it needs a declared entry plus a
/// `Present` source for every requirement, carries no evidence anchor at
/// M3.1, and is reported with a stable machine id.
#[test]
fn ready_is_a_static_commitment_with_the_stable_report_id() {
    let sources = surface_with_all_required_present();
    let declared = all_ported(FrontendShape::Gpui);
    assert_eq!(
        classify(
            &declared,
            FeatureId::GpuAdapterEnumeration,
            PlatformAxis::Windows,
            &sources
        ),
        FeaturePlatformStatus::Ready
    );
    let report = feature_platform_report(&declared, &sources);
    let cell = report
        .cell(
            FeatureId::GpuAdapterEnumeration,
            FrontendShape::Gpui,
            PlatformAxis::Windows,
        )
        .expect("folded cell");
    assert_eq!(cell.status.id(), "ready");
    assert_eq!(cell.evidence, NO_EVIDENCE);
    assert!(!cell.has_evidence());
}

/// The absence retry table is the inverse of the single failure -> status
/// authority on the four absence projections; runtime statuses never claim a
/// retry.
#[test]
fn absence_retry_matches_the_provider_failure_authority() {
    use taskmanager_platform_contract::ProviderFailure;
    for (status, failure) in [
        (CapabilityStatus::Unsupported, ProviderFailure::Unsupported),
        (
            CapabilityStatus::PermissionRequired,
            ProviderFailure::PermissionDenied,
        ),
        (
            CapabilityStatus::RequiresEscalation,
            ProviderFailure::RequiresEscalation,
        ),
        (
            CapabilityStatus::MissingDependency,
            ProviderFailure::MissingDependency,
        ),
    ] {
        assert_eq!(failure.capability_status(), status);
        assert_eq!(absence_retry(status), failure.retry());
    }
    for failure in [
        ProviderFailure::TimedOut,
        ProviderFailure::TemporarilyUnavailable,
        ProviderFailure::Rejected,
        ProviderFailure::IdentityChanged,
        ProviderFailure::ProviderFault,
    ] {
        let status = failure.capability_status();
        assert!(!PlatformSource::is_absence_projection(status));
        assert_eq!(absence_retry(status), RetryDisposition::Never);
    }
}
