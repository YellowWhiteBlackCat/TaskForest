//! The Iced shape's feature-coverage declaration (P1 contract carrying capacity).
//!
//! Iced is a PORTING shape: it renders the shared shell projections the GPUI
//! reference surfaces, mostly through its own immutable view layer. A feature
//! is declared [`CapabilitySupport::Ported`] only when this crate really
//! renders the shared projection facet that backs it; a feature with no Iced
//! surface is declared [`CapabilitySupport::Unsupported`] with its driver.
//! This is the honest coverage gap, not a target list: claiming `Ported`
//! without a surface would be exactly the silent overclaim the registry
//! exists to prevent. Every feature in the full coverage matrix is declared
//! here - a feature with no Iced surface yet stays a reasoned `Unsupported`
//! gap rather than disappearing from the ledger.
//!
//! ## Honesty boundary
//!
//! The declaration proves DECLARATION discipline - no silent omission, every
//! deliberate absence carries a reason. It does not prove behavioral
//! equivalence; a `Ported` cell claims the intent to match, never the match
//! itself (behavior evidence stays with this shape's tests).

use taskmanager_ui_contract::{
    CapabilitySupport, FeatureCoverageDeclaration, FeatureCoverageEntry, FeatureId, FrontendShape,
};

/// Declare Iced's complete feature-coverage decision set.
#[must_use]
pub fn feature_coverage_declaration() -> FeatureCoverageDeclaration {
    FeatureCoverageDeclaration {
        frontend: FrontendShape::Iced,
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
/// shape states its position. Features with no Iced surface yet stay reasoned
/// `Unsupported` gaps; they are never silently absent from the declaration.
const fn support(feature: FeatureId) -> CapabilitySupport {
    use CapabilitySupport::{Ported, Unsupported};
    match feature {
        // -- surfaces this shape really renders ---------------------------
        FeatureId::ProcessSchedulerPolicy
        | FeatureId::ProcessPriorityMapping
        | FeatureId::ProcessAffinityMask
        | FeatureId::ProcessTreeKill
        | FeatureId::MemoryBreakdownRssPss
        | FeatureId::MemoryPageFaults
        | FeatureId::MemoryTransparentHugePages
        | FeatureId::HandleEnumeration
        | FeatureId::ThreadTopologyEnumeration
        | FeatureId::ThreadRunqueueLatency
        | FeatureId::ThreadUninterruptibleSleepDiagnosis
        | FeatureId::ThreadWaitChannelClassification
        | FeatureId::ProcessNetworkThroughput
        | FeatureId::SocketInventory
        | FeatureId::SocketRttMetrics
        | FeatureId::ProcessLogicalPhysicalIo
        | FeatureId::DiskDeviceTopology
        | FeatureId::DiskIopsQueueLatency
        | FeatureId::DiskSmartHealth
        | FeatureId::SwapThroughputRate
        | FeatureId::HardwareTopologyTree
        | FeatureId::CpuCacheTopology
        | FeatureId::CpuHeterogeneousCoreClass
        | FeatureId::GpuAdapterEnumeration
        | FeatureId::NpuTelemetry
        | FeatureId::GpuEngineUtilization
        | FeatureId::GpuMemoryReadout
        | FeatureId::RaplPowerDraw
        | FeatureId::ThermalZoneSensors
        | FeatureId::ThermalThrottleEvents
        | FeatureId::CpuCStateAnalysis
        | FeatureId::BatteryPowerInventory
        | FeatureId::LinuxNamespaceAudit
        | FeatureId::PosixCapabilitiesAudit
        | FeatureId::SeccompFilterAudit
        | FeatureId::SandboxEnvironmentDetection
        | FeatureId::ProcessMasqueradingDetection
        | FeatureId::SystemdDependencyDag
        | FeatureId::ServiceLogStream
        | FeatureId::ServiceFailureDiagnosis
        | FeatureId::ServiceInventoryStatus
        | FeatureId::ServiceLifecycleControl
        | FeatureId::PsiMultiWindowTelemetry
        | FeatureId::MemoryThrashingHealthScore
        | FeatureId::PressureLoadAverageNormalized
        | FeatureId::MultiResolutionRingBuffer
        | FeatureId::MultiFormatExport => Ported,

        // -- typed absences -----------------------------------------------
        FeatureId::HandleTypeClassification => Unsupported {
            reason: "the Iced open-file facet renders fd and target only, with \
                     no descriptor type classifier",
        },
        FeatureId::DeletedFileHandleWatch => Unsupported {
            reason: "the Iced open-file facet does not surface deleted-but-held \
                     descriptors",
        },
        FeatureId::HandleFdLimitSaturation => Unsupported {
            reason: "the Iced open-file facet renders descriptors without the \
                     RLIMIT_NOFILE saturation context",
        },
        FeatureId::HandleReversePathSearch => Unsupported {
            reason: "no system-wide handle search surface is wired in Iced",
        },
        FeatureId::SocketQueueBacklog => Unsupported {
            reason: "the Iced connection rows render addresses and state without \
                     send/receive queue depth",
        },
        FeatureId::CpuCoreFrequency => Unsupported {
            reason: "the Iced CPU surface renders the package-level frequency \
                     without the per-core readout",
        },
        FeatureId::ProcessGpuAttribution => Unsupported {
            reason: "no per-process GPU attribution is wired in the Iced process \
                     surface",
        },
        FeatureId::ProcessAncestorLineage => Unsupported {
            reason: "the Iced process surface renders the row table without the \
                     ancestor lineage chain",
        },
        FeatureId::MemoryVmaMap
        | FeatureId::MemoryLeakTrend
        | FeatureId::ThreadContextSwitchRates
        | FeatureId::ListeningPortTopology
        | FeatureId::NumaMemoryDistribution
        | FeatureId::UseBottleneckAttribution
        | FeatureId::UnresponsiveAppDetection
        | FeatureId::DbusServiceTopology
        | FeatureId::DbusIntrospection
        | FeatureId::PipeDeadlockDiagnosis
        | FeatureId::SharedMemorySegments
        | FeatureId::UdsPeerTopology
        | FeatureId::PmuCounterAbstraction
        | FeatureId::SyscallDistributionProfiling
        | FeatureId::SlowSyscallTrap
        | FeatureId::ProcessEventTrace
        | FeatureId::CpuFlameGraph
        | FeatureId::TimeTravelScrubber
        | FeatureId::TelemetryPercentileAggregation
        | FeatureId::HistoryChartImageExport => Unsupported {
            reason: "no Iced surface renders this shared projection facet yet",
        },
    }
}

#[cfg(test)]
#[path = "../tests/gui/feature_coverage_tests.rs"]
mod tests;
