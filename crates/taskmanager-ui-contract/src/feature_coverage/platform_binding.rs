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
static PROCESS_AFFINITY_LANES: &[CapabilityId] = &[
    CapabilityId::PROCESS_AFFINITY,
    CapabilityId::PROCESS_AFFINITY_CONTROL,
];
static PROCESS_ROW_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_LIST];
static PROCESS_THREAD_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_INSIGHTS_THREADS];
static PROCESS_HANDLE_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_INSIGHTS_OPEN_FILES];
static PROCESS_NETWORK_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_INSIGHTS_NETWORK];
static PROCESS_ISOLATION_LANES: &[CapabilityId] = &[CapabilityId::PROCESS_INSIGHTS_ISOLATION];
static STORAGE_TELEMETRY_LANES: &[CapabilityId] = &[CapabilityId::TELEMETRY_STORAGE];
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
static SERVICE_GRAPH_LANES: &[CapabilityId] = &[CapabilityId::SERVICE_DEPENDENCIES];
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
            Self::ProcessAffinityMask => Requires(PROCESS_AFFINITY_LANES),
            // -- Area 2: memory forensics ----------------------------------
            // RSS/PSS ride the per-process projection with field-level typed
            // availability (never a zero).
            Self::MemoryBreakdownRssPss => Requires(PROCESS_ROW_LANES),
            Self::MemoryVmaMap => NotInVocabulary(
                "the per-process VMA map has no capability identity yet; \
                 planned vocabulary (M3.3)",
            ),
            Self::MemoryLeakTrend => NotApplicable(
                "the leak trend is derived from the shared history projection; \
                 the memory samples it folds are owned by process telemetry",
            ),
            // -- Area 3: handles and descriptors ---------------------------
            Self::HandleEnumeration
            | Self::HandleTypeClassification
            | Self::DeletedFileHandleWatch => Requires(PROCESS_HANDLE_LANES),
            // -- Area 4: thread topology -----------------------------------
            Self::ThreadTopologyEnumeration | Self::ThreadRunqueueLatency => {
                Requires(PROCESS_THREAD_LANES)
            }
            Self::ThreadContextSwitchRates => NotInVocabulary(
                "per-thread context-switch counters have no capability identity \
                 yet; planned vocabulary (M3.3)",
            ),
            // -- Area 5: network and sockets -------------------------------
            Self::ProcessNetworkThroughput => Requires(PROCESS_NETWORK_LANES),
            Self::SocketInventory | Self::ListeningPortTopology => NotInVocabulary(
                "the system socket/listening-port inventory has no capability \
                 identity yet; planned vocabulary (M3.3)",
            ),
            // -- Area 6: storage and filesystem I/O ------------------------
            // Per-process logical/physical bytes ride the process row scalars.
            Self::ProcessLogicalPhysicalIo => Requires(PROCESS_ROW_LANES),
            Self::DiskDeviceTopology | Self::DiskIopsQueueLatency => {
                Requires(STORAGE_TELEMETRY_LANES)
            }
            // -- Area 7: hardware topology and NUMA ------------------------
            // The tree needs static socket/core topology plus the live
            // package/NUMA mapping.
            Self::HardwareTopologyTree => Requires(HARDWARE_TOPOLOGY_LANES),
            Self::CpuCacheTopology | Self::NumaMemoryDistribution | Self::CpuCStateAnalysis => {
                Requires(CPU_METRIC_LANES)
            }
            // -- Area 8: accelerator telemetry -----------------------------
            Self::GpuAdapterEnumeration => Requires(GPU_TELEMETRY_LANES),
            Self::NpuTelemetry => Requires(NPU_INVENTORY_LANES),
            Self::GpuEngineUtilization => Requires(GPU_ENGINE_LANES),
            // -- Area 9: power and thermal ---------------------------------
            Self::RaplPowerDraw => Requires(CPU_PACKAGE_POWER_LANES),
            Self::ThermalZoneSensors => Requires(THERMAL_SENSOR_LANES),
            // -- Area 10: security isolation -------------------------------
            Self::LinuxNamespaceAudit | Self::PosixCapabilitiesAudit | Self::SeccompFilterAudit => {
                Requires(PROCESS_ISOLATION_LANES)
            }
            // -- Area 11: services and init --------------------------------
            Self::SystemdDependencyDag => Requires(SERVICE_GRAPH_LANES),
            Self::ServiceLogStream => Requires(SERVICE_LOG_LANES),
            Self::ServiceFailureDiagnosis => Requires(SERVICE_DIAGNOSIS_LANES),
            // -- Area 12: pressure and saturation --------------------------
            Self::PsiMultiWindowTelemetry => NotInVocabulary(
                "PSI stall windows have no capability identity yet; planned \
                 `telemetry.pressure` vocabulary (M3.3)",
            ),
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
            // -- Area 13: IPC and D-Bus ------------------------------------
            Self::DbusServiceTopology | Self::DbusIntrospection | Self::PipeDeadlockDiagnosis => {
                NotInVocabulary(
                    "D-Bus and pipe-topology facts have no capability identity \
                 yet; planned `ipc.*` vocabulary (M3.3)",
                )
            }
            // -- Area 14: dynamic tracing ----------------------------------
            Self::PmuCounterAbstraction
            | Self::SyscallDistributionProfiling
            | Self::SlowSyscallTrap => NotInVocabulary(
                "profiling/tracing sources have no capability identity yet; \
                 planned `profiling.*` vocabulary (M3.3)",
            ),
            // -- Area 15: history and time travel --------------------------
            Self::MultiResolutionRingBuffer
            | Self::MultiFormatExport
            | Self::TimeTravelScrubber => NotApplicable(
                "the delivery is an application-layer fold over the shared \
                 history projection; it has no operating-system source step",
            ),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/ui_feature_platform_binding.rs"]
mod tests;
