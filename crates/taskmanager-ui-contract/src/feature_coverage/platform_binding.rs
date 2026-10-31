//! Static feature -> platform binding: layer A of the three-axis ledger.
//!
//! [`FeatureId::platform_binding`] is the SINGLE AUTHORITY for what the
//! platform axis should provide for one feature. It is an exhaustive `const
//! fn` match with no wildcard arm, so a newly registered [`FeatureId`] fails
//! to compile until it declares its platform position - the same anti-drift
//! mechanism as [`FeatureId::semantic_spec`](super::FeatureId::semantic_spec).
//!
//! The declaration says what the feature needs, never what a platform has:
//! per-platform source facts live in layer B (a [`PlatformCapabilitySurface`]
//! supplied by the caller; M3.2 moves the concrete declarations next to the
//! real providers) and layer C (the runtime catalog, used only by platform
//! conformance). A binding therefore stays stable while platform delivery
//! matures; only the folded cell moves.
//!
//! [`PlatformCapabilitySurface`]: super::PlatformCapabilitySurface
//!
//! ## Honesty boundary
//!
//! - `Requires` names capability identities that already exist in
//!   `taskforest-contract`; it never invents a provider claim.
//! - `NotInVocabulary` is the explicit M3.1 baseline for a feature whose
//!   operating-system facts have no capability identity yet. It is visible
//!   debt (the ledger reports `Unregistered`), never a silent omission, and
//!   M3.3 lowers its count.
//! - `NotApplicable` is reserved for a delivery definition that is fully
//!   satisfied by shared projections or derivation and has no operating-system
//!   source step of its own. A platform gap that merely has not been
//!   implemented yet must NOT be legitimized as `NotApplicable`; it is
//!   `Requires` plus an absent/undeclared platform source.

use taskforest_contract::CapabilityId;

/// Product-expected capabilities that no feature owns, declared explicitly
/// with the reason they are not a feature-ledger item.
///
/// This is the second acceptance path of the bidirectional binding gate (G5):
/// an expected capability is accounted for when a feature binds it (`Requires`)
/// OR when it appears here with a reason. A capability that is merely not bound
/// yet is NOT feature-independent - it belongs to the accepted-debt census in
/// the gate baseline, so a gap is never legitimized as a design decision.
///
/// The list is empty today: every unbound expected identity is current
/// feature-axis debt (the 75-item registry has not expanded to it yet), not a
/// permanent product decision. Entries land here only when the owning product
/// surface really is independent of every registered feature.
pub const FEATURE_INDEPENDENT_CAPABILITIES: &[(CapabilityId, &str)] = &[];

/// What the product statically expects the platform axis to provide for one
/// feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformBinding {
    /// The delivery definition is fully satisfied by shared projections or
    /// derivation: there is no operating-system source step of its own and no
    /// source waiting to be implemented. The note states why no platform
    /// source exists.
    NotApplicable(&'static str),
    /// The delivery needs the named capability identities; each platform's
    /// registered source decides the folded cell.
    Requires(&'static [CapabilityId]),
    /// The delivery needs operating-system facts that have no capability
    /// identity yet. The note states the vocabulary gap.
    NotInVocabulary(&'static str),
}

impl PlatformBinding {
    /// The declaration note for `NotApplicable`/`NotInVocabulary`; `None` for
    /// `Requires`.
    #[must_use]
    pub const fn note(self) -> Option<&'static str> {
        match self {
            Self::NotApplicable(reason) | Self::NotInVocabulary(reason) => Some(reason),
            Self::Requires(_) => None,
        }
    }

    /// The required capability identities; empty for the classification-only
    /// variants.
    #[must_use]
    pub const fn capabilities(self) -> &'static [CapabilityId] {
        match self {
            Self::Requires(capabilities) => capabilities,
            Self::NotApplicable(_) | Self::NotInVocabulary(_) => &[],
        }
    }
}

// -- capability lanes -------------------------------------------------------
//
// One static slice per distinct requirement set. The statics keep
// `platform_binding` a `const fn`: a slice literal of `CapabilityId` values
// cannot be promoted inside a const fn (the type has a destructor), while a
// reference to a static initializer can.

static PROCESS_POLICY_LANES: &[CapabilityId] =
    &[CapabilityId::PROCESS_LIST, CapabilityId::PROCESS_CONTROL];
static PROCESS_TREE_LANES: &[CapabilityId] =
    &[CapabilityId::PROCESS_LIST, CapabilityId::PROCESS_CONTROL];
static PROCESS_CONTROL_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_CONTROL];
static PROCESS_RESOURCE_LIMIT_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_RESOURCE_CONTROL];
static PROCESS_AFFINITY_LANES: &[CapabilityId] = &[
    CapabilityId::PROCESS_AFFINITY,
    CapabilityId::PROCESS_AFFINITY_CONTROL,
];
static PROCESS_ROW_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_LIST];
static PROCESS_OOM_LANES: &[CapabilityId] =
    &[CapabilityId::PROCESS_LIST, CapabilityId::PROCESS_OOM_SCORE];
static PROCESS_THREAD_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_INSIGHTS_THREADS];
static PROCESS_HANDLE_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_INSIGHTS_OPEN_FILES];
static PROCESS_RESOURCE_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_INSIGHTS_RESOURCES];
static PROCESS_NETWORK_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_INSIGHTS_NETWORK];
static SOCKET_INVENTORY_LANES: &[CapabilityId] = &[CapabilityId::NETWORK_SOCKET_INVENTORY];
static PROCESS_GPU_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_INSIGHTS_GPU];
static PROCESS_ISOLATION_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_INSIGHTS_ISOLATION];
static IPC_SHARED_MEMORY_LANES: &[CapabilityId] =
    &[CapabilityId::IPC_POSIX, CapabilityId::IPC_SYSV];
static PROCESS_EVENT_LANES: &[CapabilityId] = &[CapabilityId::HISTORY_PROCESS_EVENTS];
static MEMORY_TELEMETRY_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_MEMORY];
static MEMORY_COMPRESSION_LANES: &[CapabilityId] = &[
    CapabilityId::TELEMETRY_MEMORY,
    CapabilityId::MEMORY_COMPRESSION,
];
static HOST_LOAD_LANES: &[CapabilityId] =
    &[CapabilityId::TELEMETRY_HOST, CapabilityId::TELEMETRY_CPU];
static STORAGE_TELEMETRY_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_STORAGE];
static STORAGE_HEALTH_LANES: &[CapabilityId] = &[CapabilityId::STORAGE_HEALTH];
static STORAGE_SMART_LANES: &[CapabilityId] = &[CapabilityId::SMART];
static MEMORY_VMA_LANES: &[CapabilityId] = &[CapabilityId::MEMORY_VMA_MAP];
static THREAD_CONTEXT_SWITCH_LANES: &[CapabilityId] = &[CapabilityId::THREADS_CONTEXT_SWITCH];
static CPU_THROTTLE_THERMAL_LANES: &[CapabilityId] = &[
    CapabilityId::TELEMETRY_CPU_THROTTLE,
    CapabilityId::TELEMETRY_CPU_MSR,
];
static PRESSURE_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_PRESSURE];
static DESKTOP_RESPONSIVENESS_LANES: &[CapabilityId] = &[CapabilityId::DESKTOP_RESPONSIVENESS];
static DESKTOP_NOTIFICATION_LANES: &[CapabilityId] = &[CapabilityId::DESKTOP_NOTIFY];
static DBUS_LANES: &[CapabilityId] = &[CapabilityId::IPC_DBUS];
static PIPE_GRAPH_LANES: &[CapabilityId] = &[CapabilityId::IPC_PIPE_GRAPH];
static PMU_PROFILING_LANES: &[CapabilityId] = &[CapabilityId::PROFILING_PMU];
static SYSCALL_PROFILING_LANES: &[CapabilityId] = &[CapabilityId::PROFILING_SYSCALLS];
static NUMA_TOPOLOGY_LANES: &[CapabilityId] = &[CapabilityId::NUMA_TOPOLOGY];
static HARDWARE_TOPOLOGY_LANES: &[CapabilityId] = &[
    CapabilityId::HARDWARE_INVENTORY,
    CapabilityId::TELEMETRY_CPU,
];
static SMBIOS_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_MEMORY_SMBIOS];
static CPU_METRIC_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_CPU];
static GPU_TELEMETRY_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_GPU];
static GPU_ENGINE_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_GPU_ENGINES];
static NPU_INVENTORY_LANES: &[CapabilityId] = &[CapabilityId::ACCELERATOR_NPU];
static CPU_PACKAGE_POWER_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_CPU_PACKAGE_POWER];
static THERMAL_SENSOR_LANES: &[CapabilityId] = &[CapabilityId::SENSORS];
static POWER_SUPPLY_LANES: &[CapabilityId] = &[CapabilityId::POWER_SUPPLIES];
static SERVICE_GRAPH_LANES: &[CapabilityId] = &[CapabilityId::SERVICE_DEPENDENCIES];
static SERVICE_INVENTORY_LANES: &[CapabilityId] = &[CapabilityId::SERVICES];
static SERVICE_LIFECYCLE_LANES: &[CapabilityId] =
    &[CapabilityId::SERVICES, CapabilityId::SERVICE_CONTROL];
static SERVICE_LOG_LANES: &[CapabilityId] =
    &[CapabilityId::SERVICE_LOGS, CapabilityId::SERVICE_LOG_STREAM];
static SERVICE_DIAGNOSIS_LANES: &[CapabilityId] =
    &[CapabilityId::SERVICES, CapabilityId::SERVICE_LOGS];
static STARTUP_EVIDENCE_LANES: &[CapabilityId] = &[CapabilityId::STARTUP_EVIDENCE];
static STARTUP_CONTROL_LANES: &[CapabilityId] = &[CapabilityId::STARTUP_CONTROL];
static NETWORK_TELEMETRY_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_NETWORK];
static STORAGE_WRITEBACK_LANES: &[CapabilityId] = &[CapabilityId::STORAGE_WRITEBACK];
static POWER_PROFILE_LANES: &[CapabilityId] = &[CapabilityId::POWER_PROFILES];
static FILESYSTEM_FD_LANES: &[CapabilityId] = &[CapabilityId::FILESYSTEM_FD_LIMITS];
static FILESYSTEM_LOCKS_LANES: &[CapabilityId] = &[CapabilityId::FILESYSTEM_FILE_LOCKS];
static SOCKET_CONTROL_LANES: &[CapabilityId] = &[CapabilityId::NETWORK_SOCKET_CONTROL];
static TRAFFIC_CONTROL_LANES: &[CapabilityId] = &[CapabilityId::NETWORK_TRAFFIC_CONTROL];
static STORAGE_IO_PRIORITY_LANES: &[CapabilityId] = &[CapabilityId::STORAGE_IO_PRIORITY];
static GPU_HANG_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_GPU_HANG];
static OFF_CPU_LANES: &[CapabilityId] = &[CapabilityId::PROFILING_OFF_CPU];

impl super::FeatureId {
    /// The product's static platform expectation for this feature.
    ///
    /// Exhaustive `const fn` with no wildcard: a new [`FeatureId`] cannot
    /// compile until it declares its platform binding. Read-only report
    /// functions ([`super::feature_platform_report`]) fold this declaration
    /// against each platform's registered source surface; this function never
    /// inspects the host, a provider, or the runtime catalog.
    ///
    /// [`FeatureId`]: super::FeatureId
    #[must_use]
    pub const fn platform_binding(self) -> PlatformBinding {
        use PlatformBinding::{NotApplicable, NotInVocabulary, Requires};
        match self {
            // -- Area 1: process lifecycle ---------------------------------
            // The observed class/tier rides the shared process projection and
            // the typed change action rides the process control lane.
            Self::ProcessSchedulerPolicy
            | Self::ProcessPriorityMapping
            | Self::ProcessEfficiencyMode => Requires(PROCESS_POLICY_LANES),
            // The whole-tree action needs the list row identity (parent
            // linkage) plus the control lane; the descendant set itself is a
            // shared projection fold.
            Self::ProcessTreeKill => Requires(PROCESS_TREE_LANES),
            Self::ProcessTwoPhaseTermination
            | Self::ProcessTrimWorkingSet
            | Self::ProcessCoreDump => Requires(PROCESS_CONTROL_LANES),
            Self::ProcessCgroupResourceLimiting => Requires(PROCESS_RESOURCE_LIMIT_LANES),
            Self::ProcessAutoRestartRule => Requires(PROCESS_EVENT_LANES),
            // The ancestry chain is a fold over the process rows' parent
            // identity and start-time evidence.
            Self::ProcessAncestorLineage => Requires(PROCESS_ROW_LANES),
            Self::ProcessOomScore => Requires(PROCESS_OOM_LANES),
            Self::ProcessFreezeThawControl => Requires(PROCESS_POLICY_LANES),
            Self::ProcessCompoundFilter => NotApplicable(
                "the compound filter is evaluated over the shared process projection; \
                 it has no operating-system source step of its own",
            ),
            Self::ProcessCustomLabel => NotApplicable(
                "custom process labels and highlight aliases are an application-level configuration over \
                 the shared process projection; they have no operating-system source step of its own",
            ),
            Self::ProcessAffinityMask => Requires(PROCESS_AFFINITY_LANES),
            // -- Area 2: memory forensics ----------------------------------
            // RSS/PSS ride the per-process projection with field-level typed
            // availability (never a zero); the fault counters and the
            // anonymous-huge-page charge are row scalars in the same family.
            Self::MemoryBreakdownRssPss
            | Self::MemoryPageFaults
            | Self::MemoryTransparentHugePages
            | Self::MemoryProcessSwapCharge
            | Self::MemorySmapsRollupTiering
            | Self::MemoryDirtyPagePressure
            | Self::MacosMemoryClassification
            | Self::CodeSectionIntegrityAudit => Requires(PROCESS_ROW_LANES),
            Self::MemoryCompressionZswap => Requires(MEMORY_COMPRESSION_LANES),
            Self::MemoryVmaMap => Requires(MEMORY_VMA_LANES),
            Self::SharedMemorySegmentCrossAudit => Requires(PROCESS_HANDLE_LANES),
            Self::WindowsCommitKernelPools => Requires(PROCESS_RESOURCE_LANES),
            Self::UserspaceAllocatorInference => NotApplicable(
                "allocator runtime state is inferred heuristically from virtual memory area \
                 fragmentation patterns in shared projections",
            ),
            Self::MemoryReadOnlyStringProbing => NotApplicable(
                "safe memory string probing operates on inspected memory snapshots \
                 without additional platform data sources",
            ),
            Self::VmaSnapshotDiff => NotApplicable(
                "virtual memory area differentials are computed across two historical address \
                 space snapshots in the shared projection",
            ),
            Self::MemoryLeakTrend => NotApplicable(
                "the leak trend is derived from the shared history projection; \
                 the memory samples it folds are owned by process telemetry",
            ),
            // -- Area 3: handles and descriptors ---------------------------
            Self::HandleEnumeration
            | Self::HandleTypeClassification
            | Self::DeletedFileHandleWatch
            | Self::DeviceNodeHandleInspection
            | Self::MemfdSealsAudit
            | Self::KernelEventNotifiers
            | Self::EpollSubscriptionHotspots
            | Self::HandleReversePathSearch => Requires(PROCESS_HANDLE_LANES),
            Self::FileLocksAudit => Requires(FILESYSTEM_LOCKS_LANES),
            // The descriptor-limit fact rides the resources insight lane
            // (`ResourceLimit` rows), so the platform source already exists;
            // the frontend saturation surface is what is missing.
            Self::HandleFdLimitSaturation => Requires(PROCESS_RESOURCE_LANES),
            Self::PipePeerPidLookup => Requires(PIPE_GRAPH_LANES),
            Self::UdsPeerAssociation => Requires(SOCKET_INVENTORY_LANES),
            Self::EmergencyCloseHandleAction => Requires(PROCESS_CONTROL_LANES),
            Self::HandleLeakTrendAnalysis => NotApplicable(
                "handle and descriptor leak trends are derived from the shared history projection; \
                 descriptor counts are owned by process handle insights",
            ),
            Self::WindowsAccessTokenAudit | Self::MountNamespaceRootLookup => {
                Requires(PROCESS_ISOLATION_LANES)
            }
            // -- Area 4: thread topology -----------------------------------
            // The wait channel rides the threads insight lane's per-thread
            // `wchan` field; the context-switch counters have their own
            // declared identity.
            Self::ThreadTopologyEnumeration
            | Self::ThreadRunqueueLatency
            | Self::ThreadUninterruptibleSleepDiagnosis
            | Self::ThreadWaitChannelClassification
            | Self::ThreadCallstackSampling
            | Self::ThreadFutexContention
            | Self::ThreadStackLayoutGuard
            | Self::ThreadPoolWorkerHeuristics
            | Self::ThreadZombieDetection
            | Self::ThreadLocalStorageAudit => Requires(PROCESS_THREAD_LANES),
            Self::ThreadContextSwitchRates => Requires(THREAD_CONTEXT_SWITCH_LANES),
            Self::ThreadSchedulingNiceAffinity => Requires(PROCESS_AFFINITY_LANES),
            Self::ThreadSuspendResumeControl => Requires(PROCESS_CONTROL_LANES),
            Self::ThreadChurnWarning => NotApplicable(
                "thread lifecycle churn alerts are derived by computing rate differentials \
                 across rolling thread topology frames",
            ),
            Self::ThreadCpuSliceHeatmap => NotApplicable(
                "per-thread CPU contribution slices are derived from aggregated thread sampling \
                 ratios in the shared projection",
            ),
            // -- Area 5: network and sockets -------------------------------
            // The per-connection RTT rides the registered per-process network
            // insight lane, next to the throughput rates.
            Self::ProcessNetworkThroughput | Self::SocketRttMetrics | Self::SocketStateTracking => {
                Requires(PROCESS_NETWORK_LANES)
            }
            // The system socket/listening-port inventory has its own declared
            // identity; the sibling queue-depth and unix-domain peer rows ride
            // the same lane. No adapter registers it yet, so the fold stays
            // honestly unsupported on every platform until one does.
            Self::SocketInventory
            | Self::ListeningPortTopology
            | Self::SocketQueueBacklog
            | Self::SocketConnectionAuditFeed
            | Self::MaliciousConnectionDetection
            | Self::OrphanSocketLeakDetection
            | Self::UdsPeerTopology => Requires(SOCKET_INVENTORY_LANES),
            Self::DnsReverseLookup => NotApplicable(
                "reverse PTR DNS lookups are an application-level asynchronous resolver derivation over \
                 socket IP addresses; they have no operating-system source step of their own",
            ),
            Self::GeoIpAsnLookup => NotApplicable(
                "GeoIP and autonomous system (ASN) attributions are an application-level database derivation over \
                 socket IP addresses; they have no operating-system source step of their own",
            ),
            Self::ApplicationProtocolInference => NotApplicable(
                "common network application protocols are an application-level heuristic dictionary over \
                 socket port numbers; they have no operating-system source step of their own",
            ),
            Self::NetworkRollingBandwidth => Requires(NETWORK_TELEMETRY_LANES),
            Self::NetworkSocketReset => Requires(SOCKET_CONTROL_LANES),
            Self::NetworkTrafficShaping => Requires(TRAFFIC_CONTROL_LANES),
            // -- Area 6: storage and filesystem I/O ------------------------
            // Per-process logical/physical bytes ride the process row scalars.
            Self::ProcessLogicalPhysicalIo => Requires(PROCESS_ROW_LANES),
            Self::ProcessIoPriority => Requires(STORAGE_IO_PRIORITY_LANES),
            Self::DiskDeviceTopology
            | Self::DiskIopsQueueLatency
            | Self::StorageTopAccessedFiles => Requires(STORAGE_TELEMETRY_LANES),
            Self::StorageFilesystemMountAudit
            | Self::VirtualMemoryFsCapacity
            | Self::StorageTrimDiscardSupport
            | Self::StorageSpaceDepletionCountdown
            | Self::RaidPoolTopology
            | Self::DiskReadOnlyFailureAlert => Requires(STORAGE_HEALTH_LANES),
            Self::StorageIoStallDiagnosis => Requires(PRESSURE_LANES),
            Self::DirtyWritebackFlow => Requires(STORAGE_WRITEBACK_LANES),
            // SMART health evidence is its own bounded observation lane.
            Self::DiskSmartHealth => Requires(STORAGE_SMART_LANES),
            // Swap-in/out counters are system memory telemetry.
            Self::SwapThroughputRate => Requires(MEMORY_TELEMETRY_LANES),
            // -- Area 7: hardware topology and NUMA ------------------------
            // The tree needs static socket/core topology plus the live
            // package/NUMA mapping; the core-class breakdown is the same
            // static-topology fact family.
            Self::HardwareTopologyTree
            | Self::CpuHeterogeneousCoreClass
            | Self::CpuInstructionFeatures
            | Self::PcieLinkSpeedDegradation
            | Self::HeterogeneousInterconnectArchitecture
            | Self::FirmwareSecureBoot => Requires(HARDWARE_TOPOLOGY_LANES),
            Self::SmbiosMemorySlots | Self::MemoryChannelSymmetry => Requires(SMBIOS_LANES),
            Self::CpuCacheTopology
            | Self::CpuCStateAnalysis
            | Self::CpuCoreFrequency
            | Self::CpuThermalBoostStatus
            | Self::CpuInterruptAffinityBalance => Requires(CPU_METRIC_LANES),
            // The NUMA memory distribution is a per-node fact: it needs the
            // declared NUMA topology identity, which no adapter registers yet.
            Self::NumaMemoryDistribution
            | Self::ProcessNumaMemoryPolicy
            | Self::NumaThrashingAlert => Requires(NUMA_TOPOLOGY_LANES),
            // -- Area 8: accelerator telemetry -----------------------------
            Self::GpuAdapterEnumeration
            | Self::GpuMemoryReadout
            | Self::GpuTemperatureReadout
            | Self::GpuPowerDraw
            | Self::GpuCoreClockFrequency
            | Self::GpuThrottleReasons
            | Self::GpuLinkTopology
            | Self::GpuFirmwareApiRuntime => Requires(GPU_TELEMETRY_LANES),
            Self::GpuResetTdrLogs => Requires(GPU_HANG_LANES),
            Self::NpuTelemetry | Self::NpuInferenceTelemetry => Requires(NPU_INVENTORY_LANES),
            Self::GpuEngineUtilization | Self::GpuCodecUtilization => Requires(GPU_ENGINE_LANES),
            // Per-process GPU attribution is its own insights facet.
            Self::ProcessGpuAttribution | Self::GpuProcessClients => Requires(PROCESS_GPU_LANES),
            // -- Area 9: power and thermal ---------------------------------
            Self::RaplPowerDraw | Self::SessionEnergyCostCalculation => {
                Requires(CPU_PACKAGE_POWER_LANES)
            }
            Self::EnergyImpactScore
            | Self::CpuEnergyPerformancePreference
            | Self::ProcessWakeupFrequency => Requires(CPU_METRIC_LANES),
            Self::PowerProfilesDaemon | Self::SleepInhibitLocks | Self::UsbDeviceRuntimePm => {
                Requires(POWER_PROFILE_LANES)
            }
            Self::ThermalZoneSensors | Self::FanSpeedTelemetry => Requires(THERMAL_SENSOR_LANES),
            // Battery facts ride the power-supply lane.
            Self::BatteryPowerInventory
            | Self::BatteryRemainingTimeEstimate
            | Self::DisplayBacklightPower => Requires(POWER_SUPPLY_LANES),
            // Thermal-throttle trigger counters are their own reliability fact,
            // distinct from the temperature readouts the sensor lane carries.
            // The real-time PROCHOT assertion rides the privileged MSR lane
            // beside the unprivileged cumulative counters, so the delivery now
            // requires both.
            Self::ThermalThrottleEvents => Requires(CPU_THROTTLE_THERMAL_LANES),
            // -- Area 10: security isolation -------------------------------
            Self::LinuxNamespaceAudit
            | Self::PosixCapabilitiesAudit
            | Self::SeccompFilterAudit
            | Self::LandlockSandboxAudit
            | Self::SandboxEnvironmentDetection
            | Self::SecurityNoNewPrivs
            | Self::CompilerHardeningAudit
            | Self::WindowsIntegrityLevels
            | Self::MacosTccPermissions
            | Self::PrivilegeAnomalyDetection
            | Self::MandatoryAccessControlLabels => Requires(PROCESS_ISOLATION_LANES),
            Self::CodeInjectionDetection => Requires(MEMORY_VMA_LANES),
            // The executable/argv0 mismatch compares two process-row identity
            // facts.
            Self::ProcessMasqueradingDetection
            | Self::ProcessOwnerCredentials
            | Self::BinarySignatureVerification => Requires(PROCESS_ROW_LANES),
            // -- Area 11: services and init --------------------------------
            Self::SystemdDependencyDag => Requires(SERVICE_GRAPH_LANES),
            Self::ServiceLogStream => Requires(SERVICE_LOG_LANES),
            Self::ServiceFailureDiagnosis => Requires(SERVICE_DIAGNOSIS_LANES),
            Self::ServiceInventoryStatus
            | Self::ServiceStateMachine
            | Self::ServiceUnitFileState
            | Self::ServiceRestartChurn
            | Self::ServiceTimerSchedule
            | Self::ServiceSocketActivation
            | Self::ServiceUnitFileDropIns
            | Self::ServiceCgroupResourceLimits
            | Self::UserServiceAggregation => Requires(SERVICE_INVENTORY_LANES),
            Self::ServiceLifecycleControl => Requires(SERVICE_LIFECYCLE_LANES),
            Self::StartupImpactAssessment => Requires(STARTUP_EVIDENCE_LANES),
            Self::StartupItemControl => Requires(STARTUP_CONTROL_LANES),
            // -- Area 12: pressure and saturation --------------------------
            // The PSI stall windows are their own declared identity; no adapter
            // registers a dedicated lane yet.
            Self::PsiMultiWindowTelemetry
            | Self::FilesystemLockLatency
            | Self::MemoryCompactionStall
            | Self::OomImminentWarning => Requires(PRESSURE_LANES),
            Self::GlobalFdPoolCapacity => Requires(FILESYSTEM_FD_LANES),
            Self::MemoryThrashingHealthScore => NotApplicable(
                "the score and its auditable deduction bill are a shared \
                 projection over already-typed inputs; the pressure input is \
                 owned by the pressure feature family",
            ),
            Self::PressureSaturationGrade => NotApplicable(
                "the red/yellow/green grade is a bounded classification of the \
                 shared system-health score; it has no operating-system source \
                 step of its own",
            ),
            Self::UseBottleneckAttribution => NotApplicable(
                "USE bottleneck attribution is a shared derivation over \
                 already-typed utilization/saturation facts; it has no \
                 operating-system source step of its own",
            ),
            // The normalized load folds the host load triple against the
            // logical-processor count, so both telemetry lanes are required.
            Self::PressureLoadAverageNormalized | Self::SoftirqSaturationAnalysis => {
                Requires(HOST_LOAD_LANES)
            }
            // The probe mechanism is windowing-system specific; its declared
            // identity is the desktop-responsiveness lane.
            Self::UnresponsiveAppDetection => Requires(DESKTOP_RESPONSIVENESS_LANES),
            Self::PressureDStateRatio => Requires(PROCESS_ROW_LANES),
            Self::PressureNativeAlertIntegration => Requires(DESKTOP_NOTIFICATION_LANES),
            // -- Area 13: IPC and D-Bus ------------------------------------
            // Shared memory spans both POSIX and System V object families, so
            // both declared identities are required; no adapter registers
            // either yet.
            Self::SharedMemorySegments
            | Self::NamedSemaphoresInventory
            | Self::PosixSysvMessageQueues
            | Self::SharedMemorySecurityAudit
            | Self::OrphanIpcReclamation => Requires(IPC_SHARED_MEMORY_LANES),
            // Bus topology and introspection ride one declared D-Bus identity;
            // pipe deadlock detection needs the declared pipe-graph identity.
            // No adapter registers either lane yet.
            Self::DbusServiceTopology
            | Self::DbusIntrospection
            | Self::DbusTrafficMonitoring
            | Self::DbusActivationLatency => Requires(DBUS_LANES),
            Self::PipeDeadlockDiagnosis
            | Self::PipeGraphVisualization
            | Self::WindowsNamedPipes
            | Self::TopIpcThroughputProducers
            | Self::IpcBufferSaturation => Requires(PIPE_GRAPH_LANES),
            // -- Area 14: dynamic tracing ----------------------------------
            // The process lifecycle event stream has its own declared identity
            // (no adapter registers it yet).
            Self::ProcessEventTrace => Requires(PROCESS_EVENT_LANES),
            // Stack-sampling (the flame graph) has no declared identity: the
            // declared `profiling.*` lanes cover hardware counters, syscall
            // distributions and off-CPU attribution, not sampled call stacks.
            Self::CpuFlameGraph => NotInVocabulary(
                "stack-sampling profiler facts have no capability identity yet; \
                 the declared `profiling.*` lanes cover PMU counters and off-CPU \
                 time only",
            ),
            // PMU counters and the syscall distribution each have their declared
            // identity; no adapter registers either lane yet.
            Self::PmuCounterAbstraction => Requires(PMU_PROFILING_LANES),
            Self::SyscallDistributionProfiling | Self::SlowSyscallTrap => {
                Requires(SYSCALL_PROFILING_LANES)
            }
            Self::OffCpuBlockingAnalysis => Requires(OFF_CPU_LANES),
            Self::ProcessFileActivityTrace => Requires(PROCESS_HANDLE_LANES),
            Self::SocketLifecycleAuditStream => Requires(PROCESS_NETWORK_LANES),
            Self::DynamicAllocationProfiling | Self::PageFaultCallchainBacktrace => {
                Requires(PROCESS_ROW_LANES)
            }
            Self::MutexContentionProfiler => Requires(PROCESS_THREAD_LANES),
            Self::ContextSwitchTriggerAnalysis => Requires(THREAD_CONTEXT_SWITCH_LANES),
            Self::DynamicTracingCapabilityGate => Requires(PROCESS_CONTROL_LANES),
            Self::FlightRecordingModeHook => NotApplicable(
                "low-overhead flight recording buffers trace events in shared memory \
                 without external platform telemetry",
            ),
            Self::TraceDataStandardExport => NotApplicable(
                "trace data format conversion formats shared trace buffers for export \
                 without platform dependencies",
            ),
            // -- Area 15: history and time travel --------------------------
            Self::GhostProcessesCapture => Requires(PROCESS_EVENT_LANES),
            Self::MultiResolutionRingBuffer
            | Self::MultiFormatExport
            | Self::TimeTravelScrubber
            | Self::TelemetryPercentileAggregation
            | Self::HistoryChartImageExport
            | Self::HistoryLttbDownsampling
            | Self::SystemSnapshotDiff
            | Self::BlackboxCrashPreservation
            | Self::OfflineSessionRecording
            | Self::EventTimelineBookmarks
            | Self::LeaderboardRankEvolution
            | Self::BenchmarkAbComparison
            | Self::VariableSpeedReplayEngine
            | Self::EnterprisePrivacyRedaction => NotApplicable(
                "the delivery is an application-layer fold or render over the \
                 shared history projection; it has no operating-system source \
                 step",
            ),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/ui_feature_platform_binding.rs"]
mod tests;
