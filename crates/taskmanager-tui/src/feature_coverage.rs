//! The TUI shape's feature-coverage declaration (P1 contract carrying capacity).
//!
//! The terminal is a PORTING shape with real, terminal-driven limits. A
//! feature is declared [`CapabilitySupport::Ported`] only when this crate
//! renders the shared shell projection facet that backs it; a deliberate
//! terminal reshaping is declared [`CapabilitySupport::Divergent`] with its
//! driver; a feature with no terminal surface is declared
//! [`CapabilitySupport::Unsupported`]. This is the honest coverage gap, not a
//! target list - claiming `Ported` without a surface would be exactly the
//! silent overclaim the registry exists to prevent. Every feature in the full
//! coverage matrix is declared here - a feature with no terminal surface yet
//! stays a reasoned `Unsupported` gap rather than disappearing from the
//! ledger.
//!
//! ## Honesty boundary
//!
//! The declaration proves DECLARATION discipline - no silent omission, every
//! deliberate difference or absence carries a reason. It does not prove
//! behavioral equivalence; a `Ported` cell claims the intent to match, never
//! the match itself (behavior evidence stays with this shape's tests).

use taskmanager_ui_contract::{
    CapabilitySupport, FeatureCoverageDeclaration, FeatureCoverageEntry, FeatureId, FrontendShape,
};

/// Declare TUI's complete feature-coverage decision set.
#[must_use]
pub fn feature_coverage_declaration() -> FeatureCoverageDeclaration {
    FeatureCoverageDeclaration {
        frontend: FrontendShape::Tui,
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
/// shape states its position. Features with no terminal surface yet stay
/// reasoned `Unsupported` gaps; they are never silently absent.
const fn support(feature: FeatureId) -> CapabilitySupport {
    use CapabilitySupport::{Ported, Unsupported};
    match feature {
        // -- surfaces this shape really renders ---------------------------
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
        | FeatureId::MultiFormatExport => Ported,

        // -- typed absences -----------------------------------------------
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
            reason: "no terminal surface renders this shared projection facet yet",
        },
    }
}

#[cfg(test)]
#[path = "../tests/gui/feature_coverage_tests.rs"]
mod tests;
