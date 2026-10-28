//! Feature coverage registry (P1 contract carrying capacity).
//!
//! [`crate::functional::ProductIntent`] stays a small, stable set of
//! cross-layer semantic intents, and
//! [`crate::capabilities::ComponentCapability`] covers component/surface
//! mechanics. Neither vocabulary reaches the granularity of the product
//! feature blueprints: the 225-item four-frontend parity blueprint (15 areas x
//! 15 items) and the 10 Wave 3 / Wave 4 core deliveries. This module adds the
//! missing middle layer: a stable [`FeatureId`] registry the parity ledger can
//! fold against, exactly the way declarations fold against
//! [`crate::capabilities::ComponentCapability::ALL`].
//!
//! Every feature in [`FeatureId::ALL`] must carry one explicit
//! [`CapabilitySupport`] decision per frontend. The support vocabulary is
//! deliberately REUSED from [`crate::capabilities`] - there is no second
//! taxonomy. A silent omission is drift; a deliberate difference must carry
//! its explanation. [`feature_coverage_findings`] is the one-call gate; an
//! empty result is the only passing state.
//!
//! ## The coverage matrix, not a shipped-word list
//!
//! [`FeatureId::ALL`] is the FULL roadmap parity matrix: every blueprint item
//! this registry has admitted, delivered or not. The gate's job is to make
//! every gap visible and honest, never to shrink out of sight. An item enters
//! `ALL` once it is a real product feature the ledger must track; it does not
//! need a shipped surface on every frontend to be admitted.
//!
//! Each cell is an explicit decision:
//!
//! - [`CapabilitySupport::Reference`] - the GPUI shape owns the reference
//!   semantics and really renders the shared projection facet that backs the
//!   feature.
//! - [`CapabilitySupport::Ported`] / [`CapabilitySupport::Native`] /
//!   [`CapabilitySupport::Divergent`] - a parallel shape renders the facet,
//!   delegates the mechanism to its toolkit, or reshapes it with a stated
//!   driver.
//! - [`CapabilitySupport::Unsupported`] - the shape does not offer the
//!   feature yet, with the driver stated. The REFERENCE shape may also declare
//!   `Unsupported { reason }` when the reference surface has not delivered the
//!   item yet ("reference surface not yet delivered"). It may NOT declare
//!   `Ported`/`Divergent`/`Native`: the shape that owns the semantics cannot
//!   port, diverge from, or delegate them. This keeps every gap visible
//!   without fabricating a `Reference` claim for an undelivered item.
//!
//! ## Semantic specifications: the single delivery authority
//!
//! Every feature carries one explicit, toolkit-neutral
//! [`FeatureSemanticSpec`] ([`FeatureId::semantic_spec`]). Its
//! [`FeatureSemanticSpec::delivery_definition`] is the SINGLE AUTHORITY for
//! "what counts as delivered" for that feature: no separate checklist, prose
//! claim, or frontend-local README may define delivery. A declaration is honest
//! only when its status matches that definition - a surface that renders less
//! than the specification must declare [`CapabilitySupport::Divergent`] with the
//! missing part (or [`CapabilitySupport::Unsupported`] when it renders none of
//! it), and a specification that outran the delivered surface is narrowed in the
//! same change; it is never silently overclaimed. A
//! `Ported`/`Native`/`Reference` cell claims the intent to match the
//! specification, never the match itself. A frontend that finds its surface and
//! the specification disagree must fix one of them, never reword a declaration
//! to hide the gap.
//!
//! The three boundaries most easily blurred are anchored in the specification
//! text itself: `hardware.cpu-cache-topology` covers the L1d/L1i/L2/L3 capacity
//! readout and explicitly EXCLUDES cache-sharing topology;
//! `pressure.memory-thrashing-health` requires the `SystemHealthScore` deduction
//! bill including the `MemoryFullStall` deduction, not a bare score; and
//! `hardware.numa-memory-distribution` requires per-NUMA-node distribution, not
//! a package-level or aggregate total (so, today, every frontend honestly
//! declares it `Unsupported`).
//!
//! ## Intentionally asymmetric with capability coverage
//!
//! This registry is the ROADMAP coverage matrix, and that is deliberately NOT
//! the same contract as [`crate::capabilities`], the delivered-surface
//! vocabulary:
//!
//! - Capability coverage admits only surfaces at least one shape really ships,
//!   and its reference shape must declare [`CapabilitySupport::Reference`]: the
//!   gate rejects a reference `Ported`/`Divergent`/`Unsupported`
//!   ([`crate::CapabilityFindingKind::ReferenceShapeCannotDefer`]).
//! - Feature coverage keeps the full roadmap, so its reference shape MAY declare
//!   [`CapabilitySupport::Unsupported`] for an undelivered item. Only the
//!   porting decisions are rejected
//!   ([`FeatureCoverageFindingKind::ReferenceShapeCannotPort`]).
//!
//! The asymmetry keeps both promises: a capability is never an undelivered
//! roadmap stub, and a roadmap feature is never hidden by a fabricated
//! `Reference` claim. Neither registry should be "corrected" into the other; the
//! behavior is pinned by
//! `tests/headless/ui_feature_coverage.rs::capability_and_feature_registries_are_deliberately_asymmetric`.
//!
//! ## Coverage status and structural invariant
//!
//! This is the extensible skeleton, not the finished 225-item mapping. `ALL`
//! registers 76 representative items, of which ten are Wave 3 / Wave 4
//! deliverables. The remaining blueprint items are a deliberate TODO: they must
//! be added to [`FeatureId`] (with a per-frontend decision and a semantic
//! specification) rather than claimed as covered. A gate that never sees a
//! feature cannot protect it.
//!
//! The registry is NOT a uniform 15 x 5 grid. The 225-item blueprint is 15
//! areas x 15 items, and the registry grows per area toward that full
//! blueprint: the five-item area baseline is only the starting census, not a
//! permanent shape. An area admits a sixth (or later fifteenth) item the moment
//! a real product feature needs its own authority, exactly as
//! `memory.process-swap-charge` did for the already-rendered per-process swap
//! scalar. The real structural invariant is therefore a per-area census plus
//! its exact total - pinned explicitly by
//! `feature_areas_are_well_formed_and_total` - never fixed grid arithmetic.
//! Every per-area addition stays a conscious census change, the total is the
//! census sum, and no area is ever emptied.
//!
//! ## Platform axis fold (P5)
//!
//! [`FeatureId::ALL`] is also the feature axis of the three-axis parity
//! ledger. [`FeatureId::platform_binding`] declares, per feature and again as
//! an exhaustive `const fn`, what the platform axis should provide: a
//! capability requirement set, an explicit vocabulary gap, or a
//! `NotApplicable` delivery that has no operating-system source step.
//! [`feature_platform_report`] folds that declaration against a supplied
//! [`PlatformCapabilitySurface`] (owned by `taskmanager-platform-contract`) and
//! one frontend declaration into [`FeaturePlatformLedger`], where `Missing` (no
//! frontend entry) and `Unsupported`/`Unregistered` (no platform source) come
//! from different axes and never infer each other. [`feature_platform_gate_findings`]
//! is the M3.4 hard gate over that fold: exhaustive grid, evidence on `Ready`,
//! typed reasons elsewhere, zero false success, bidirectional capability
//! binding, and directional ceilings from [`PLATFORM_GATE_BASELINE`]. The
//! platform identity and registration types are owned by
//! `taskmanager-platform-contract`; this crate composes them and never
//! re-exports them.
//!
//! ## Honesty boundary
//!
//! Like the capability registry, this proves DECLARATION discipline: every
//! registered feature is either explicitly delivered or explicitly refused by
//! every frontend. It does NOT prove behavioral equivalence; a
//! `Ported`/`Native` cell claims the intent to match, never the match itself.
//! It also does NOT prove that a `FeatureId`'s backing surface is finished -
//! only that some frontend really renders the shared projection facet that
//! backs it (or deliberately refuses it).

mod blueprint_dispositions;
mod declaration;
mod identifiers;
mod platform_axis;
mod platform_binding;
mod platform_gate;
mod semantic_spec;

pub use blueprint_dispositions::{
    BlueprintDisposition, BlueprintDispositionFinding, BlueprintDispositionRow,
    BlueprintDispositionTable, blueprint_disposition_findings,
};
pub use declaration::{
    FeatureCoverageDeclaration, FeatureCoverageEntry, FeatureCoverageFinding,
    FeatureCoverageFindingKind, FeatureCoverageStatus, feature_coverage_drift,
    feature_coverage_findings, feature_coverage_report,
};
pub use platform_axis::{
    FeaturePlatformCell, FeaturePlatformLedger, FeaturePlatformStatus, MISSING_DECLARATION_REASON,
    MISSING_UNSUPPORTED_REASON, NO_EVIDENCE, PartialCause, PlatformUnavailability, classify,
    feature_platform_report,
};
pub use platform_binding::{FEATURE_INDEPENDENT_CAPABILITIES, PlatformBinding};
pub use platform_gate::{
    PLATFORM_GATE_BASELINE, PLATFORM_GATE_POLICY, PlatformGateBaseline, PlatformGateFinding,
    PlatformGatePolicy, PlatformGateRule, feature_platform_cell_findings,
    feature_platform_gate_findings,
};
pub use semantic_spec::FeatureSemanticSpec;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FeatureArea {
    /// Area 1: process lifecycle, scheduling and control.
    ProcessLifecycle,
    /// Area 2: deep memory forensics and virtual address space analysis.
    MemoryForensics,
    /// Area 3: handles, file descriptors and kernel object audit.
    HandleDescriptorAudit,
    /// Area 4: thread topology, call stacks and kernel lock contention.
    ThreadTopology,
    /// Area 5: network throughput, sockets and live connection tracking.
    NetworkSockets,
    /// Area 6: storage topology, filesystem and I/O latency analysis.
    StorageFilesystemIo,
    /// Area 7: hardware topology, NUMA architecture and multi-socket
    /// coordination.
    HardwareTopologyNuma,
    /// Area 8: heterogeneous accelerator telemetry (dGPU, iGPU, NPU).
    AcceleratorTelemetry,
    /// Area 9: power, thermal zones and energy governance.
    PowerThermal,
    /// Area 10: security isolation, namespaces and sandbox audit.
    SecurityIsolation,
    /// Area 11: system services, daemons and init control.
    ServicesInit,
    /// Area 12: pressure stall (PSI), queueing and saturation diagnostics.
    PressureSaturation,
    /// Area 13: IPC, D-Bus and synchronization topology.
    IpcDbus,
    /// Area 14: dynamic tracing, kernel probes and profiling.
    DynamicTracing,
    /// Area 15: history flight recorder, time travel and formatted export.
    HistoryTimeTravel,
}

impl FeatureArea {
    /// Every blueprint area in canonical order.
    pub const ALL: &'static [Self] = &[
        Self::ProcessLifecycle,
        Self::MemoryForensics,
        Self::HandleDescriptorAudit,
        Self::ThreadTopology,
        Self::NetworkSockets,
        Self::StorageFilesystemIo,
        Self::HardwareTopologyNuma,
        Self::AcceleratorTelemetry,
        Self::PowerThermal,
        Self::SecurityIsolation,
        Self::ServicesInit,
        Self::PressureSaturation,
        Self::IpcDbus,
        Self::DynamicTracing,
        Self::HistoryTimeTravel,
    ];

    /// Stable machine name for gates and diagnostics.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::ProcessLifecycle => "process-lifecycle",
            Self::MemoryForensics => "memory-forensics",
            Self::HandleDescriptorAudit => "handle-descriptor-audit",
            Self::ThreadTopology => "thread-topology",
            Self::NetworkSockets => "network-sockets",
            Self::StorageFilesystemIo => "storage-filesystem-io",
            Self::HardwareTopologyNuma => "hardware-topology-numa",
            Self::AcceleratorTelemetry => "accelerator-telemetry",
            Self::PowerThermal => "power-thermal",
            Self::SecurityIsolation => "security-isolation",
            Self::ServicesInit => "services-init",
            Self::PressureSaturation => "pressure-saturation",
            Self::IpcDbus => "ipc-dbus",
            Self::DynamicTracing => "dynamic-tracing",
            Self::HistoryTimeTravel => "history-time-travel",
        }
    }

    /// Short human-readable title for reports.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::ProcessLifecycle => "Process lifecycle, scheduling and control",
            Self::MemoryForensics => "Deep memory forensics and virtual address space",
            Self::HandleDescriptorAudit => "Handles, file descriptors and kernel objects",
            Self::ThreadTopology => "Thread topology, call stacks and lock contention",
            Self::NetworkSockets => "Network throughput, sockets and connections",
            Self::StorageFilesystemIo => "Storage topology, filesystem and I/O latency",
            Self::HardwareTopologyNuma => "Hardware topology, NUMA and multi-socket",
            Self::AcceleratorTelemetry => "Accelerator telemetry (dGPU, iGPU, NPU)",
            Self::PowerThermal => "Power, thermal zones and energy governance",
            Self::SecurityIsolation => "Security isolation, namespaces and sandboxing",
            Self::ServicesInit => "System services, daemons and init",
            Self::PressureSaturation => "Pressure stall (PSI) and saturation diagnostics",
            Self::IpcDbus => "IPC, D-Bus and synchronization topology",
            Self::DynamicTracing => "Dynamic tracing, kernel probes and profiling",
            Self::HistoryTimeTravel => "History flight recorder, time travel and export",
        }
    }
}

/// Which blueprint produced a feature: the 225-item area list or the Wave 3 /
/// Wave 4 core delivery set./// Which blueprint produced a feature: the 225-item area list or the Wave 3 /
/// Wave 4 core delivery set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FeatureOrigin {
    /// A representative item from the 225-item blueprint's 15 areas.
    Blueprint225,
    /// One of the 10 Wave 3 / Wave 4 core architecture deliverables.
    Wave3Wave4,
}

impl FeatureOrigin {
    /// Stable machine name for gates and diagnostics.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Blueprint225 => "blueprint-225",
            Self::Wave3Wave4 => "wave3-wave4",
        }
    }
}

/// The blueprint's total slot count. The complete 225-slot census is
/// [`BlueprintDispositionTable`] (ADR-059); [`FeatureId::ALL`] is the typed
/// subset that has been adopted, so this count is not its length. The census
/// states every remaining slot explicitly instead of hiding it in prose.
pub const BLUEPRINT_ITEM_COUNT: usize = 225;

/// One feature identity in the coverage matrix.
///
/// Every variant here is a real product feature the parity ledger tracks -
/// delivered by at least one frontend, or an explicit gap every frontend
/// states. Feature ids are stable machine names and must be unique across
/// `ALL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FeatureId {
    /// Blueprint item 1: Linux scheduler policy perception and adjustment.
    ProcessSchedulerPolicy,
    /// Blueprint item 2: cross-platform nice / priority-class mapping.
    ProcessPriorityMapping,
    /// Blueprint item 3: process efficiency mode (EcoQoS / cgroup throttle).
    ProcessEfficiencyMode,
    /// Blueprint item 4: interactive CPU affinity bitmask editing.
    ProcessAffinityMask,
    /// Blueprint item 5: cascade process-tree control (tree kill).
    ProcessTreeKill,
    /// Blueprint item 6: two-phase graceful shutdown with automatic escalation.
    ProcessTwoPhaseTermination,
    /// Blueprint item 7: cgroups v2 runtime resource quota limits.
    ProcessCgroupResourceLimiting,
    /// Blueprint item 8: Linux OOM score perception.
    ProcessOomScore,
    /// Blueprint item 9: active process physical memory working set trimming.
    ProcessTrimWorkingSet,
    /// Blueprint item 10: on-demand process memory core dump / minidump generation.
    ProcessCoreDump,
    /// Blueprint item 11: daemon fault self-healing and automatic restart rule.
    ProcessAutoRestartRule,
    /// Blueprint item 12: process freeze and thaw control.
    ProcessFreezeThawControl,
    /// Blueprint item 13: user-defined process classification tags and highlight badges.
    ProcessCustomLabel,
    /// Blueprint item 14: full process ancestry lineage.
    ProcessAncestorLineage,
    /// Blueprint item 15: compound search filter over process projection.
    ProcessCompoundFilter,
    /// Blueprint item 16: RSS/PSS/USS and virtual memory breakdown.
    MemoryBreakdownRssPss,
    /// Blueprint item 17: two-tier smaps rollup summary and lazy detailed smaps parsing.
    MemorySmapsRollupTiering,
    /// Per-process swap charge (`VmSwap`).
    MemoryProcessSwapCharge,
    /// Blueprint item 18: full virtual memory area (VMA) map.
    MemoryVmaMap,
    /// Blueprint item 20: shared memory segment cross-process PID reference correlation.
    SharedMemorySegmentCrossAudit,
    /// Blueprint item 21: private and shared dirty page accumulation and writeback pressure.
    MemoryDirtyPagePressure,
    /// Blueprint item 27: long-term memory leak trend prediction.
    MemoryLeakTrend,
    /// Blueprint item 19: minor/major page-fault accounting.
    MemoryPageFaults,
    /// Blueprint item 22: ZSWAP / compressed swap and memory telemetry.
    MemoryCompressionZswap,
    /// Blueprint item 23: anonymous transparent huge-page accounting.
    MemoryTransparentHugePages,
    /// Blueprint item 24: user-space memory allocator heap topology and fragmentation inference.
    UserspaceAllocatorInference,
    /// Blueprint item 25: Windows commit limit charge and paged / non-paged kernel pool breakdown.
    WindowsCommitKernelPools,
    /// Blueprint item 26: macOS physical memory breakdown (compressed, wired, resident).
    MacosMemoryClassification,
    /// Blueprint item 28: safe read-only memory signature probing.
    MemoryReadOnlyStringProbing,
    /// Blueprint item 29: dual-snapshot virtual memory area differential comparison.
    VmaSnapshotDiff,
    /// Blueprint item 30: memory mapped executable code section integrity verification.
    CodeSectionIntegrityAudit,
    /// Blueprint item 31: structured FD / handle enumeration.
    HandleEnumeration,
    /// Blueprint item 32: automatic handle type classifier.
    HandleTypeClassification,
    /// Blueprint item 33: open file locks and exclusive status detection.
    FileLocksAudit,
    /// Blueprint item 34: deleted-but-held (ghost) file detection.
    DeletedFileHandleWatch,
    /// Blueprint item 35: descriptor-limit saturation (RLIMIT_NOFILE).
    HandleFdLimitSaturation,
    /// Blueprint item 36: bidirectional pipe peer PID reverse lookup.
    PipePeerPidLookup,
    /// Blueprint item 37: Unix-domain-socket peer process association.
    UdsPeerAssociation,
    /// Blueprint item 38: hardware device node descriptor inspection.
    DeviceNodeHandleInspection,
    /// Blueprint item 39: reverse lookup of handles by path.
    HandleReversePathSearch,
    /// Blueprint item 40: targeted emergency handle closure action.
    EmergencyCloseHandleAction,
    /// Blueprint item 41: handle and descriptor leak trend analysis.
    HandleLeakTrendAnalysis,
    /// Blueprint item 42: Windows security access token privileges audit.
    WindowsAccessTokenAudit,
    /// Blueprint item 43: anonymous memory file (memfd) sealed properties audit.
    MemfdSealsAudit,
    /// Blueprint item 44: mount namespace root inode dual-perspective path resolution.
    MountNamespaceRootLookup,
    /// Blueprint item 45: epoll and kqueue descriptor subscription hot-spot monitoring.
    EpollSubscriptionHotspots,
    /// Blueprint item 46: per-thread topology and resource statistics.
    ThreadTopologyEnumeration,
    /// Blueprint item 57: runqueue wait latency per thread.
    ThreadRunqueueLatency,
    /// Blueprint item 47: voluntary / involuntary context switch rates.
    ThreadContextSwitchRates,
    /// Blueprint item 48: uninterruptible-sleep (D-state) diagnosis.
    ThreadUninterruptibleSleepDiagnosis,
    /// Blueprint item 49: kernel wait-channel classification.
    ThreadWaitChannelClassification,
    /// Blueprint item 50: safe user and kernel thread callstack sampling.
    ThreadCallstackSampling,
    /// Blueprint item 51: fast userspace mutex (futex) contention and lock owner perception.
    ThreadFutexContention,
    /// Blueprint item 52: thread stack memory layout, usage depth, and guard page safety.
    ThreadStackLayoutGuard,
    /// Blueprint item 53: per-thread nice priority and CPU core affinity orchestration.
    ThreadSchedulingNiceAffinity,
    /// Blueprint item 54: thread pool activity heuristics and idle worker identification.
    ThreadPoolWorkerHeuristics,
    /// Blueprint item 55: targeted single-thread suspend and resume lifecycle control.
    ThreadSuspendResumeControl,
    /// Blueprint item 56: zombie thread and unharvested task exit detection.
    ThreadZombieDetection,
    /// Blueprint item 58: thread-local storage (TLS) area sizing and overhead audit.
    ThreadLocalStorageAudit,
    /// Blueprint item 59: high-frequency thread creation and destruction churn warning.
    ThreadChurnWarning,
    /// Blueprint item 60: multi-thread aggregated CPU execution contribution slice breakdown.
    ThreadCpuSliceHeatmap,
    /// Blueprint item 61: per-process Rx/Tx throughput telemetry.
    ProcessNetworkThroughput,
    /// Blueprint item 62: TCP/UDP/Unix socket inventory.
    SocketInventory,
    /// Blueprint item 63: TCP connection state tracking.
    SocketStateTracking,
    /// Blueprint item 64: listening-port topology and conflict warnings.
    ListeningPortTopology,
    /// Blueprint item 65: per-socket round-trip-time telemetry.
    SocketRttMetrics,
    /// Blueprint item 66: socket send/receive queue backlog.
    SocketQueueBacklog,
    /// Blueprint item 67: asynchronous reverse PTR domain resolution with LRU caching.
    DnsReverseLookup,
    /// Blueprint item 68: IP geolocation and autonomous system (ASN) attribution.
    GeoIpAsnLookup,
    /// Blueprint item 69: real-time socket connection creation and closure audit event stream.
    SocketConnectionAuditFeed,
    /// Blueprint item 70: anomalous port and heuristic threat intelligence detection.
    MaliciousConnectionDetection,
    /// Blueprint item 71: emergency network socket reset and drop action.
    NetworkSocketReset,
    /// Blueprint item 72: process network bandwidth limits and traffic shaping.
    NetworkTrafficShaping,
    /// Blueprint item 73: common application-layer network protocol heuristic inference.
    ApplicationProtocolInference,
    /// Blueprint item 74: half-closed (CLOSE_WAIT) and orphan socket leak detection.
    OrphanSocketLeakDetection,
    /// Blueprint item 75: rolling network bandwidth history time series.
    NetworkRollingBandwidth,
    /// Blueprint item 76: logical vs physical I/O accounting.
    ProcessLogicalPhysicalIo,
    /// Blueprint item 77: system storage device and partition topology.
    DiskDeviceTopology,
    /// Blueprint item 78: per-disk IOPS, queue depth and latency.
    DiskIopsQueueLatency,
    /// Blueprint item 79: process I/O scheduling class and priority adjustment.
    ProcessIoPriority,
    /// Blueprint item 80: filesystem mount integrity and status audit.
    StorageFilesystemMountAudit,
    /// Blueprint item 81: kernel dirty pages and writeback flow.
    DirtyWritebackFlow,
    /// Blueprint item 82: NVMe/SATA SMART health and wear readouts.
    DiskSmartHealth,
    /// Blueprint item 83: filesystem space and Inode exhaustion countdown prediction.
    StorageSpaceDepletionCountdown,
    /// Blueprint item 84: disk I/O stall and sync write hang diagnosis.
    StorageIoStallDiagnosis,
    /// Blueprint item 85: system swap-in/swap-out throughput.
    SwapThroughputRate,
    /// Blueprint item 86: active file hotspot ranking and write throughput attribution.
    StorageTopAccessedFiles,
    /// Blueprint item 87: in-memory virtual filesystem capacity accounting.
    VirtualMemoryFsCapacity,
    /// Blueprint item 88: storage media TRIM / Discard reclamation support.
    StorageTrimDiscardSupport,
    /// Blueprint item 89: multi-disk array (RAID/LVM/ZFS/Btrfs) pool topology and sync rebuild progress.
    RaidPoolTopology,
    /// Blueprint item 90: emergency read-only remount and media failure alerts.
    DiskReadOnlyFailureAlert,
    /// Blueprint item 91: socket -> NUMA -> core -> thread topology tree.
    HardwareTopologyTree,
    /// Blueprint item 92: performance/efficiency core-class breakdown.
    CpuHeterogeneousCoreClass,
    /// Blueprint item 93: multi-level CPU cache capacity readout.
    CpuCacheTopology,
    /// Blueprint item 94: NUMA node memory distribution and locality.
    NumaMemoryDistribution,
    /// Blueprint item 95: per-process NUMA memory allocation policy (MPOL) configuration.
    ProcessNumaMemoryPolicy,
    /// Blueprint item 96: cross-NUMA node remote memory allocation and thrashing alert.
    NumaThrashingAlert,
    /// Blueprint item 97: PCIe bus topology and negotiated link speed degradation detection.
    PcieLinkSpeedDegradation,
    /// Blueprint item 98: per-core clock frequency readout.
    CpuCoreFrequency,
    /// Blueprint item 99: advanced hardware thermal velocity and precision boost state perception.
    CpuThermalBoostStatus,
    /// Blueprint item 100: CPU instruction-set features and extensions.
    CpuInstructionFeatures,
    /// Blueprint item 101: SMBIOS / DMI physical memory slots (DIMM) details.
    SmbiosMemorySlots,
    /// Blueprint item 102: memory channel topology and symmetry assessment.
    MemoryChannelSymmetry,
    /// Blueprint item 103: CPU-GPU-NPU heterogeneous interconnect and unified memory architecture.
    HeterogeneousInterconnectArchitecture,
    /// Blueprint item 104: motherboard chipset, firmware version and secure boot state.
    FirmwareSecureBoot,
    /// Blueprint item 105: per-core hardware IRQ and softirq load distribution balance matrix.
    CpuInterruptAffinityBalance,
    /// Blueprint item 106: dGPU / iGPU adapter enumeration.
    GpuAdapterEnumeration,
    /// Blueprint item 107: native NPU telemetry integration.
    NpuTelemetry,
    /// Blueprint item 108: per-engine GPU utilization breakdown.
    GpuEngineUtilization,
    /// Blueprint item 109: GPU memory usage readout.
    GpuMemoryReadout,
    /// Blueprint item 110: per-process GPU attribution.
    ProcessGpuAttribution,
    /// Blueprint item 111: GPU core and hot-spot temperature readout.
    GpuTemperatureReadout,
    /// Blueprint item 112: GPU board power draw and limit telemetry.
    GpuPowerDraw,
    /// Blueprint item 113: GPU core clock frequency telemetry.
    GpuCoreClockFrequency,
    /// Blueprint item 114: GPU performance limiters and throttle reasons.
    GpuThrottleReasons,
    /// Blueprint item 115: dedicated hardware video encoder/decoder engine utilization.
    GpuCodecUtilization,
    /// Blueprint item 116: per-process GPU context handles and client connections.
    GpuProcessClients,
    /// Blueprint item 117: NPU tensor workflow queue depth and inference execution latency.
    NpuInferenceTelemetry,
    /// Blueprint item 118: multi-GPU interconnect bridge bandwidth and topology status.
    GpuLinkTopology,
    /// Blueprint item 119: GPU driver, VBIOS and graphics API runtime.
    GpuFirmwareApiRuntime,
    /// Blueprint item 120: GPU reset and timeout detection recovery event telemetry.
    GpuResetTdrLogs,
    /// Blueprint item 121: Intel/AMD RAPL hardware power draw.
    RaplPowerDraw,
    /// Blueprint item 122: relative energy impact score across CPU, GPU, wakeups.
    EnergyImpactScore,
    /// Blueprint item 124: battery remaining time and charge estimate.
    BatteryRemainingTimeEstimate,
    /// Blueprint item 125: system thermal zone sensor traversal.
    ThermalZoneSensors,
    /// Blueprint item 127: CPU C-state residency analysis.
    CpuCStateAnalysis,
    /// Blueprint item 123: battery charge/power/health inventory.
    BatteryPowerInventory,
    /// Blueprint item 126: CPU thermal-throttle (PROCHOT) events.
    ThermalThrottleEvents,
    /// Blueprint item 128: CPU energy performance preference (EPP) bias.
    CpuEnergyPerformancePreference,
    /// Blueprint item 129: chassis and cooling fan speed telemetry.
    FanSpeedTelemetry,
    /// Blueprint item 130: per-process timer expiration and wakeup frequency telemetry.
    ProcessWakeupFrequency,
    /// Blueprint item 131: power-profiles daemon state and switching.
    PowerProfilesDaemon,
    /// Blueprint item 132: sleep/suspend inhibitor locks and power assertions detection.
    SleepInhibitLocks,
    /// Blueprint item 133: display screen and backlight power consumption correlation.
    DisplayBacklightPower,
    /// Blueprint item 134: USB and external bus device runtime power management (Runtime PM) audit.
    UsbDeviceRuntimePm,
    /// Blueprint item 135: session cumulative energy consumption and cost conversion.
    SessionEnergyCostCalculation,
    /// Blueprint item 136 / Wave 3 task 1: Linux namespace audit.
    LinuxNamespaceAudit,
    /// Blueprint item 137: container/sandbox environment identification.
    SandboxEnvironmentDetection,
    /// Blueprint item 138 / Wave 3 task 2: POSIX capabilities decode.
    PosixCapabilitiesAudit,
    /// Blueprint item 139: mandatory access control policy label inspection.
    MandatoryAccessControlLabels,
    /// Blueprint item 140: seccomp syscall filter audit.
    SeccompFilterAudit,
    /// Blueprint item 141: Linux Landlock runtime sandbox ruleset audit.
    LandlockSandboxAudit,
    /// Blueprint item 142: no-new-privileges flag audit.
    SecurityNoNewPrivs,
    /// Blueprint item 143: user and group identity credentials audit.
    ProcessOwnerCredentials,
    /// Blueprint item 144: executable binary digital signature and certificate chain verification.
    BinarySignatureVerification,
    /// Blueprint item 145: binary compiler hardening flags audit.
    CompilerHardeningAudit,
    /// Blueprint item 146: Windows integrity levels and AppContainer sandboxing audit.
    WindowsIntegrityLevels,
    /// Blueprint item 147: macOS entitlements and TCC hardware privacy permissions audit.
    MacosTccPermissions,
    /// Blueprint item 148: heuristic high-privilege anomaly detection and escalation alert.
    PrivilegeAnomalyDetection,
    /// Blueprint item 149: executable/argv0 masquerading detection.
    ProcessMasqueradingDetection,
    /// Blueprint item 150: executable memory scan detecting anonymous unmapped code and DLL injection.
    CodeInjectionDetection,
    /// Blueprint item 151: service unit inventory and status.
    ServiceInventoryStatus,
    /// Blueprint item 152: system and user session service units aggregation.
    UserServiceAggregation,
    /// Blueprint item 153: granular service supervisor state machine.
    ServiceStateMachine,
    /// Blueprint item 154: service unit file state (enabled/disabled/masked).
    ServiceUnitFileState,
    /// Blueprint item 155: typed service lifecycle control.
    ServiceLifecycleControl,
    /// Blueprint item 156: service unit cgroup memory usage and ceiling.
    ServiceCgroupResourceLimits,
    /// Blueprint item 157 / Wave 3 task 3: systemd dependency DAG.
    SystemdDependencyDag,
    /// Blueprint item 158: service failure root-cause diagnosis.
    ServiceFailureDiagnosis,
    /// Blueprint item 159 / Wave 3 task 4: sd-journal service log stream.
    ServiceLogStream,
    /// Blueprint item 160: service restart churn and start-limit accounting.
    ServiceRestartChurn,
    /// Blueprint item 161: systemd timer elapse schedule.
    ServiceTimerSchedule,
    /// Blueprint item 162: socket activation trigger relationships.
    ServiceSocketActivation,
    /// Blueprint item 163: service unit file and drop-in configuration overrides.
    ServiceUnitFileDropIns,
    /// Blueprint item 164: startup app boot-impact assessment.
    StartupImpactAssessment,
    /// Blueprint item 165: reversible startup item enable/disable control.
    StartupItemControl,
    /// Blueprint item 166 / Wave 3 task 5: PSI multi-window telemetry.
    PsiMultiWindowTelemetry,
    /// Blueprint item 167: red/yellow/green saturation grading.
    PressureSaturationGrade,
    /// Blueprint item 168 / Wave 3 task 6: thrashing and health scoring.
    MemoryThrashingHealthScore,
    /// Blueprint item 169: logical-processor-normalized load average.
    PressureLoadAverageNormalized,
    /// Blueprint item 170: hardware IRQ and softirq CPU saturation analysis.
    SoftirqSaturationAnalysis,
    /// Blueprint item 171: share of observed tasks blocked in D-state.
    PressureDStateRatio,
    /// Blueprint item 172: filesystem metadata and Inode lock contention latency.
    FilesystemLockLatency,
    /// Blueprint item 173: memory compaction stall and transparent huge page allocation delay.
    MemoryCompactionStall,
    /// Blueprint item 174: critical memory exhaustion and imminent kernel OOM-killer alert.
    OomImminentWarning,
    /// Blueprint item 175: kernel global file descriptor pool capacity.
    GlobalFdPoolCapacity,
    /// Blueprint item 176: IPC pipe and message queue buffer filling and saturation monitoring.
    IpcBufferSaturation,
    /// Blueprint item 177: USE-methodology bottleneck attribution.
    UseBottleneckAttribution,
    /// Blueprint item 178: unresponsive foreground application detection.
    UnresponsiveAppDetection,
    /// Blueprint item 180: native desktop alert and tray notification integration.
    PressureNativeAlertIntegration,
    /// Blueprint item 181 / Wave 4 task 7: D-Bus service topology.
    DbusServiceTopology,
    /// Blueprint item 182: D-Bus introspection browsing.
    DbusIntrospection,
    /// Blueprint item 183: real-time D-Bus message flow and method call frequency telemetry.
    DbusTrafficMonitoring,
    /// Blueprint item 184: POSIX / System V shared-memory segment inventory.
    SharedMemorySegments,
    /// Blueprint item 185: POSIX and System V message queue depth and capacity limits.
    PosixSysvMessageQueues,
    /// Blueprint item 186: named semaphores and mutexes inventory.
    NamedSemaphoresInventory,
    /// Blueprint item 187: anonymous pipe bidirectional topology visualization.
    PipeGraphVisualization,
    /// Blueprint item 188: Unix-domain-socket peer topology.
    UdsPeerTopology,
    /// Blueprint item 189: Windows named pipe and mailslot namespace enumeration.
    WindowsNamedPipes,
    /// Blueprint item 190 / Wave 4 task 8: pipe deadlock diagnosis.
    PipeDeadlockDiagnosis,
    /// Blueprint item 191: cross-process kernel eventfd and signalfd association.
    KernelEventNotifiers,
    /// Blueprint item 192: shared memory segment permissions and global leak risk audit.
    SharedMemorySecurityAudit,
    /// Blueprint item 193: D-Bus on-demand auto-activation service response latency profiling.
    DbusActivationLatency,
    /// Blueprint item 194: unattached orphaned IPC shared memory and semaphore resource detection.
    OrphanIpcReclamation,
    /// Blueprint item 195: system-wide IPC data throughput ranking across communication channels.
    TopIpcThroughputProducers,
    /// Blueprint item 196 / Wave 4 task 9: hardware PMU counter abstraction.
    PmuCounterAbstraction,
    /// Blueprint item 197: syscall frequency and latency distribution.
    SyscallDistributionProfiling,
    /// Blueprint item 198: slow syscall trap.
    SlowSyscallTrap,
    /// Blueprint item 199: off-CPU time attribution and blocking duration.
    OffCpuBlockingAnalysis,
    /// Blueprint item 200: real-time process file activity monitor stream.
    ProcessFileActivityTrace,
    /// Blueprint item 201: real-time network socket lifecycle call audit stream.
    SocketLifecycleAuditStream,
    /// Blueprint item 202: process fork/exec/exit event trace.
    ProcessEventTrace,
    /// Blueprint item 203: CPU stack-sampling flame graph.
    CpuFlameGraph,
    /// Blueprint item 204: dynamic memory allocation hotspot backtrace profiling.
    DynamicAllocationProfiling,
    /// Blueprint item 205: mutex contention hotspot and wait duration profiling.
    MutexContentionProfiler,
    /// Blueprint item 206: page-fault occurrence callchain backtrace localization.
    PageFaultCallchainBacktrace,
    /// Blueprint item 207: voluntary and involuntary context-switch trigger categorization.
    ContextSwitchTriggerAnalysis,
    /// Blueprint item 208: low-overhead in-memory flight recording trace hook.
    FlightRecordingModeHook,
    /// Blueprint item 209: cross-tool standard trace data format export (Chrome Trace / Perfetto).
    TraceDataStandardExport,
    /// Blueprint item 210: dynamic tracing privileges and kernel safety restrictions self-audit.
    DynamicTracingCapabilityGate,
    /// Blueprint item 211 / Wave 4 task 10: multi-resolution ring buffer.
    MultiResolutionRingBuffer,
    /// Blueprint item 212: interactive time-travel scrubber.
    TimeTravelScrubber,
    /// Blueprint item 213: short-lived and exited ghost process retention and trace capture.
    GhostProcessesCapture,
    /// Blueprint item 214: arbitrary two-point system state differential comparison.
    SystemSnapshotDiff,
    /// Blueprint item 215: pre-crash system telemetry blackbox automatic preservation.
    BlackboxCrashPreservation,
    /// Blueprint item 216: offline session recording (.tmrec) serialization and replay load.
    OfflineSessionRecording,
    /// Blueprint item 217: structured multi-format export engine.
    MultiFormatExport,
    /// Blueprint item 218: percentile aggregate analytics (P50/P90/P99).
    TelemetryPercentileAggregation,
    /// Blueprint item 219: gap-preserving LTTB downsampling for history charts.
    HistoryLttbDownsampling,
    /// Blueprint item 220: user-defined event timeline bookmarks and notes annotation.
    EventTimelineBookmarks,
    /// Blueprint item 221: process leaderboard ranking evolution over time replay.
    LeaderboardRankEvolution,
    /// Blueprint item 222: automated performance benchmarking and A/B diff comparison.
    BenchmarkAbComparison,
    /// Blueprint item 223: chart image export (SVG/PNG).
    HistoryChartImageExport,
    /// Blueprint item 224: variable-speed telemetry replay simulation engine.
    VariableSpeedReplayEngine,
    /// Blueprint item 225: enterprise privacy data redaction and compliance audit export.
    EnterprisePrivacyRedaction,
}

impl FeatureId {
    /// Every gated feature in canonical order; declarations fold against
    /// this set the way capability declarations fold against
    /// [`crate::capabilities::ComponentCapability::ALL`].
    pub const ALL: &'static [Self] = &[
        Self::ProcessSchedulerPolicy,
        Self::ProcessPriorityMapping,
        Self::ProcessEfficiencyMode,
        Self::ProcessAffinityMask,
        Self::ProcessTreeKill,
        Self::ProcessTwoPhaseTermination,
        Self::ProcessCgroupResourceLimiting,
        Self::ProcessOomScore,
        Self::ProcessTrimWorkingSet,
        Self::ProcessCoreDump,
        Self::ProcessAutoRestartRule,
        Self::ProcessFreezeThawControl,
        Self::ProcessCustomLabel,
        Self::ProcessAncestorLineage,
        Self::ProcessCompoundFilter,
        Self::MemoryBreakdownRssPss,
        Self::MemorySmapsRollupTiering,
        Self::MemoryProcessSwapCharge,
        Self::MemoryVmaMap,
        Self::SharedMemorySegmentCrossAudit,
        Self::MemoryDirtyPagePressure,
        Self::MemoryLeakTrend,
        Self::MemoryPageFaults,
        Self::MemoryCompressionZswap,
        Self::MemoryTransparentHugePages,
        Self::UserspaceAllocatorInference,
        Self::WindowsCommitKernelPools,
        Self::MacosMemoryClassification,
        Self::MemoryReadOnlyStringProbing,
        Self::VmaSnapshotDiff,
        Self::CodeSectionIntegrityAudit,
        Self::HandleEnumeration,
        Self::HandleTypeClassification,
        Self::FileLocksAudit,
        Self::DeletedFileHandleWatch,
        Self::HandleFdLimitSaturation,
        Self::PipePeerPidLookup,
        Self::UdsPeerAssociation,
        Self::DeviceNodeHandleInspection,
        Self::HandleReversePathSearch,
        Self::EmergencyCloseHandleAction,
        Self::HandleLeakTrendAnalysis,
        Self::WindowsAccessTokenAudit,
        Self::MemfdSealsAudit,
        Self::MountNamespaceRootLookup,
        Self::EpollSubscriptionHotspots,
        Self::ThreadTopologyEnumeration,
        Self::ThreadRunqueueLatency,
        Self::ThreadContextSwitchRates,
        Self::ThreadUninterruptibleSleepDiagnosis,
        Self::ThreadWaitChannelClassification,
        Self::ThreadCallstackSampling,
        Self::ThreadFutexContention,
        Self::ThreadStackLayoutGuard,
        Self::ThreadSchedulingNiceAffinity,
        Self::ThreadPoolWorkerHeuristics,
        Self::ThreadSuspendResumeControl,
        Self::ThreadZombieDetection,
        Self::ThreadLocalStorageAudit,
        Self::ThreadChurnWarning,
        Self::ThreadCpuSliceHeatmap,
        Self::ProcessNetworkThroughput,
        Self::SocketInventory,
        Self::SocketStateTracking,
        Self::ListeningPortTopology,
        Self::SocketRttMetrics,
        Self::SocketQueueBacklog,
        Self::DnsReverseLookup,
        Self::GeoIpAsnLookup,
        Self::SocketConnectionAuditFeed,
        Self::MaliciousConnectionDetection,
        Self::NetworkSocketReset,
        Self::NetworkTrafficShaping,
        Self::ApplicationProtocolInference,
        Self::OrphanSocketLeakDetection,
        Self::NetworkRollingBandwidth,
        Self::ProcessLogicalPhysicalIo,
        Self::DiskDeviceTopology,
        Self::DiskIopsQueueLatency,
        Self::ProcessIoPriority,
        Self::StorageFilesystemMountAudit,
        Self::DirtyWritebackFlow,
        Self::DiskSmartHealth,
        Self::StorageSpaceDepletionCountdown,
        Self::StorageIoStallDiagnosis,
        Self::SwapThroughputRate,
        Self::StorageTopAccessedFiles,
        Self::VirtualMemoryFsCapacity,
        Self::StorageTrimDiscardSupport,
        Self::RaidPoolTopology,
        Self::DiskReadOnlyFailureAlert,
        Self::HardwareTopologyTree,
        Self::CpuHeterogeneousCoreClass,
        Self::CpuCacheTopology,
        Self::NumaMemoryDistribution,
        Self::ProcessNumaMemoryPolicy,
        Self::NumaThrashingAlert,
        Self::PcieLinkSpeedDegradation,
        Self::CpuCoreFrequency,
        Self::CpuThermalBoostStatus,
        Self::CpuInstructionFeatures,
        Self::SmbiosMemorySlots,
        Self::MemoryChannelSymmetry,
        Self::HeterogeneousInterconnectArchitecture,
        Self::FirmwareSecureBoot,
        Self::CpuInterruptAffinityBalance,
        Self::GpuAdapterEnumeration,
        Self::NpuTelemetry,
        Self::GpuEngineUtilization,
        Self::GpuMemoryReadout,
        Self::ProcessGpuAttribution,
        Self::GpuTemperatureReadout,
        Self::GpuPowerDraw,
        Self::GpuCoreClockFrequency,
        Self::GpuThrottleReasons,
        Self::GpuCodecUtilization,
        Self::GpuProcessClients,
        Self::NpuInferenceTelemetry,
        Self::GpuLinkTopology,
        Self::GpuFirmwareApiRuntime,
        Self::GpuResetTdrLogs,
        Self::RaplPowerDraw,
        Self::EnergyImpactScore,
        Self::BatteryRemainingTimeEstimate,
        Self::ThermalZoneSensors,
        Self::CpuCStateAnalysis,
        Self::BatteryPowerInventory,
        Self::ThermalThrottleEvents,
        Self::CpuEnergyPerformancePreference,
        Self::FanSpeedTelemetry,
        Self::ProcessWakeupFrequency,
        Self::PowerProfilesDaemon,
        Self::SleepInhibitLocks,
        Self::DisplayBacklightPower,
        Self::UsbDeviceRuntimePm,
        Self::SessionEnergyCostCalculation,
        Self::LinuxNamespaceAudit,
        Self::SandboxEnvironmentDetection,
        Self::PosixCapabilitiesAudit,
        Self::MandatoryAccessControlLabels,
        Self::SeccompFilterAudit,
        Self::LandlockSandboxAudit,
        Self::SecurityNoNewPrivs,
        Self::ProcessOwnerCredentials,
        Self::BinarySignatureVerification,
        Self::CompilerHardeningAudit,
        Self::WindowsIntegrityLevels,
        Self::MacosTccPermissions,
        Self::PrivilegeAnomalyDetection,
        Self::ProcessMasqueradingDetection,
        Self::CodeInjectionDetection,
        Self::ServiceInventoryStatus,
        Self::UserServiceAggregation,
        Self::ServiceStateMachine,
        Self::ServiceUnitFileState,
        Self::ServiceLifecycleControl,
        Self::ServiceCgroupResourceLimits,
        Self::SystemdDependencyDag,
        Self::ServiceFailureDiagnosis,
        Self::ServiceLogStream,
        Self::ServiceRestartChurn,
        Self::ServiceTimerSchedule,
        Self::ServiceSocketActivation,
        Self::ServiceUnitFileDropIns,
        Self::StartupImpactAssessment,
        Self::StartupItemControl,
        Self::PsiMultiWindowTelemetry,
        Self::PressureSaturationGrade,
        Self::MemoryThrashingHealthScore,
        Self::PressureLoadAverageNormalized,
        Self::SoftirqSaturationAnalysis,
        Self::PressureDStateRatio,
        Self::FilesystemLockLatency,
        Self::MemoryCompactionStall,
        Self::OomImminentWarning,
        Self::GlobalFdPoolCapacity,
        Self::IpcBufferSaturation,
        Self::UseBottleneckAttribution,
        Self::UnresponsiveAppDetection,
        Self::PressureNativeAlertIntegration,
        Self::DbusServiceTopology,
        Self::DbusIntrospection,
        Self::DbusTrafficMonitoring,
        Self::SharedMemorySegments,
        Self::PosixSysvMessageQueues,
        Self::NamedSemaphoresInventory,
        Self::PipeGraphVisualization,
        Self::UdsPeerTopology,
        Self::WindowsNamedPipes,
        Self::PipeDeadlockDiagnosis,
        Self::KernelEventNotifiers,
        Self::SharedMemorySecurityAudit,
        Self::DbusActivationLatency,
        Self::OrphanIpcReclamation,
        Self::TopIpcThroughputProducers,
        Self::PmuCounterAbstraction,
        Self::SyscallDistributionProfiling,
        Self::SlowSyscallTrap,
        Self::OffCpuBlockingAnalysis,
        Self::ProcessFileActivityTrace,
        Self::SocketLifecycleAuditStream,
        Self::ProcessEventTrace,
        Self::CpuFlameGraph,
        Self::DynamicAllocationProfiling,
        Self::MutexContentionProfiler,
        Self::PageFaultCallchainBacktrace,
        Self::ContextSwitchTriggerAnalysis,
        Self::FlightRecordingModeHook,
        Self::TraceDataStandardExport,
        Self::DynamicTracingCapabilityGate,
        Self::MultiResolutionRingBuffer,
        Self::TimeTravelScrubber,
        Self::GhostProcessesCapture,
        Self::SystemSnapshotDiff,
        Self::BlackboxCrashPreservation,
        Self::OfflineSessionRecording,
        Self::MultiFormatExport,
        Self::TelemetryPercentileAggregation,
        Self::HistoryLttbDownsampling,
        Self::EventTimelineBookmarks,
        Self::LeaderboardRankEvolution,
        Self::BenchmarkAbComparison,
        Self::HistoryChartImageExport,
        Self::VariableSpeedReplayEngine,
        Self::EnterprisePrivacyRedaction,
    ];

    /// Which blueprint produced this feature.
    #[must_use]
    pub const fn origin(self) -> FeatureOrigin {
        match self {
            Self::LinuxNamespaceAudit
            | Self::PosixCapabilitiesAudit
            | Self::SystemdDependencyDag
            | Self::ServiceLogStream
            | Self::PsiMultiWindowTelemetry
            | Self::MemoryThrashingHealthScore
            | Self::DbusServiceTopology
            | Self::PipeDeadlockDiagnosis
            | Self::PmuCounterAbstraction
            | Self::MultiResolutionRingBuffer => FeatureOrigin::Wave3Wave4,
            _ => FeatureOrigin::Blueprint225,
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/ui_feature_coverage.rs"]
mod tests;
