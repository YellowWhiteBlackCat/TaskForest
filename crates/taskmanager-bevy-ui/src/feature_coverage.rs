//! The Bevy UI shape's feature-coverage declaration (P1 contract carrying capacity).
//!
//! Bevy is a PORTING shape that renders a subset of the shared shell
//! projections: a feature is declared [`CapabilitySupport::Ported`] only when
//! this crate really renders the shared projection facet that backs it. A
//! feature with no Bevy surface - including the deep telemetry facets the
//! scene layer has not yet wired - is declared
//! [`CapabilitySupport::Unsupported`] with its driver. This is the honest
//! coverage gap, not a target list: claiming `Ported` without a surface would
//! be exactly the silent overclaim the registry exists to prevent. Every
//! feature in the full coverage matrix is declared here - a feature with no
//! Bevy surface yet stays a reasoned `Unsupported` gap rather than
//! disappearing from the ledger.
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

/// Declare the Bevy shape's complete feature-coverage decision set.
#[must_use]
pub fn feature_coverage_declaration() -> FeatureCoverageDeclaration {
    FeatureCoverageDeclaration {
        frontend: FrontendShape::Bevy,
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
/// shape states its position. Features with no Bevy surface yet stay reasoned
/// `Unsupported` gaps; they are never silently absent.
const fn support(feature: FeatureId) -> CapabilitySupport {
    use CapabilitySupport::{Divergent, Ported, Unsupported};
    match feature {
        // -- surfaces this shape really renders ---------------------------
        FeatureId::ProcessSchedulerPolicy
        | FeatureId::ProcessPriorityMapping
        | FeatureId::ProcessAffinityMask
        | FeatureId::ProcessTreeKill
        | FeatureId::MemoryBreakdownRssPss
        | FeatureId::MemoryProcessSwapCharge
        | FeatureId::MemoryPageFaults
        | FeatureId::MemoryTransparentHugePages
        | FeatureId::HandleEnumeration
        | FeatureId::HandleTypeClassification
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
        | FeatureId::GpuAdapterEnumeration
        | FeatureId::NpuTelemetry
        | FeatureId::GpuEngineUtilization
        | FeatureId::GpuMemoryReadout
        | FeatureId::ThermalThrottleEvents
        | FeatureId::CpuCStateAnalysis
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
        | FeatureId::MultiResolutionRingBuffer => Ported,

        // -- deliberate scene reduction -----------------------------------
        FeatureId::ThermalZoneSensors => Divergent {
            reason: "Bevy surfaces the CPU temperature readout without the \
                     thermal-zone source distinction",
        },
        FeatureId::BatteryPowerInventory => Divergent {
            reason: "the Bevy battery block renders charge and power without \
                     the voltage/health/cycle detail",
        },

        // -- typed absences -----------------------------------------------
        FeatureId::DeletedFileHandleWatch => Unsupported {
            reason: "the Bevy open-file summary renders fd/kind/target but does \
                     not surface deleted-but-held descriptors",
        },
        FeatureId::HandleFdLimitSaturation => Unsupported {
            reason: "the Bevy open-file summary renders descriptors without the \
                     RLIMIT_NOFILE saturation context",
        },
        FeatureId::HandleReversePathSearch => Unsupported {
            reason: "no system-wide handle search surface is wired in the Bevy \
                     shape",
        },
        FeatureId::SocketQueueBacklog => Unsupported {
            reason: "the Bevy connection summary renders state without \
                     send/receive queue depth",
        },
        FeatureId::CpuHeterogeneousCoreClass => Unsupported {
            reason: "no heterogeneous core-class breakdown is wired in the Bevy \
                     performance surface",
        },
        FeatureId::CpuCoreFrequency => Unsupported {
            reason: "no per-core clock frequency readout is wired in the Bevy \
                     performance surface",
        },
        FeatureId::ProcessGpuAttribution => Unsupported {
            reason: "no per-process GPU attribution is wired in the Bevy shape",
        },
        FeatureId::RaplPowerDraw => Unsupported {
            reason: "no RAPL package-power readout is wired in the Bevy \
                     performance surface",
        },
        FeatureId::ProcessAncestorLineage => Unsupported {
            reason: "the Bevy process surface renders the tree without an \
                     ancestor lineage chain",
        },
        FeatureId::MultiFormatExport => Unsupported {
            reason: "no structured multi-format export engine is wired in the \
                     Bevy shape",
        },
        FeatureId::CpuCacheTopology => Unsupported {
            reason: "no multi-level cache capacity readout is wired in the Bevy \
                     performance surface",
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
            reason: "no Bevy surface renders this shared projection facet yet",
        },
    }
}

#[cfg(test)]
#[path = "../tests/headless/feature_coverage.rs"]
mod tests;
