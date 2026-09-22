//! The GPUI shape's feature-coverage declaration (P1 contract carrying capacity).
//!
//! GPUI is the REFERENCE shape (GPUI-05): the parallel feature surfaces (Iced,
//! TUI, Bevy) port the semantics this product owns. A feature the reference
//! surface really renders is declared [`CapabilitySupport::Reference`] here.
//!
//! The coverage matrix is the FULL roadmap parity matrix, so it also carries
//! features the reference surface has not delivered yet. For those GPUI
//! declares [`CapabilitySupport::Unsupported`] with the stated driver
//! ("reference surface not yet delivered") - the honest gap marker. GPUI may
//! not declare `Ported`/`Divergent`/`Native` (the shape that owns the
//! semantics cannot port, diverge from, or delegate them), but `Unsupported`
//! keeps an undelivered item visible instead of fabricating a `Reference`
//! claim for it.
//!
//! The declaration proves DECLARATION discipline only - every registered
//! feature is explicitly delivered or explicitly refused, never silently
//! omitted. It does not claim that the reference behavior is finished; each
//! cell is backed by the shape's own behavior tests and evidence route.

use taskmanager_ui_contract::{
    CapabilitySupport, FeatureCoverageDeclaration, FeatureCoverageEntry, FeatureId, FrontendShape,
};

/// The reference shape has not delivered this surface yet.
const REFERENCE_UNDELIVERED: &str = "reference surface not yet delivered";

/// Declare the GPUI reference surface for every registered feature.
#[must_use]
pub fn feature_coverage_declaration() -> FeatureCoverageDeclaration {
    FeatureCoverageDeclaration {
        frontend: FrontendShape::Gpui,
        entries: FeatureId::ALL
            .iter()
            .map(|feature| FeatureCoverageEntry {
                feature: *feature,
                support: support(*feature),
            })
            .collect(),
    }
}

/// One explicit support decision per feature. The match is exhaustive with no
/// wildcard arm, so a newly registered `FeatureId` fails to compile until this
/// shape states its position.
const fn support(feature: FeatureId) -> CapabilitySupport {
    use CapabilitySupport::{Reference, Unsupported};
    match feature {
        // -- surfaces the reference shape really renders -------------------
        FeatureId::ProcessSchedulerPolicy
        | FeatureId::ProcessPriorityMapping
        | FeatureId::ProcessAffinityMask
        | FeatureId::MemoryBreakdownRssPss
        | FeatureId::HandleEnumeration
        | FeatureId::HandleTypeClassification
        | FeatureId::DeletedFileHandleWatch
        | FeatureId::ThreadTopologyEnumeration
        | FeatureId::ThreadRunqueueLatency
        | FeatureId::ProcessNetworkThroughput
        | FeatureId::SocketInventory
        | FeatureId::ProcessLogicalPhysicalIo
        | FeatureId::DiskDeviceTopology
        | FeatureId::DiskIopsQueueLatency
        | FeatureId::HardwareTopologyTree
        | FeatureId::CpuCacheTopology
        | FeatureId::GpuAdapterEnumeration
        | FeatureId::NpuTelemetry
        | FeatureId::GpuEngineUtilization
        | FeatureId::RaplPowerDraw
        | FeatureId::ThermalZoneSensors
        | FeatureId::CpuCStateAnalysis
        | FeatureId::LinuxNamespaceAudit
        | FeatureId::PosixCapabilitiesAudit
        | FeatureId::SeccompFilterAudit
        | FeatureId::SystemdDependencyDag
        | FeatureId::ServiceLogStream
        | FeatureId::ServiceFailureDiagnosis
        | FeatureId::PsiMultiWindowTelemetry
        | FeatureId::MemoryThrashingHealthScore
        | FeatureId::MultiResolutionRingBuffer
        | FeatureId::MultiFormatExport => Reference,

        // -- roadmap items the reference surface has not delivered yet -----
        FeatureId::MemoryVmaMap
        | FeatureId::MemoryLeakTrend
        | FeatureId::ThreadContextSwitchRates
        | FeatureId::ListeningPortTopology
        | FeatureId::NumaMemoryDistribution
        | FeatureId::UseBottleneckAttribution
        | FeatureId::DbusServiceTopology
        | FeatureId::DbusIntrospection
        | FeatureId::PipeDeadlockDiagnosis
        | FeatureId::PmuCounterAbstraction
        | FeatureId::SyscallDistributionProfiling
        | FeatureId::SlowSyscallTrap
        | FeatureId::TimeTravelScrubber => Unsupported {
            reason: REFERENCE_UNDELIVERED,
        },
    }
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_feature_coverage_tests.rs"]
mod tests;
