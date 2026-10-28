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
        | FeatureId::CpuCacheTopology
        | FeatureId::CpuHeterogeneousCoreClass
        | FeatureId::CpuCoreFrequency
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
        | FeatureId::HandleFdLimitSaturation => Reference,

        // -- roadmap items the reference surface has not delivered yet -----
        FeatureId::MemoryVmaMap
        | FeatureId::MemoryLeakTrend
        | FeatureId::HandleReversePathSearch
        | FeatureId::ThreadContextSwitchRates
        | FeatureId::ListeningPortTopology
        | FeatureId::SocketQueueBacklog
        | FeatureId::NumaMemoryDistribution
        | FeatureId::ProcessGpuAttribution
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
            reason: REFERENCE_UNDELIVERED,
        },
    }
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_feature_coverage_tests.rs"]
mod tests;
