//! Feature identifiers and area mappings.

use super::{FeatureArea, FeatureId};

impl FeatureId {
    /// Stable machine name for gates and diagnostics.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::ProcessSchedulerPolicy => "process.scheduler-policy",
            Self::ProcessPriorityMapping => "process.priority-mapping",
            Self::ProcessEfficiencyMode => "process.efficiency-mode",
            Self::ProcessAffinityMask => "process.affinity-mask",
            Self::ProcessTreeKill => "process.tree-kill",
            Self::ProcessTwoPhaseTermination => "process.two-phase-termination",
            Self::ProcessCgroupResourceLimiting => "process.cgroup-resource-limiting",
            Self::ProcessOomScore => "process.oom-score",
            Self::ProcessTrimWorkingSet => "process.trim-working-set",
            Self::ProcessCoreDump => "process.core-dump",
            Self::ProcessAutoRestartRule => "process.auto-restart-rule",
            Self::ProcessFreezeThawControl => "process.freeze-thaw-control",
            Self::ProcessCustomLabel => "process.custom-labels",
            Self::ProcessAncestorLineage => "process.ancestor-lineage",
            Self::ProcessCompoundFilter => "process.compound-filter",
            Self::MemoryBreakdownRssPss => "memory.breakdown-rss-pss",
            Self::MemorySmapsRollupTiering => "memory.smaps-rollup-tiering",
            Self::MemoryProcessSwapCharge => "memory.process-swap-charge",
            Self::MemoryVmaMap => "memory.vma-map",
            Self::SharedMemorySegmentCrossAudit => "memory.shared-segments-audit",
            Self::MemoryDirtyPagePressure => "memory.dirty-page-pressure",
            Self::MemoryLeakTrend => "memory.leak-trend",
            Self::MemoryPageFaults => "memory.page-faults",
            Self::MemoryCompressionZswap => "memory.compression-zswap",
            Self::MemoryTransparentHugePages => "memory.transparent-huge-pages",
            Self::UserspaceAllocatorInference => "memory.allocator-inference",
            Self::WindowsCommitKernelPools => "memory.windows-commit-pools",
            Self::MacosMemoryClassification => "memory.macos-classification",
            Self::MemoryReadOnlyStringProbing => "memory.readonly-string-probing",
            Self::VmaSnapshotDiff => "memory.vma-snapshot-diff",
            Self::CodeSectionIntegrityAudit => "memory.code-integrity-audit",
            Self::HandleEnumeration => "handles.enumeration",
            Self::HandleTypeClassification => "handles.type-classification",
            Self::FileLocksAudit => "handles.file-locks-audit",
            Self::DeletedFileHandleWatch => "handles.deleted-file-watch",
            Self::HandleFdLimitSaturation => "handles.fd-limit-saturation",
            Self::PipePeerPidLookup => "handles.pipe-peer-pid",
            Self::UdsPeerAssociation => "handles.uds-peer-association",
            Self::DeviceNodeHandleInspection => "handles.device-node-handles",
            Self::HandleReversePathSearch => "handles.reverse-path-search",
            Self::EmergencyCloseHandleAction => "handles.emergency-close-action",
            Self::HandleLeakTrendAnalysis => "handles.leak-trend-analysis",
            Self::WindowsAccessTokenAudit => "handles.windows-token-audit",
            Self::MemfdSealsAudit => "handles.memfd-seals-audit",
            Self::MountNamespaceRootLookup => "handles.mount-namespace-root",
            Self::EpollSubscriptionHotspots => "handles.epoll-subscription-hotspots",
            Self::ThreadTopologyEnumeration => "threads.topology",
            Self::ThreadRunqueueLatency => "threads.runqueue-latency",
            Self::ThreadContextSwitchRates => "threads.context-switch-rates",
            Self::ThreadUninterruptibleSleepDiagnosis => "threads.uninterruptible-sleep",
            Self::ThreadWaitChannelClassification => "threads.wait-channel-classification",
            Self::ThreadCallstackSampling => "threads.callstack-sampling",
            Self::ThreadFutexContention => "threads.futex-contention",
            Self::ThreadStackLayoutGuard => "threads.stack-layout-guard",
            Self::ThreadSchedulingNiceAffinity => "threads.nice-affinity",
            Self::ThreadPoolWorkerHeuristics => "threads.pool-worker-heuristics",
            Self::ThreadSuspendResumeControl => "threads.suspend-resume",
            Self::ThreadZombieDetection => "threads.zombie-detection",
            Self::ThreadLocalStorageAudit => "threads.tls-audit",
            Self::ThreadChurnWarning => "threads.churn-warning",
            Self::ThreadCpuSliceHeatmap => "threads.cpu-slice",
            Self::ProcessNetworkThroughput => "network.process-throughput",
            Self::SocketInventory => "network.socket-inventory",
            Self::SocketStateTracking => "network.socket-state-tracking",
            Self::ListeningPortTopology => "network.listening-port-topology",
            Self::SocketRttMetrics => "network.socket-rtt",
            Self::SocketQueueBacklog => "network.socket-queue-backlog",
            Self::DnsReverseLookup => "network.dns-reverse-lookup",
            Self::GeoIpAsnLookup => "network.geoip-asn-lookup",
            Self::SocketConnectionAuditFeed => "network.connection-audit-feed",
            Self::MaliciousConnectionDetection => "network.threat-detection",
            Self::NetworkSocketReset => "network.socket-reset",
            Self::NetworkTrafficShaping => "network.traffic-shaping",
            Self::ApplicationProtocolInference => "network.protocol-inference",
            Self::OrphanSocketLeakDetection => "network.orphan-socket-leak",
            Self::NetworkRollingBandwidth => "network.rolling-bandwidth",
            Self::ProcessLogicalPhysicalIo => "storage.process-logical-physical-io",
            Self::DiskDeviceTopology => "storage.device-topology",
            Self::DiskIopsQueueLatency => "storage.iops-queue-latency",
            Self::ProcessIoPriority => "storage.io-priority",
            Self::StorageFilesystemMountAudit => "storage.filesystem-mount-audit",
            Self::DirtyWritebackFlow => "storage.dirty-writeback-flow",
            Self::DiskSmartHealth => "storage.smart-health",
            Self::StorageSpaceDepletionCountdown => "storage.space-depletion-countdown",
            Self::StorageIoStallDiagnosis => "storage.io-stall-diagnosis",
            Self::SwapThroughputRate => "storage.swap-throughput",
            Self::StorageTopAccessedFiles => "storage.top-accessed-files",
            Self::VirtualMemoryFsCapacity => "storage.virtual-fs-capacity",
            Self::StorageTrimDiscardSupport => "storage.trim-discard-support",
            Self::RaidPoolTopology => "storage.raid-pool-topology",
            Self::DiskReadOnlyFailureAlert => "storage.disk-read-only-failure-alert",
            Self::HardwareTopologyTree => "hardware.topology-tree",
            Self::CpuHeterogeneousCoreClass => "hardware.heterogeneous-cores",
            Self::CpuCacheTopology => "hardware.cpu-cache-topology",
            Self::NumaMemoryDistribution => "hardware.numa-memory-distribution",
            Self::ProcessNumaMemoryPolicy => "hardware.numa-memory-policy",
            Self::NumaThrashingAlert => "hardware.numa-thrashing-alert",
            Self::PcieLinkSpeedDegradation => "hardware.pcie-link-degradation",
            Self::CpuCoreFrequency => "hardware.core-frequency",
            Self::CpuThermalBoostStatus => "hardware.cpu-thermal-boost",
            Self::CpuInstructionFeatures => "hardware.instruction-features",
            Self::SmbiosMemorySlots => "hardware.smbios-memory-slots",
            Self::MemoryChannelSymmetry => "hardware.memory-channel-symmetry",
            Self::HeterogeneousInterconnectArchitecture => "hardware.heterogeneous-interconnect",
            Self::FirmwareSecureBoot => "hardware.firmware-secure-boot",
            Self::CpuInterruptAffinityBalance => "hardware.cpu-interrupt-balance",
            Self::GpuAdapterEnumeration => "gpu.adapter-enumeration",
            Self::NpuTelemetry => "accelerator.npu-telemetry",
            Self::GpuEngineUtilization => "gpu.engine-utilization",
            Self::GpuMemoryReadout => "gpu.memory-usage",
            Self::ProcessGpuAttribution => "gpu.process-attribution",
            Self::GpuTemperatureReadout => "gpu.temperature-readout",
            Self::GpuPowerDraw => "gpu.power-draw",
            Self::GpuCoreClockFrequency => "gpu.core-clock-frequency",
            Self::GpuThrottleReasons => "gpu.perfcap-reasons",
            Self::GpuCodecUtilization => "gpu.codec-utilization",
            Self::GpuProcessClients => "gpu.process-clients",
            Self::NpuInferenceTelemetry => "accelerator.npu-inference",
            Self::GpuLinkTopology => "gpu.link-topology",
            Self::GpuFirmwareApiRuntime => "gpu.driver-firmware-api",
            Self::GpuResetTdrLogs => "gpu.reset-tdr-logs",
            Self::RaplPowerDraw => "power.rapl-draw",
            Self::EnergyImpactScore => "power.energy-impact-score",
            Self::BatteryRemainingTimeEstimate => "power.battery-time-estimate",
            Self::ThermalZoneSensors => "power.thermal-zones",
            Self::CpuCStateAnalysis => "power.cstate-analysis",
            Self::BatteryPowerInventory => "power.battery-inventory",
            Self::ThermalThrottleEvents => "power.thermal-throttle-events",
            Self::CpuEnergyPerformancePreference => "power.cpu-energy-performance-preference",
            Self::FanSpeedTelemetry => "power.fan-speed-telemetry",
            Self::ProcessWakeupFrequency => "power.process-wakeup-frequency",
            Self::PowerProfilesDaemon => "power.profiles-integration",
            Self::SleepInhibitLocks => "power.sleep-inhibit-locks",
            Self::DisplayBacklightPower => "power.display-backlight-power",
            Self::UsbDeviceRuntimePm => "power.usb-runtime-pm",
            Self::SessionEnergyCostCalculation => "power.session-energy-cost",
            Self::LinuxNamespaceAudit => "security.namespace-audit",
            Self::SandboxEnvironmentDetection => "security.sandbox-detection",
            Self::PosixCapabilitiesAudit => "security.posix-capabilities",
            Self::MandatoryAccessControlLabels => "security.mac-labels",
            Self::SeccompFilterAudit => "security.seccomp-filter",
            Self::LandlockSandboxAudit => "security.landlock-rules",
            Self::SecurityNoNewPrivs => "security.no-new-privs",
            Self::ProcessOwnerCredentials => "security.user-credentials",
            Self::BinarySignatureVerification => "security.binary-signatures",
            Self::CompilerHardeningAudit => "security.compiler-hardening",
            Self::WindowsIntegrityLevels => "security.windows-integrity-levels",
            Self::MacosTccPermissions => "security.macos-tcc-permissions",
            Self::PrivilegeAnomalyDetection => "security.privilege-anomaly",
            Self::ProcessMasqueradingDetection => "security.masquerading-detection",
            Self::CodeInjectionDetection => "security.code-injection-detection",
            Self::ServiceInventoryStatus => "services.inventory",
            Self::UserServiceAggregation => "services.user-units-aggregation",
            Self::ServiceStateMachine => "services.state-machine",
            Self::ServiceUnitFileState => "services.unit-file-state",
            Self::ServiceLifecycleControl => "services.lifecycle-control",
            Self::ServiceCgroupResourceLimits => "services.cgroup-resource-limits",
            Self::SystemdDependencyDag => "services.dependency-dag",
            Self::ServiceFailureDiagnosis => "services.failure-diagnosis",
            Self::ServiceLogStream => "services.log-stream",
            Self::ServiceRestartChurn => "services.restart-churn",
            Self::ServiceTimerSchedule => "services.timer-schedule",
            Self::ServiceSocketActivation => "services.socket-activation",
            Self::ServiceUnitFileDropIns => "services.unit-file-drop-ins",
            Self::StartupImpactAssessment => "startup.impact-assessment",
            Self::StartupItemControl => "startup.item-control",
            Self::PsiMultiWindowTelemetry => "pressure.psi-multi-window",
            Self::PressureSaturationGrade => "pressure.saturation-grade",
            Self::MemoryThrashingHealthScore => "pressure.memory-thrashing-health",
            Self::PressureLoadAverageNormalized => "pressure.load-average-normalized",
            Self::SoftirqSaturationAnalysis => "pressure.softirq-saturation",
            Self::PressureDStateRatio => "pressure.dstate-task-ratio",
            Self::FilesystemLockLatency => "pressure.filesystem-lock-latency",
            Self::MemoryCompactionStall => "pressure.memory-compaction-stall",
            Self::OomImminentWarning => "pressure.oom-imminent-warning",
            Self::GlobalFdPoolCapacity => "pressure.global-fd-pool",
            Self::IpcBufferSaturation => "pressure.ipc-buffer-saturation",
            Self::UseBottleneckAttribution => "pressure.use-attribution",
            Self::UnresponsiveAppDetection => "pressure.unresponsive-apps",
            Self::PressureNativeAlertIntegration => "pressure.native-alert-integration",
            Self::DbusServiceTopology => "ipc.dbus-service-topology",
            Self::DbusIntrospection => "ipc.dbus-introspection",
            Self::DbusTrafficMonitoring => "ipc.dbus-traffic-monitoring",
            Self::SharedMemorySegments => "ipc.shared-memory-segments",
            Self::PosixSysvMessageQueues => "ipc.posix-sysv-mqueue",
            Self::NamedSemaphoresInventory => "ipc.named-semaphores-inventory",
            Self::PipeGraphVisualization => "ipc.pipe-graph-visualization",
            Self::UdsPeerTopology => "ipc.uds-peer-topology",
            Self::WindowsNamedPipes => "ipc.windows-named-pipes",
            Self::PipeDeadlockDiagnosis => "ipc.pipe-deadlock",
            Self::KernelEventNotifiers => "ipc.kernel-eventfd-signalfd",
            Self::SharedMemorySecurityAudit => "ipc.shm-security-audit",
            Self::DbusActivationLatency => "ipc.dbus-activation-latency",
            Self::OrphanIpcReclamation => "ipc.orphan-ipc-reclamation",
            Self::TopIpcThroughputProducers => "ipc.top-ipc-producers",
            Self::PmuCounterAbstraction => "tracing.pmu-counters",
            Self::SyscallDistributionProfiling => "tracing.syscall-distribution",
            Self::SlowSyscallTrap => "tracing.slow-syscall-trap",
            Self::OffCpuBlockingAnalysis => "tracing.off-cpu-profiling",
            Self::ProcessFileActivityTrace => "tracing.file-activity-trace",
            Self::SocketLifecycleAuditStream => "tracing.socket-lifecycle-stream",
            Self::ProcessEventTrace => "tracing.process-event-trace",
            Self::CpuFlameGraph => "tracing.cpu-flame-graph",
            Self::DynamicAllocationProfiling => "tracing.allocation-profiling",
            Self::MutexContentionProfiler => "tracing.mutex-contention-profiler",
            Self::PageFaultCallchainBacktrace => "tracing.page-fault-backtrace",
            Self::ContextSwitchTriggerAnalysis => "tracing.context-switch-triggers",
            Self::FlightRecordingModeHook => "tracing.flight-recording-hook",
            Self::TraceDataStandardExport => "tracing.standard-export",
            Self::DynamicTracingCapabilityGate => "tracing.capability-gate",
            Self::MultiResolutionRingBuffer => "history.multi-resolution-ring",
            Self::TimeTravelScrubber => "history.time-travel-scrubber",
            Self::GhostProcessesCapture => "history.ghost-processes-capture",
            Self::SystemSnapshotDiff => "history.snapshot-diff",
            Self::BlackboxCrashPreservation => "history.blackbox-preservation",
            Self::OfflineSessionRecording => "history.offline-session-recording",
            Self::MultiFormatExport => "history.multi-format-export",
            Self::TelemetryPercentileAggregation => "history.percentile-aggregation",
            Self::HistoryLttbDownsampling => "history.lttb-downsampling",
            Self::EventTimelineBookmarks => "history.event-bookmarks",
            Self::LeaderboardRankEvolution => "history.leaderboard-rank-evolution",
            Self::BenchmarkAbComparison => "history.benchmark-ab-diff",
            Self::HistoryChartImageExport => "history.chart-image-export",
            Self::VariableSpeedReplayEngine => "history.variable-speed-replay",
            Self::EnterprisePrivacyRedaction => "history.enterprise-privacy-redaction",
        }
    }

    /// The blueprint area this feature belongs to.
    #[must_use]
    pub const fn area(self) -> FeatureArea {
        match self {
            Self::ProcessSchedulerPolicy
            | Self::ProcessPriorityMapping
            | Self::ProcessEfficiencyMode
            | Self::ProcessAffinityMask
            | Self::ProcessTreeKill
            | Self::ProcessTwoPhaseTermination
            | Self::ProcessCgroupResourceLimiting
            | Self::ProcessOomScore
            | Self::ProcessTrimWorkingSet
            | Self::ProcessCoreDump
            | Self::ProcessAutoRestartRule
            | Self::ProcessFreezeThawControl
            | Self::ProcessCustomLabel
            | Self::ProcessAncestorLineage
            | Self::ProcessCompoundFilter => FeatureArea::ProcessLifecycle,
            Self::MemoryBreakdownRssPss
            | Self::MemorySmapsRollupTiering
            | Self::MemoryProcessSwapCharge
            | Self::MemoryVmaMap
            | Self::SharedMemorySegmentCrossAudit
            | Self::MemoryDirtyPagePressure
            | Self::MemoryLeakTrend
            | Self::MemoryPageFaults
            | Self::MemoryCompressionZswap
            | Self::MemoryTransparentHugePages
            | Self::UserspaceAllocatorInference
            | Self::WindowsCommitKernelPools
            | Self::MacosMemoryClassification
            | Self::MemoryReadOnlyStringProbing
            | Self::VmaSnapshotDiff
            | Self::CodeSectionIntegrityAudit => FeatureArea::MemoryForensics,
            Self::HandleEnumeration
            | Self::HandleTypeClassification
            | Self::FileLocksAudit
            | Self::DeletedFileHandleWatch
            | Self::HandleFdLimitSaturation
            | Self::PipePeerPidLookup
            | Self::UdsPeerAssociation
            | Self::DeviceNodeHandleInspection
            | Self::HandleReversePathSearch
            | Self::EmergencyCloseHandleAction
            | Self::HandleLeakTrendAnalysis
            | Self::WindowsAccessTokenAudit
            | Self::MemfdSealsAudit
            | Self::MountNamespaceRootLookup
            | Self::EpollSubscriptionHotspots => FeatureArea::HandleDescriptorAudit,
            Self::ThreadTopologyEnumeration
            | Self::ThreadRunqueueLatency
            | Self::ThreadContextSwitchRates
            | Self::ThreadUninterruptibleSleepDiagnosis
            | Self::ThreadWaitChannelClassification
            | Self::ThreadCallstackSampling
            | Self::ThreadFutexContention
            | Self::ThreadStackLayoutGuard
            | Self::ThreadSchedulingNiceAffinity
            | Self::ThreadPoolWorkerHeuristics
            | Self::ThreadSuspendResumeControl
            | Self::ThreadZombieDetection
            | Self::ThreadLocalStorageAudit
            | Self::ThreadChurnWarning
            | Self::ThreadCpuSliceHeatmap => FeatureArea::ThreadTopology,
            Self::ProcessNetworkThroughput
            | Self::SocketInventory
            | Self::SocketStateTracking
            | Self::ListeningPortTopology
            | Self::SocketRttMetrics
            | Self::SocketQueueBacklog
            | Self::DnsReverseLookup
            | Self::GeoIpAsnLookup
            | Self::SocketConnectionAuditFeed
            | Self::MaliciousConnectionDetection
            | Self::NetworkSocketReset
            | Self::NetworkTrafficShaping
            | Self::ApplicationProtocolInference
            | Self::OrphanSocketLeakDetection
            | Self::NetworkRollingBandwidth => FeatureArea::NetworkSockets,
            Self::ProcessLogicalPhysicalIo
            | Self::DiskDeviceTopology
            | Self::DiskIopsQueueLatency
            | Self::ProcessIoPriority
            | Self::StorageFilesystemMountAudit
            | Self::DirtyWritebackFlow
            | Self::DiskSmartHealth
            | Self::StorageSpaceDepletionCountdown
            | Self::StorageIoStallDiagnosis
            | Self::SwapThroughputRate
            | Self::StorageTopAccessedFiles
            | Self::VirtualMemoryFsCapacity
            | Self::StorageTrimDiscardSupport
            | Self::RaidPoolTopology
            | Self::DiskReadOnlyFailureAlert => FeatureArea::StorageFilesystemIo,
            Self::HardwareTopologyTree
            | Self::CpuHeterogeneousCoreClass
            | Self::CpuCacheTopology
            | Self::NumaMemoryDistribution
            | Self::ProcessNumaMemoryPolicy
            | Self::NumaThrashingAlert
            | Self::PcieLinkSpeedDegradation
            | Self::CpuCoreFrequency
            | Self::CpuThermalBoostStatus
            | Self::CpuInstructionFeatures
            | Self::SmbiosMemorySlots
            | Self::MemoryChannelSymmetry
            | Self::HeterogeneousInterconnectArchitecture
            | Self::FirmwareSecureBoot
            | Self::CpuInterruptAffinityBalance => FeatureArea::HardwareTopologyNuma,
            Self::GpuAdapterEnumeration
            | Self::NpuTelemetry
            | Self::GpuEngineUtilization
            | Self::GpuMemoryReadout
            | Self::ProcessGpuAttribution
            | Self::GpuTemperatureReadout
            | Self::GpuPowerDraw
            | Self::GpuCoreClockFrequency
            | Self::GpuThrottleReasons
            | Self::GpuCodecUtilization
            | Self::GpuProcessClients
            | Self::NpuInferenceTelemetry
            | Self::GpuLinkTopology
            | Self::GpuFirmwareApiRuntime
            | Self::GpuResetTdrLogs => FeatureArea::AcceleratorTelemetry,
            Self::RaplPowerDraw
            | Self::EnergyImpactScore
            | Self::BatteryRemainingTimeEstimate
            | Self::ThermalZoneSensors
            | Self::CpuCStateAnalysis
            | Self::BatteryPowerInventory
            | Self::ThermalThrottleEvents
            | Self::CpuEnergyPerformancePreference
            | Self::FanSpeedTelemetry
            | Self::ProcessWakeupFrequency
            | Self::PowerProfilesDaemon
            | Self::SleepInhibitLocks
            | Self::DisplayBacklightPower
            | Self::UsbDeviceRuntimePm
            | Self::SessionEnergyCostCalculation => FeatureArea::PowerThermal,
            Self::LinuxNamespaceAudit
            | Self::SandboxEnvironmentDetection
            | Self::PosixCapabilitiesAudit
            | Self::MandatoryAccessControlLabels
            | Self::SeccompFilterAudit
            | Self::LandlockSandboxAudit
            | Self::SecurityNoNewPrivs
            | Self::ProcessOwnerCredentials
            | Self::BinarySignatureVerification
            | Self::CompilerHardeningAudit
            | Self::WindowsIntegrityLevels
            | Self::MacosTccPermissions
            | Self::PrivilegeAnomalyDetection
            | Self::ProcessMasqueradingDetection
            | Self::CodeInjectionDetection => FeatureArea::SecurityIsolation,
            Self::ServiceInventoryStatus
            | Self::UserServiceAggregation
            | Self::ServiceStateMachine
            | Self::ServiceUnitFileState
            | Self::ServiceLifecycleControl
            | Self::ServiceCgroupResourceLimits
            | Self::SystemdDependencyDag
            | Self::ServiceFailureDiagnosis
            | Self::ServiceLogStream
            | Self::ServiceRestartChurn
            | Self::ServiceTimerSchedule
            | Self::ServiceSocketActivation
            | Self::ServiceUnitFileDropIns
            | Self::StartupImpactAssessment
            | Self::StartupItemControl => FeatureArea::ServicesInit,
            Self::PsiMultiWindowTelemetry
            | Self::PressureSaturationGrade
            | Self::MemoryThrashingHealthScore
            | Self::PressureLoadAverageNormalized
            | Self::SoftirqSaturationAnalysis
            | Self::PressureDStateRatio
            | Self::FilesystemLockLatency
            | Self::MemoryCompactionStall
            | Self::OomImminentWarning
            | Self::GlobalFdPoolCapacity
            | Self::IpcBufferSaturation
            | Self::UseBottleneckAttribution
            | Self::UnresponsiveAppDetection
            | Self::PressureNativeAlertIntegration => FeatureArea::PressureSaturation,
            Self::DbusServiceTopology
            | Self::DbusIntrospection
            | Self::DbusTrafficMonitoring
            | Self::SharedMemorySegments
            | Self::PosixSysvMessageQueues
            | Self::NamedSemaphoresInventory
            | Self::PipeGraphVisualization
            | Self::UdsPeerTopology
            | Self::WindowsNamedPipes
            | Self::PipeDeadlockDiagnosis
            | Self::KernelEventNotifiers
            | Self::SharedMemorySecurityAudit
            | Self::DbusActivationLatency
            | Self::OrphanIpcReclamation
            | Self::TopIpcThroughputProducers => FeatureArea::IpcDbus,
            Self::PmuCounterAbstraction
            | Self::SyscallDistributionProfiling
            | Self::SlowSyscallTrap
            | Self::OffCpuBlockingAnalysis
            | Self::ProcessFileActivityTrace
            | Self::SocketLifecycleAuditStream
            | Self::ProcessEventTrace
            | Self::CpuFlameGraph
            | Self::DynamicAllocationProfiling
            | Self::MutexContentionProfiler
            | Self::PageFaultCallchainBacktrace
            | Self::ContextSwitchTriggerAnalysis
            | Self::FlightRecordingModeHook
            | Self::TraceDataStandardExport
            | Self::DynamicTracingCapabilityGate => FeatureArea::DynamicTracing,
            Self::MultiResolutionRingBuffer
            | Self::TimeTravelScrubber
            | Self::GhostProcessesCapture
            | Self::SystemSnapshotDiff
            | Self::BlackboxCrashPreservation
            | Self::OfflineSessionRecording
            | Self::MultiFormatExport
            | Self::TelemetryPercentileAggregation
            | Self::HistoryLttbDownsampling
            | Self::EventTimelineBookmarks
            | Self::LeaderboardRankEvolution
            | Self::BenchmarkAbComparison
            | Self::HistoryChartImageExport
            | Self::VariableSpeedReplayEngine
            | Self::EnterprisePrivacyRedaction => FeatureArea::HistoryTimeTravel,
        }
    }
}
