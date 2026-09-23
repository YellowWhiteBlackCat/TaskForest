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
//!   `taskmanager-platform-contract`; it never invents a provider claim.
//! - `NotInVocabulary` is the explicit M3.1 baseline for a feature whose
//!   operating-system facts have no capability identity yet. It is visible
//!   debt (the ledger reports `Unregistered`), never a silent omission, and
//!   M3.3 lowers its count.
//! - `NotApplicable` is reserved for a delivery definition that is fully
//!   satisfied by shared projections or derivation and has no operating-system
//!   source step of its own. A platform gap that merely has not been
//!   implemented yet must NOT be legitimized as `NotApplicable`; it is
//!   `Requires` plus an absent/undeclared platform source.

use taskmanager_platform_contract::CapabilityId;

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
static PROCESS_AFFINITY_LANES: &[CapabilityId] = &[
    CapabilityId::PROCESS_AFFINITY,
    CapabilityId::PROCESS_AFFINITY_CONTROL,
];
static PROCESS_ROW_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_LIST];
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
static HOST_LOAD_LANES: &[CapabilityId] =
    &[CapabilityId::TELEMETRY_HOST, CapabilityId::TELEMETRY_CPU];
static STORAGE_TELEMETRY_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_STORAGE];
static STORAGE_SMART_LANES: &[CapabilityId] = &[CapabilityId::SMART];
static MEMORY_VMA_LANES: &[CapabilityId] = &[CapabilityId::MEMORY_VMA_MAP];
static THREAD_CONTEXT_SWITCH_LANES: &[CapabilityId] = &[CapabilityId::THREADS_CONTEXT_SWITCH];
static CPU_THROTTLE_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_CPU_THROTTLE];
static PRESSURE_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_PRESSURE];
static DESKTOP_RESPONSIVENESS_LANES: &[CapabilityId] = &[CapabilityId::DESKTOP_RESPONSIVENESS];
static DBUS_LANES: &[CapabilityId] = &[CapabilityId::IPC_DBUS];
static PIPE_GRAPH_LANES: &[CapabilityId] = &[CapabilityId::IPC_PIPE_GRAPH];
static PMU_PROFILING_LANES: &[CapabilityId] = &[CapabilityId::PROFILING_PMU];
static SYSCALL_PROFILING_LANES: &[CapabilityId] = &[CapabilityId::PROFILING_SYSCALLS];
static NUMA_TOPOLOGY_LANES: &[CapabilityId] = &[CapabilityId::NUMA_TOPOLOGY];
static HARDWARE_TOPOLOGY_LANES: &[CapabilityId] = &[
    CapabilityId::HARDWARE_INVENTORY,
    CapabilityId::TELEMETRY_CPU,
];
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
            Self::ProcessSchedulerPolicy | Self::ProcessPriorityMapping => {
                Requires(PROCESS_POLICY_LANES)
            }
            // The whole-tree action needs the list row identity (parent
            // linkage) plus the control lane; the descendant set itself is a
            // shared projection fold.
            Self::ProcessTreeKill => Requires(PROCESS_TREE_LANES),
            // The ancestry chain is a fold over the process rows' parent
            // identity and start-time evidence.
            Self::ProcessAncestorLineage => Requires(PROCESS_ROW_LANES),
            Self::ProcessAffinityMask => Requires(PROCESS_AFFINITY_LANES),
            // -- Area 2: memory forensics ----------------------------------
            // RSS/PSS ride the per-process projection with field-level typed
            // availability (never a zero); the fault counters and the
            // anonymous-huge-page charge are row scalars in the same family.
            Self::MemoryBreakdownRssPss
            | Self::MemoryPageFaults
            | Self::MemoryTransparentHugePages
            | Self::MemoryProcessSwapCharge => Requires(PROCESS_ROW_LANES),
            Self::MemoryVmaMap => Requires(MEMORY_VMA_LANES),
            Self::MemoryLeakTrend => NotApplicable(
                "the leak trend is derived from the shared history projection; \
                 the memory samples it folds are owned by process telemetry",
            ),
            // -- Area 3: handles and descriptors ---------------------------
            Self::HandleEnumeration
            | Self::HandleTypeClassification
            | Self::DeletedFileHandleWatch
            | Self::HandleReversePathSearch => Requires(PROCESS_HANDLE_LANES),
            // The descriptor-limit fact rides the resources insight lane
            // (`ResourceLimit` rows), so the platform source already exists;
            // the frontend saturation surface is what is missing.
            Self::HandleFdLimitSaturation => Requires(PROCESS_RESOURCE_LANES),
            // -- Area 4: thread topology -----------------------------------
            // The wait channel rides the threads insight lane's per-thread
            // `wchan` field; the context-switch counters have their own
            // declared identity.
            Self::ThreadTopologyEnumeration
            | Self::ThreadRunqueueLatency
            | Self::ThreadUninterruptibleSleepDiagnosis
            | Self::ThreadWaitChannelClassification => Requires(PROCESS_THREAD_LANES),
            Self::ThreadContextSwitchRates => Requires(THREAD_CONTEXT_SWITCH_LANES),
            // -- Area 5: network and sockets -------------------------------
            // The per-connection RTT rides the registered per-process network
            // insight lane, next to the throughput rates.
            Self::ProcessNetworkThroughput | Self::SocketRttMetrics => {
                Requires(PROCESS_NETWORK_LANES)
            }
            // The system socket/listening-port inventory has its own declared
            // identity; the sibling queue-depth and unix-domain peer rows ride
            // the same lane. No adapter registers it yet, so the fold stays
            // honestly unsupported on every platform until one does.
            Self::SocketInventory
            | Self::ListeningPortTopology
            | Self::SocketQueueBacklog
            | Self::UdsPeerTopology => Requires(SOCKET_INVENTORY_LANES),
            // -- Area 6: storage and filesystem I/O ------------------------
            // Per-process logical/physical bytes ride the process row scalars.
            Self::ProcessLogicalPhysicalIo => Requires(PROCESS_ROW_LANES),
            Self::DiskDeviceTopology | Self::DiskIopsQueueLatency => {
                Requires(STORAGE_TELEMETRY_LANES)
            }
            // SMART health evidence is its own bounded observation lane.
            Self::DiskSmartHealth => Requires(STORAGE_SMART_LANES),
            // Swap-in/out counters are system memory telemetry.
            Self::SwapThroughputRate => Requires(MEMORY_TELEMETRY_LANES),
            // -- Area 7: hardware topology and NUMA ------------------------
            // The tree needs static socket/core topology plus the live
            // package/NUMA mapping; the core-class breakdown is the same
            // static-topology fact family.
            Self::HardwareTopologyTree | Self::CpuHeterogeneousCoreClass => {
                Requires(HARDWARE_TOPOLOGY_LANES)
            }
            Self::CpuCacheTopology | Self::CpuCStateAnalysis | Self::CpuCoreFrequency => {
                Requires(CPU_METRIC_LANES)
            }
            // The NUMA memory distribution is a per-node fact: it needs the
            // declared NUMA topology identity, which no adapter registers yet.
            Self::NumaMemoryDistribution => Requires(NUMA_TOPOLOGY_LANES),
            // -- Area 8: accelerator telemetry -----------------------------
            Self::GpuAdapterEnumeration | Self::GpuMemoryReadout => Requires(GPU_TELEMETRY_LANES),
            Self::NpuTelemetry => Requires(NPU_INVENTORY_LANES),
            Self::GpuEngineUtilization => Requires(GPU_ENGINE_LANES),
            // Per-process GPU attribution is its own insights facet.
            Self::ProcessGpuAttribution => Requires(PROCESS_GPU_LANES),
            // -- Area 9: power and thermal ---------------------------------
            Self::RaplPowerDraw => Requires(CPU_PACKAGE_POWER_LANES),
            Self::ThermalZoneSensors => Requires(THERMAL_SENSOR_LANES),
            // Battery facts ride the power-supply lane.
            Self::BatteryPowerInventory => Requires(POWER_SUPPLY_LANES),
            // Thermal-throttle trigger counters are their own reliability fact,
            // distinct from the temperature readouts the sensor lane carries.
            Self::ThermalThrottleEvents => Requires(CPU_THROTTLE_LANES),
            // -- Area 10: security isolation -------------------------------
            Self::LinuxNamespaceAudit
            | Self::PosixCapabilitiesAudit
            | Self::SeccompFilterAudit
            | Self::SandboxEnvironmentDetection => Requires(PROCESS_ISOLATION_LANES),
            // The executable/argv0 mismatch compares two process-row identity
            // facts.
            Self::ProcessMasqueradingDetection => Requires(PROCESS_ROW_LANES),
            // -- Area 11: services and init --------------------------------
            Self::SystemdDependencyDag => Requires(SERVICE_GRAPH_LANES),
            Self::ServiceLogStream => Requires(SERVICE_LOG_LANES),
            Self::ServiceFailureDiagnosis => Requires(SERVICE_DIAGNOSIS_LANES),
            Self::ServiceInventoryStatus => Requires(SERVICE_INVENTORY_LANES),
            Self::ServiceLifecycleControl => Requires(SERVICE_LIFECYCLE_LANES),
            // -- Area 12: pressure and saturation --------------------------
            // The PSI stall windows are their own declared identity; no adapter
            // registers a dedicated lane yet.
            Self::PsiMultiWindowTelemetry => Requires(PRESSURE_LANES),
            Self::MemoryThrashingHealthScore => NotApplicable(
                "the score and its auditable deduction bill are a shared \
                 projection over already-typed inputs; the pressure input is \
                 owned by the pressure feature family",
            ),
            Self::UseBottleneckAttribution => NotApplicable(
                "USE bottleneck attribution is a shared derivation over \
                 already-typed utilization/saturation facts; it has no \
                 operating-system source step of its own",
            ),
            // The normalized load folds the host load triple against the
            // logical-processor count, so both telemetry lanes are required.
            Self::PressureLoadAverageNormalized => Requires(HOST_LOAD_LANES),
            // The probe mechanism is windowing-system specific; its declared
            // identity is the desktop-responsiveness lane.
            Self::UnresponsiveAppDetection => Requires(DESKTOP_RESPONSIVENESS_LANES),
            // -- Area 13: IPC and D-Bus ------------------------------------
            // Shared memory spans both POSIX and System V object families, so
            // both declared identities are required; no adapter registers
            // either yet.
            Self::SharedMemorySegments => Requires(IPC_SHARED_MEMORY_LANES),
            // Bus topology and introspection ride one declared D-Bus identity;
            // pipe deadlock detection needs the declared pipe-graph identity.
            // No adapter registers either lane yet.
            Self::DbusServiceTopology | Self::DbusIntrospection => Requires(DBUS_LANES),
            Self::PipeDeadlockDiagnosis => Requires(PIPE_GRAPH_LANES),
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
            // -- Area 15: history and time travel --------------------------
            Self::MultiResolutionRingBuffer
            | Self::MultiFormatExport
            | Self::TimeTravelScrubber
            | Self::TelemetryPercentileAggregation
            | Self::HistoryChartImageExport => NotApplicable(
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
