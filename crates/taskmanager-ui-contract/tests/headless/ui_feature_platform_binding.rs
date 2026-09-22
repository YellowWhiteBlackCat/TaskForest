//! Static platform-binding tests (P5 layer A): totality, honesty and the
//! reverse example that keeps Linux-only gaps visible.
//!
//! `platform_binding` is an exhaustive `const fn` with no wildcard arm, so a
//! newly registered feature fails to compile until it declares its platform
//! position. These tests pin the runtime half: every variant carries a
//! non-empty note, every `Requires` set is non-empty and duplicate-free, the
//! binding shapes are counted, and "Linux-only today" is never legitimized as
//! `NotApplicable`.

use super::*;
use crate::feature_coverage::FeatureId;

/// Every feature declares exactly one binding shape; `Requires` sets are
/// non-empty, duplicate-free, and expose no note; the two classification
/// variants carry a non-empty note that `note()` returns.
#[test]
fn platform_binding_is_total_typed_and_explained() {
    assert_eq!(FeatureId::ALL.len(), 75);
    let mut requires = 0usize;
    let mut not_applicable = 0usize;
    let mut not_in_vocabulary = 0usize;
    for feature in FeatureId::ALL {
        let binding = feature.platform_binding();
        match binding {
            PlatformBinding::Requires(capabilities) => {
                requires += 1;
                assert!(
                    !capabilities.is_empty(),
                    "feature {} declares an empty Requires binding",
                    feature.id()
                );
                let mut ids: Vec<_> = capabilities.iter().map(|id| id.as_str()).collect();
                let count = ids.len();
                ids.sort_unstable();
                ids.dedup();
                assert_eq!(
                    ids.len(),
                    count,
                    "feature {} declares a capability twice",
                    feature.id()
                );
                assert!(ids.iter().all(|id| !id.is_empty()));
                assert_eq!(binding.note(), None);
            }
            PlatformBinding::NotApplicable(reason) => {
                not_applicable += 1;
                assert!(
                    !reason.is_empty(),
                    "feature {} must state why no platform source applies",
                    feature.id()
                );
                assert_eq!(binding.note(), Some(reason));
            }
            PlatformBinding::NotInVocabulary(reason) => {
                not_in_vocabulary += 1;
                assert!(
                    !reason.is_empty(),
                    "feature {} must state its vocabulary gap",
                    feature.id()
                );
                assert_eq!(binding.note(), Some(reason));
            }
        }
        assert_eq!(
            binding.capabilities().is_empty(),
            !matches!(binding, PlatformBinding::Requires(_)),
            "only Requires exposes required capabilities for {}",
            feature.id()
        );
    }
    assert_eq!(
        (requires, not_applicable, not_in_vocabulary),
        (53, 8, 14),
        "binding-shape counts are part of the contract; changing one is a conscious registry change"
    );
    assert_eq!(
        requires + not_applicable + not_in_vocabulary,
        FeatureId::ALL.len()
    );
}

/// The forwarded capability expectation surface is exactly the pinned set:
/// `Requires` may only name capability identities that really exist in
/// `taskmanager-platform-contract`, and dropping or adding one is visible.
#[test]
fn required_capabilities_are_a_pinned_product_expectation_surface() {
    let mut required: Vec<_> = FeatureId::ALL
        .iter()
        .flat_map(|feature| feature.platform_binding().capabilities().iter())
        .map(|capability| capability.as_str())
        .collect();
    required.sort_unstable();
    required.dedup();
    assert_eq!(
        required,
        [
            "accelerator.npu",
            "hardware.inventory",
            "hardware.power-supplies",
            "history.process-events",
            "ipc.posix",
            "ipc.sysv",
            "network.socket-inventory",
            "process.affinity",
            "process.affinity.control",
            "process.control",
            "process.insights.gpu",
            "process.insights.isolation",
            "process.insights.network",
            "process.insights.open_files",
            "process.insights.resources",
            "process.insights.threads",
            "process.list",
            "sensors",
            "services",
            "services.control",
            "services.dependencies",
            "services.logs",
            "services.logs.stream",
            "storage.smart",
            "telemetry.cpu",
            "telemetry.cpu.package_power",
            "telemetry.gpu",
            "telemetry.gpu.engines",
            "telemetry.host",
            "telemetry.memory",
            "telemetry.storage",
        ]
    );
}

/// The blueprint's reverse example: a feature whose source exists on one
/// platform and is merely unimplemented on the others must stay `Requires`,
/// never `NotApplicable`. `NotApplicable` is reserved for shared derivations.
#[test]
fn shared_derivations_are_not_applicable_and_platform_gaps_are_not() {
    for feature in [
        FeatureId::MemoryLeakTrend,
        FeatureId::MemoryThrashingHealthScore,
        FeatureId::UseBottleneckAttribution,
        FeatureId::MultiResolutionRingBuffer,
        FeatureId::MultiFormatExport,
        FeatureId::TimeTravelScrubber,
        FeatureId::TelemetryPercentileAggregation,
        FeatureId::HistoryChartImageExport,
    ] {
        assert!(
            matches!(
                feature.platform_binding(),
                PlatformBinding::NotApplicable(reason) if !reason.is_empty()
            ),
            "{} is a shared derivation with no OS source step",
            feature.id()
        );
    }
    for feature in [
        FeatureId::NpuTelemetry,
        FeatureId::RaplPowerDraw,
        FeatureId::CpuCStateAnalysis,
        FeatureId::LinuxNamespaceAudit,
        FeatureId::PosixCapabilitiesAudit,
        FeatureId::SeccompFilterAudit,
        FeatureId::SystemdDependencyDag,
        FeatureId::ServiceLogStream,
        FeatureId::GpuEngineUtilization,
        FeatureId::PsiMultiWindowTelemetry,
        FeatureId::SocketRttMetrics,
        FeatureId::HandleFdLimitSaturation,
        FeatureId::ThermalThrottleEvents,
        FeatureId::UnresponsiveAppDetection,
        FeatureId::SharedMemorySegments,
        FeatureId::ProcessEventTrace,
        FeatureId::CpuFlameGraph,
    ] {
        assert!(
            !matches!(
                feature.platform_binding(),
                PlatformBinding::NotApplicable(_)
            ),
            "{} must keep its platform gap visible, never legitimize it as NotApplicable",
            feature.id()
        );
    }
}
