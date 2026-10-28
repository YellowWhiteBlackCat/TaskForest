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
        | FeatureId::ProcessEfficiencyMode
        | FeatureId::ProcessAffinityMask
        | FeatureId::ProcessTreeKill
        | FeatureId::ProcessCompoundFilter
        | FeatureId::MemoryBreakdownRssPss
        | FeatureId::MemoryProcessSwapCharge
        | FeatureId::MemoryPageFaults
        | FeatureId::MemoryTransparentHugePages
        | FeatureId::HandleEnumeration
        | FeatureId::HandleTypeClassification
        | FeatureId::DeletedFileHandleWatch
        | FeatureId::ThreadTopologyEnumeration
        | FeatureId::ThreadRunqueueLatency
        | FeatureId::ThreadUninterruptibleSleepDiagnosis
        | FeatureId::ThreadWaitChannelClassification
        | FeatureId::ProcessNetworkThroughput
        | FeatureId::SocketInventory
        | FeatureId::SocketStateTracking
        | FeatureId::SocketRttMetrics
        | FeatureId::ProcessLogicalPhysicalIo
        | FeatureId::DiskDeviceTopology
        | FeatureId::DiskIopsQueueLatency
        | FeatureId::StorageFilesystemMountAudit
        | FeatureId::DiskSmartHealth
        | FeatureId::SwapThroughputRate
        | FeatureId::HardwareTopologyTree
        | FeatureId::CpuCoreFrequency
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
        | FeatureId::SecurityNoNewPrivs
        | FeatureId::SandboxEnvironmentDetection
        | FeatureId::ProcessMasqueradingDetection
        | FeatureId::SystemdDependencyDag
        | FeatureId::ServiceLogStream
        | FeatureId::ServiceFailureDiagnosis
        | FeatureId::ServiceInventoryStatus
        | FeatureId::ServiceUnitFileState
        | FeatureId::ServiceLifecycleControl
        | FeatureId::ServiceRestartChurn
        | FeatureId::ServiceTimerSchedule
        | FeatureId::ServiceSocketActivation
        | FeatureId::StartupImpactAssessment
        | FeatureId::StartupItemControl
        | FeatureId::PsiMultiWindowTelemetry
        | FeatureId::MemoryThrashingHealthScore
        | FeatureId::PressureLoadAverageNormalized
        | FeatureId::PressureSaturationGrade
        | FeatureId::PressureDStateRatio
        | FeatureId::PressureNativeAlertIntegration
        | FeatureId::MultiResolutionRingBuffer
        | FeatureId::MultiFormatExport
        | FeatureId::HistoryLttbDownsampling
        | FeatureId::ProcessAncestorLineage
        | FeatureId::ProcessOomScore
        | FeatureId::CpuInstructionFeatures
        | FeatureId::SmbiosMemorySlots
        | FeatureId::GpuTemperatureReadout
        | FeatureId::GpuPowerDraw
        | FeatureId::GpuCoreClockFrequency
        | FeatureId::BatteryRemainingTimeEstimate
        | FeatureId::FanSpeedTelemetry
        | FeatureId::ProcessFreezeThawControl
        | FeatureId::MemoryCompressionZswap
        | FeatureId::FirmwareSecureBoot
        | FeatureId::GpuThrottleReasons
        | FeatureId::GpuFirmwareApiRuntime
        | FeatureId::ProcessOwnerCredentials
        | FeatureId::ServiceStateMachine
        | FeatureId::ServiceCgroupResourceLimits
        | FeatureId::NetworkRollingBandwidth
        | FeatureId::DirtyWritebackFlow
        | FeatureId::VirtualMemoryFsCapacity
        | FeatureId::StorageTrimDiscardSupport
        | FeatureId::MemoryChannelSymmetry
        | FeatureId::PowerProfilesDaemon
        | FeatureId::CompilerHardeningAudit
        | FeatureId::GlobalFdPoolCapacity
        | FeatureId::FileLocksAudit
        | FeatureId::NetworkSocketReset
        | FeatureId::NetworkTrafficShaping
        | FeatureId::ProcessIoPriority
        | FeatureId::GpuResetTdrLogs
        | FeatureId::MandatoryAccessControlLabels
        | FeatureId::UserServiceAggregation
        | FeatureId::OffCpuBlockingAnalysis
        | FeatureId::ProcessTwoPhaseTermination
        | FeatureId::ProcessCgroupResourceLimiting
        | FeatureId::DiskReadOnlyFailureAlert
        | FeatureId::EnergyImpactScore
        | FeatureId::CpuEnergyPerformancePreference
        | FeatureId::ServiceUnitFileDropIns
        | FeatureId::NamedSemaphoresInventory
        | FeatureId::PipeGraphVisualization
        | FeatureId::ProcessTrimWorkingSet
        | FeatureId::ProcessCoreDump
        | FeatureId::ProcessAutoRestartRule
        | FeatureId::ProcessCustomLabel
        | FeatureId::GpuCodecUtilization
        | FeatureId::GpuProcessClients
        | FeatureId::NpuInferenceTelemetry
        | FeatureId::GpuLinkTopology
        | FeatureId::StorageSpaceDepletionCountdown
        | FeatureId::StorageIoStallDiagnosis
        | FeatureId::StorageTopAccessedFiles
        | FeatureId::RaidPoolTopology
        | FeatureId::ProcessWakeupFrequency
        | FeatureId::SleepInhibitLocks
        | FeatureId::DisplayBacklightPower
        | FeatureId::UsbDeviceRuntimePm
        | FeatureId::SessionEnergyCostCalculation
        | FeatureId::LandlockSandboxAudit
        | FeatureId::BinarySignatureVerification
        | FeatureId::SoftirqSaturationAnalysis
        | FeatureId::FilesystemLockLatency
        | FeatureId::MemoryCompactionStall
        | FeatureId::OomImminentWarning
        | FeatureId::IpcBufferSaturation
        | FeatureId::DnsReverseLookup
        | FeatureId::GeoIpAsnLookup
        | FeatureId::SocketConnectionAuditFeed
        | FeatureId::MaliciousConnectionDetection
        | FeatureId::WindowsIntegrityLevels
        | FeatureId::MacosTccPermissions
        | FeatureId::PrivilegeAnomalyDetection
        | FeatureId::CodeInjectionDetection
        | FeatureId::ApplicationProtocolInference
        | FeatureId::OrphanSocketLeakDetection
        | FeatureId::ProcessNumaMemoryPolicy
        | FeatureId::NumaThrashingAlert
        | FeatureId::PcieLinkSpeedDegradation
        | FeatureId::CpuThermalBoostStatus
        | FeatureId::HeterogeneousInterconnectArchitecture
        | FeatureId::CpuInterruptAffinityBalance
        | FeatureId::DbusTrafficMonitoring
        | FeatureId::PosixSysvMessageQueues
        | FeatureId::WindowsNamedPipes
        | FeatureId::KernelEventNotifiers
        | FeatureId::SharedMemorySecurityAudit
        | FeatureId::DbusActivationLatency
        | FeatureId::OrphanIpcReclamation
        | FeatureId::TopIpcThroughputProducers
        | FeatureId::PipePeerPidLookup
        | FeatureId::UdsPeerAssociation
        | FeatureId::DeviceNodeHandleInspection
        | FeatureId::EmergencyCloseHandleAction
        | FeatureId::HandleLeakTrendAnalysis
        | FeatureId::WindowsAccessTokenAudit
        | FeatureId::MemfdSealsAudit
        | FeatureId::MountNamespaceRootLookup
        | FeatureId::EpollSubscriptionHotspots
        | FeatureId::ThreadCallstackSampling
        | FeatureId::ThreadFutexContention
        | FeatureId::ThreadStackLayoutGuard
        | FeatureId::ThreadSchedulingNiceAffinity
        | FeatureId::ThreadPoolWorkerHeuristics
        | FeatureId::ThreadSuspendResumeControl
        | FeatureId::ThreadZombieDetection
        | FeatureId::MemorySmapsRollupTiering
        | FeatureId::SharedMemorySegmentCrossAudit
        | FeatureId::MemoryDirtyPagePressure
        | FeatureId::UserspaceAllocatorInference
        | FeatureId::WindowsCommitKernelPools
        | FeatureId::ThreadLocalStorageAudit
        | FeatureId::ThreadChurnWarning
        | FeatureId::ThreadCpuSliceHeatmap
        | FeatureId::MacosMemoryClassification
        | FeatureId::MemoryReadOnlyStringProbing
        | FeatureId::VmaSnapshotDiff
        | FeatureId::CodeSectionIntegrityAudit
        | FeatureId::ProcessFileActivityTrace
        | FeatureId::SocketLifecycleAuditStream
        | FeatureId::DynamicAllocationProfiling
        | FeatureId::MutexContentionProfiler
        | FeatureId::PageFaultCallchainBacktrace
        | FeatureId::ContextSwitchTriggerAnalysis
        | FeatureId::FlightRecordingModeHook
        | FeatureId::TraceDataStandardExport
        | FeatureId::DynamicTracingCapabilityGate
        | FeatureId::GhostProcessesCapture
        | FeatureId::SystemSnapshotDiff
        | FeatureId::BlackboxCrashPreservation
        | FeatureId::OfflineSessionRecording
        | FeatureId::EventTimelineBookmarks
        | FeatureId::LeaderboardRankEvolution
        | FeatureId::BenchmarkAbComparison
        | FeatureId::VariableSpeedReplayEngine
        | FeatureId::EnterprisePrivacyRedaction
        | FeatureId::HandleFdLimitSaturation => Ported,

        // -- typed absences -----------------------------------------------
        FeatureId::HandleReversePathSearch => Unsupported {
            reason: "no system-wide handle search surface is wired in Iced",
        },
        FeatureId::SocketQueueBacklog => Unsupported {
            reason: "the Iced connection rows render addresses and state without \
                     send/receive queue depth",
        },
        FeatureId::ProcessGpuAttribution => Unsupported {
            reason: "no per-process GPU attribution is wired in the Iced process \
                     surface",
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
