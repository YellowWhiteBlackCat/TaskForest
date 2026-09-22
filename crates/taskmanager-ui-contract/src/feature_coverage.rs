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
//! ## Coverage status / TODO
//!
//! This is the extensible skeleton, not the finished 225-item mapping. `ALL`
//! registers 75 representative items - five per blueprint area - of which ten
//! are Wave 3 / Wave 4 deliverables. The remaining blueprint items are a
//! deliberate TODO: they must be added to [`FeatureId`] (with a per-frontend
//! decision and a semantic specification) rather than claimed as covered. A
//! gate that never sees a feature cannot protect it.
//!
//! ## Platform axis fold (P5)
//!
//! [`FeatureId::ALL`] is also the feature axis of the three-axis parity
//! ledger. [`FeatureId::platform_binding`] declares, per feature and again as
//! an exhaustive `const fn`, what the platform axis should provide: a
//! capability requirement set, an explicit vocabulary gap, or a
//! `NotApplicable` delivery that has no operating-system source step.
//! [`feature_platform_report`] folds that declaration against a supplied
//! [`PlatformCapabilitySurface`] and one frontend declaration into
//! [`FeaturePlatformLedger`], where `Missing` (no frontend entry) and
//! `Unsupported`/`Unregistered` (no platform source) come from different axes
//! and never infer each other. The platform identity type is
//! `taskmanager-platform-contract::PlatformAxis`; this crate composes it and
//! never re-exports it.
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

use crate::capabilities::CapabilitySupport;
use crate::keybindings::FrontendShape;

mod platform_axis;
mod platform_binding;
mod semantic_spec;

pub use platform_axis::{
    FeaturePlatformCell, FeaturePlatformLedger, FeaturePlatformStatus, MISSING_DECLARATION_REASON,
    MISSING_UNSUPPORTED_REASON, NO_EVIDENCE, PartialCause, PlatformCapabilitySurface,
    PlatformSource, PlatformSourceError, PlatformUnavailability, classify, feature_platform_report,
};
pub use platform_binding::PlatformBinding;
pub use semantic_spec::FeatureSemanticSpec;

/// One functional area of the 225-item four-frontend parity blueprint.
///
/// The 15 variants mirror the blueprint's 15 areas one-to-one. The 10 Wave 3 /
/// Wave 4 deliverables do not introduce an extra axis: each is filed under the
/// blueprint area it deepens (see [`FeatureId::area`]).
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

/// One feature identity in the coverage matrix.
///
/// Every variant here is a real product feature the parity ledger tracks -
/// delivered by at least one frontend, or an explicit gap every frontend
/// states. Feature ids are stable machine names and must be unique across
/// `ALL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FeatureId {
    // -- Area 1: process lifecycle -----------------------------------------
    /// Blueprint item 1: Linux scheduler policy perception and adjustment.
    ProcessSchedulerPolicy,
    /// Blueprint item 2: cross-platform nice / priority-class mapping.
    ProcessPriorityMapping,
    /// Blueprint item 4: interactive CPU affinity bitmask editing.
    ProcessAffinityMask,
    /// Blueprint item 5: cascade process-tree control (tree kill).
    ProcessTreeKill,
    /// Blueprint item 14: full process ancestry lineage.
    ProcessAncestorLineage,
    // -- Area 2: memory forensics ------------------------------------------
    /// Blueprint item 16: RSS/PSS/USS and virtual memory breakdown.
    MemoryBreakdownRssPss,
    /// Blueprint item 18: full virtual memory area (VMA) map.
    MemoryVmaMap,
    /// Blueprint item 27: long-term memory leak trend prediction.
    MemoryLeakTrend,
    /// Blueprint item 19: minor/major page-fault accounting.
    MemoryPageFaults,
    /// Blueprint item 23: anonymous transparent huge-page accounting.
    MemoryTransparentHugePages,
    // -- Area 3: handles and descriptors -----------------------------------
    /// Blueprint item 31: structured FD / handle enumeration.
    HandleEnumeration,
    /// Blueprint item 32: automatic handle type classifier.
    HandleTypeClassification,
    /// Blueprint item 34: deleted-but-held (ghost) file detection.
    DeletedFileHandleWatch,
    /// Blueprint item 35: descriptor-limit saturation (RLIMIT_NOFILE).
    HandleFdLimitSaturation,
    /// Blueprint item 39: reverse lookup of handles by path.
    HandleReversePathSearch,
    // -- Area 4: thread topology -------------------------------------------
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
    // -- Area 5: network and sockets ---------------------------------------
    /// Blueprint item 61: per-process Rx/Tx throughput telemetry.
    ProcessNetworkThroughput,
    /// Blueprint item 62: TCP/UDP/Unix socket inventory.
    SocketInventory,
    /// Blueprint item 64: listening-port topology and conflict warnings.
    ListeningPortTopology,
    /// Blueprint item 65: per-socket round-trip-time telemetry.
    SocketRttMetrics,
    /// Blueprint item 66: socket send/receive queue backlog.
    SocketQueueBacklog,
    // -- Area 6: storage and filesystem I/O --------------------------------
    /// Blueprint item 76: logical vs physical I/O accounting.
    ProcessLogicalPhysicalIo,
    /// Blueprint item 77: system storage device and partition topology.
    DiskDeviceTopology,
    /// Blueprint item 78: per-disk IOPS, queue depth and latency.
    DiskIopsQueueLatency,
    /// Blueprint item 82: NVMe/SATA SMART health and wear readouts.
    DiskSmartHealth,
    /// Blueprint item 85: system swap-in/swap-out throughput.
    SwapThroughputRate,
    // -- Area 7: hardware topology and NUMA --------------------------------
    /// Blueprint item 91: socket -> NUMA -> core -> thread topology tree.
    HardwareTopologyTree,
    /// Blueprint item 93: multi-level CPU cache capacity readout.
    CpuCacheTopology,
    /// Blueprint item 94: NUMA node memory distribution and locality.
    NumaMemoryDistribution,
    /// Blueprint item 92: performance/efficiency core-class breakdown.
    CpuHeterogeneousCoreClass,
    /// Blueprint item 98: per-core clock frequency readout.
    CpuCoreFrequency,
    // -- Area 8: accelerator telemetry -------------------------------------
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
    // -- Area 9: power and thermal -----------------------------------------
    /// Blueprint item 121: Intel/AMD RAPL hardware power draw.
    RaplPowerDraw,
    /// Blueprint item 125: system thermal zone sensor traversal.
    ThermalZoneSensors,
    /// Blueprint item 127: CPU C-state residency analysis.
    CpuCStateAnalysis,
    /// Blueprint item 123: battery charge/power/health inventory.
    BatteryPowerInventory,
    /// Blueprint item 126: CPU thermal-throttle (PROCHOT) events.
    ThermalThrottleEvents,
    // -- Area 10: security isolation ---------------------------------------
    /// Blueprint item 136 / Wave 3 task 1: Linux namespace audit.
    LinuxNamespaceAudit,
    /// Blueprint item 138 / Wave 3 task 2: POSIX capabilities decode.
    PosixCapabilitiesAudit,
    /// Blueprint item 140: seccomp syscall filter audit.
    SeccompFilterAudit,
    /// Blueprint item 137: container/sandbox environment identification.
    SandboxEnvironmentDetection,
    /// Blueprint item 149: executable/argv0 masquerading detection.
    ProcessMasqueradingDetection,
    // -- Area 11: services and init ----------------------------------------
    /// Blueprint item 157 / Wave 3 task 3: systemd dependency DAG.
    SystemdDependencyDag,
    /// Blueprint item 159 / Wave 3 task 4: sd-journal service log stream.
    ServiceLogStream,
    /// Blueprint item 158: service failure root-cause diagnosis.
    ServiceFailureDiagnosis,
    /// Blueprint item 151: service unit inventory and status.
    ServiceInventoryStatus,
    /// Blueprint item 155: typed service lifecycle control.
    ServiceLifecycleControl,
    // -- Area 12: pressure and saturation ----------------------------------
    /// Blueprint item 166 / Wave 3 task 5: PSI multi-window telemetry.
    PsiMultiWindowTelemetry,
    /// Blueprint item 168 / Wave 3 task 6: thrashing and health scoring.
    MemoryThrashingHealthScore,
    /// Blueprint item 177: USE-methodology bottleneck attribution.
    UseBottleneckAttribution,
    /// Blueprint item 169: logical-processor-normalized load average.
    PressureLoadAverageNormalized,
    /// Blueprint item 178: unresponsive foreground application detection.
    UnresponsiveAppDetection,
    // -- Area 13: IPC and D-Bus --------------------------------------------
    /// Blueprint item 181 / Wave 4 task 7: D-Bus service topology.
    DbusServiceTopology,
    /// Blueprint item 182: D-Bus introspection browsing.
    DbusIntrospection,
    /// Blueprint item 190 / Wave 4 task 8: pipe deadlock diagnosis.
    PipeDeadlockDiagnosis,
    /// Blueprint item 184: POSIX / System V shared-memory segment inventory.
    SharedMemorySegments,
    /// Blueprint item 188: Unix-domain-socket peer topology.
    UdsPeerTopology,
    // -- Area 14: dynamic tracing ------------------------------------------
    /// Blueprint item 196 / Wave 4 task 9: hardware PMU counter abstraction.
    PmuCounterAbstraction,
    /// Blueprint item 197: syscall frequency and latency distribution.
    SyscallDistributionProfiling,
    /// Blueprint item 198: slow syscall trap.
    SlowSyscallTrap,
    /// Blueprint item 202: process fork/exec/exit event trace.
    ProcessEventTrace,
    /// Blueprint item 203: CPU stack-sampling flame graph.
    CpuFlameGraph,
    // -- Area 15: history and time travel ----------------------------------
    /// Blueprint item 211 / Wave 4 task 10: multi-resolution ring buffer.
    MultiResolutionRingBuffer,
    /// Blueprint item 217: structured multi-format export engine.
    MultiFormatExport,
    /// Blueprint item 212: interactive time-travel scrubber.
    TimeTravelScrubber,
    /// Blueprint item 218: percentile aggregate analytics (P50/P90/P99).
    TelemetryPercentileAggregation,
    /// Blueprint item 223: chart image export (SVG/PNG).
    HistoryChartImageExport,
}

impl FeatureId {
    /// Every gated feature in canonical order; declarations fold against
    /// this set the way capability declarations fold against
    /// [`crate::capabilities::ComponentCapability::ALL`].
    pub const ALL: &'static [Self] = &[
        Self::ProcessSchedulerPolicy,
        Self::ProcessPriorityMapping,
        Self::ProcessAffinityMask,
        Self::ProcessTreeKill,
        Self::ProcessAncestorLineage,
        Self::MemoryBreakdownRssPss,
        Self::MemoryVmaMap,
        Self::MemoryLeakTrend,
        Self::MemoryPageFaults,
        Self::MemoryTransparentHugePages,
        Self::HandleEnumeration,
        Self::HandleTypeClassification,
        Self::DeletedFileHandleWatch,
        Self::HandleFdLimitSaturation,
        Self::HandleReversePathSearch,
        Self::ThreadTopologyEnumeration,
        Self::ThreadRunqueueLatency,
        Self::ThreadContextSwitchRates,
        Self::ThreadUninterruptibleSleepDiagnosis,
        Self::ThreadWaitChannelClassification,
        Self::ProcessNetworkThroughput,
        Self::SocketInventory,
        Self::ListeningPortTopology,
        Self::SocketRttMetrics,
        Self::SocketQueueBacklog,
        Self::ProcessLogicalPhysicalIo,
        Self::DiskDeviceTopology,
        Self::DiskIopsQueueLatency,
        Self::DiskSmartHealth,
        Self::SwapThroughputRate,
        Self::HardwareTopologyTree,
        Self::CpuCacheTopology,
        Self::NumaMemoryDistribution,
        Self::CpuHeterogeneousCoreClass,
        Self::CpuCoreFrequency,
        Self::GpuAdapterEnumeration,
        Self::NpuTelemetry,
        Self::GpuEngineUtilization,
        Self::GpuMemoryReadout,
        Self::ProcessGpuAttribution,
        Self::RaplPowerDraw,
        Self::ThermalZoneSensors,
        Self::CpuCStateAnalysis,
        Self::BatteryPowerInventory,
        Self::ThermalThrottleEvents,
        Self::LinuxNamespaceAudit,
        Self::PosixCapabilitiesAudit,
        Self::SeccompFilterAudit,
        Self::SandboxEnvironmentDetection,
        Self::ProcessMasqueradingDetection,
        Self::SystemdDependencyDag,
        Self::ServiceLogStream,
        Self::ServiceFailureDiagnosis,
        Self::ServiceInventoryStatus,
        Self::ServiceLifecycleControl,
        Self::PsiMultiWindowTelemetry,
        Self::MemoryThrashingHealthScore,
        Self::UseBottleneckAttribution,
        Self::PressureLoadAverageNormalized,
        Self::UnresponsiveAppDetection,
        Self::DbusServiceTopology,
        Self::DbusIntrospection,
        Self::PipeDeadlockDiagnosis,
        Self::SharedMemorySegments,
        Self::UdsPeerTopology,
        Self::PmuCounterAbstraction,
        Self::SyscallDistributionProfiling,
        Self::SlowSyscallTrap,
        Self::ProcessEventTrace,
        Self::CpuFlameGraph,
        Self::MultiResolutionRingBuffer,
        Self::MultiFormatExport,
        Self::TimeTravelScrubber,
        Self::TelemetryPercentileAggregation,
        Self::HistoryChartImageExport,
    ];

    /// Stable machine name for gates and diagnostics.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::ProcessSchedulerPolicy => "process.scheduler-policy",
            Self::ProcessPriorityMapping => "process.priority-mapping",
            Self::ProcessAffinityMask => "process.affinity-mask",
            Self::ProcessTreeKill => "process.tree-kill",
            Self::ProcessAncestorLineage => "process.ancestor-lineage",
            Self::MemoryBreakdownRssPss => "memory.breakdown-rss-pss",
            Self::MemoryVmaMap => "memory.vma-map",
            Self::MemoryLeakTrend => "memory.leak-trend",
            Self::MemoryPageFaults => "memory.page-faults",
            Self::MemoryTransparentHugePages => "memory.transparent-huge-pages",
            Self::HandleEnumeration => "handles.enumeration",
            Self::HandleTypeClassification => "handles.type-classification",
            Self::DeletedFileHandleWatch => "handles.deleted-file-watch",
            Self::HandleFdLimitSaturation => "handles.fd-limit-saturation",
            Self::HandleReversePathSearch => "handles.reverse-path-search",
            Self::ThreadTopologyEnumeration => "threads.topology",
            Self::ThreadRunqueueLatency => "threads.runqueue-latency",
            Self::ThreadContextSwitchRates => "threads.context-switch-rates",
            Self::ThreadUninterruptibleSleepDiagnosis => "threads.uninterruptible-sleep",
            Self::ThreadWaitChannelClassification => "threads.wait-channel-classification",
            Self::ProcessNetworkThroughput => "network.process-throughput",
            Self::SocketInventory => "network.socket-inventory",
            Self::ListeningPortTopology => "network.listening-port-topology",
            Self::SocketRttMetrics => "network.socket-rtt",
            Self::SocketQueueBacklog => "network.socket-queue-backlog",
            Self::ProcessLogicalPhysicalIo => "storage.process-logical-physical-io",
            Self::DiskDeviceTopology => "storage.device-topology",
            Self::DiskIopsQueueLatency => "storage.iops-queue-latency",
            Self::DiskSmartHealth => "storage.smart-health",
            Self::SwapThroughputRate => "storage.swap-throughput",
            Self::HardwareTopologyTree => "hardware.topology-tree",
            Self::CpuCacheTopology => "hardware.cpu-cache-topology",
            Self::NumaMemoryDistribution => "hardware.numa-memory-distribution",
            Self::CpuHeterogeneousCoreClass => "hardware.heterogeneous-cores",
            Self::CpuCoreFrequency => "hardware.core-frequency",
            Self::GpuAdapterEnumeration => "gpu.adapter-enumeration",
            Self::NpuTelemetry => "accelerator.npu-telemetry",
            Self::GpuEngineUtilization => "gpu.engine-utilization",
            Self::GpuMemoryReadout => "gpu.memory-usage",
            Self::ProcessGpuAttribution => "gpu.process-attribution",
            Self::RaplPowerDraw => "power.rapl-draw",
            Self::ThermalZoneSensors => "power.thermal-zones",
            Self::CpuCStateAnalysis => "power.cstate-analysis",
            Self::BatteryPowerInventory => "power.battery-inventory",
            Self::ThermalThrottleEvents => "power.thermal-throttle-events",
            Self::LinuxNamespaceAudit => "security.namespace-audit",
            Self::PosixCapabilitiesAudit => "security.posix-capabilities",
            Self::SeccompFilterAudit => "security.seccomp-filter",
            Self::SandboxEnvironmentDetection => "security.sandbox-detection",
            Self::ProcessMasqueradingDetection => "security.masquerading-detection",
            Self::SystemdDependencyDag => "services.dependency-dag",
            Self::ServiceLogStream => "services.log-stream",
            Self::ServiceFailureDiagnosis => "services.failure-diagnosis",
            Self::ServiceInventoryStatus => "services.inventory",
            Self::ServiceLifecycleControl => "services.lifecycle-control",
            Self::PsiMultiWindowTelemetry => "pressure.psi-multi-window",
            Self::MemoryThrashingHealthScore => "pressure.memory-thrashing-health",
            Self::UseBottleneckAttribution => "pressure.use-attribution",
            Self::PressureLoadAverageNormalized => "pressure.load-average-normalized",
            Self::UnresponsiveAppDetection => "pressure.unresponsive-apps",
            Self::DbusServiceTopology => "ipc.dbus-service-topology",
            Self::DbusIntrospection => "ipc.dbus-introspection",
            Self::PipeDeadlockDiagnosis => "ipc.pipe-deadlock",
            Self::SharedMemorySegments => "ipc.shared-memory-segments",
            Self::UdsPeerTopology => "ipc.uds-peer-topology",
            Self::PmuCounterAbstraction => "tracing.pmu-counters",
            Self::SyscallDistributionProfiling => "tracing.syscall-distribution",
            Self::SlowSyscallTrap => "tracing.slow-syscall-trap",
            Self::ProcessEventTrace => "tracing.process-event-trace",
            Self::CpuFlameGraph => "tracing.cpu-flame-graph",
            Self::MultiResolutionRingBuffer => "history.multi-resolution-ring",
            Self::MultiFormatExport => "history.multi-format-export",
            Self::TimeTravelScrubber => "history.time-travel-scrubber",
            Self::TelemetryPercentileAggregation => "history.percentile-aggregation",
            Self::HistoryChartImageExport => "history.chart-image-export",
        }
    }

    /// The blueprint area this feature belongs to.
    #[must_use]
    pub const fn area(self) -> FeatureArea {
        match self {
            Self::ProcessSchedulerPolicy
            | Self::ProcessPriorityMapping
            | Self::ProcessAffinityMask
            | Self::ProcessTreeKill
            | Self::ProcessAncestorLineage => FeatureArea::ProcessLifecycle,
            Self::MemoryBreakdownRssPss
            | Self::MemoryVmaMap
            | Self::MemoryLeakTrend
            | Self::MemoryPageFaults
            | Self::MemoryTransparentHugePages => FeatureArea::MemoryForensics,
            Self::HandleEnumeration
            | Self::HandleTypeClassification
            | Self::DeletedFileHandleWatch
            | Self::HandleFdLimitSaturation
            | Self::HandleReversePathSearch => FeatureArea::HandleDescriptorAudit,
            Self::ThreadTopologyEnumeration
            | Self::ThreadRunqueueLatency
            | Self::ThreadContextSwitchRates
            | Self::ThreadUninterruptibleSleepDiagnosis
            | Self::ThreadWaitChannelClassification => FeatureArea::ThreadTopology,
            Self::ProcessNetworkThroughput
            | Self::SocketInventory
            | Self::ListeningPortTopology
            | Self::SocketRttMetrics
            | Self::SocketQueueBacklog => FeatureArea::NetworkSockets,
            Self::ProcessLogicalPhysicalIo
            | Self::DiskDeviceTopology
            | Self::DiskIopsQueueLatency
            | Self::DiskSmartHealth
            | Self::SwapThroughputRate => FeatureArea::StorageFilesystemIo,
            Self::HardwareTopologyTree
            | Self::CpuCacheTopology
            | Self::NumaMemoryDistribution
            | Self::CpuHeterogeneousCoreClass
            | Self::CpuCoreFrequency => FeatureArea::HardwareTopologyNuma,
            Self::GpuAdapterEnumeration
            | Self::NpuTelemetry
            | Self::GpuEngineUtilization
            | Self::GpuMemoryReadout
            | Self::ProcessGpuAttribution => FeatureArea::AcceleratorTelemetry,
            Self::RaplPowerDraw
            | Self::ThermalZoneSensors
            | Self::CpuCStateAnalysis
            | Self::BatteryPowerInventory
            | Self::ThermalThrottleEvents => FeatureArea::PowerThermal,
            Self::LinuxNamespaceAudit
            | Self::PosixCapabilitiesAudit
            | Self::SeccompFilterAudit
            | Self::SandboxEnvironmentDetection
            | Self::ProcessMasqueradingDetection => FeatureArea::SecurityIsolation,
            Self::SystemdDependencyDag
            | Self::ServiceLogStream
            | Self::ServiceFailureDiagnosis
            | Self::ServiceInventoryStatus
            | Self::ServiceLifecycleControl => FeatureArea::ServicesInit,
            Self::PsiMultiWindowTelemetry
            | Self::MemoryThrashingHealthScore
            | Self::UseBottleneckAttribution
            | Self::PressureLoadAverageNormalized
            | Self::UnresponsiveAppDetection => FeatureArea::PressureSaturation,
            Self::DbusServiceTopology
            | Self::DbusIntrospection
            | Self::PipeDeadlockDiagnosis
            | Self::SharedMemorySegments
            | Self::UdsPeerTopology => FeatureArea::IpcDbus,
            Self::PmuCounterAbstraction
            | Self::SyscallDistributionProfiling
            | Self::SlowSyscallTrap
            | Self::ProcessEventTrace
            | Self::CpuFlameGraph => FeatureArea::DynamicTracing,
            Self::MultiResolutionRingBuffer
            | Self::MultiFormatExport
            | Self::TimeTravelScrubber
            | Self::TelemetryPercentileAggregation
            | Self::HistoryChartImageExport => FeatureArea::HistoryTimeTravel,
        }
    }

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

/// One declared feature-to-support pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureCoverageEntry {
    pub feature: FeatureId,
    pub support: CapabilitySupport,
}

/// A frontend's explicit declaration of its feature coverage: one entry per
/// contract-known feature.
#[derive(Clone, Debug)]
pub struct FeatureCoverageDeclaration {
    pub frontend: FrontendShape,
    pub entries: Vec<FeatureCoverageEntry>,
}

/// The coverage outcome for one feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureCoverageStatus {
    /// An explicit support decision.
    Declared(CapabilitySupport),
    /// Contract-known but absent from the declaration - a silent omission.
    Missing,
    /// Declared more than once by the same frontend.
    Duplicated,
    /// Declared but outside the known feature set.
    Unknown,
}

impl FeatureCoverageStatus {
    /// Whether the feature carries an explicit decision - the only status a
    /// no-drift declaration may show.
    #[must_use]
    pub const fn is_explicit(self) -> bool {
        matches!(self, Self::Declared(_))
    }

    /// Whether this status is a drift finding a gate must reject.
    #[must_use]
    pub const fn is_drift(self) -> bool {
        !self.is_explicit()
    }
}

/// What a feature-coverage gate rejects, beyond the plain drift statuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureCoverageFindingKind {
    /// Contract-known feature absent from the declaration.
    Missing,
    /// Feature declared more than once.
    Duplicated,
    /// Declared feature outside the known set.
    Unknown,
    /// A non-reference shape declared `Reference` (reusing the GPUI-05 rule
    /// from the capability registry: only the GPUI shape owns reference
    /// semantics).
    ReferenceOutsideReferenceShape,
    /// The reference shape declared `Ported`/`Divergent`/`Native` - the
    /// shape that owns the semantics cannot port, diverge from, or delegate
    /// itself. It MAY declare [`CapabilitySupport::Unsupported`] ("reference
    /// surface not yet delivered"): the coverage matrix keeps every gap
    /// visible, and the reference shape honestly states its own undelivered
    /// items instead of fabricating a `Reference` claim.
    ReferenceShapeCannotPort,
    /// A `Native`/`Divergent`/`Unsupported` decision without its required
    /// supplier/reason text.
    EmptyExplanation,
}

/// One gate finding: the frontend, the feature, and what is wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureCoverageFinding {
    pub frontend: FrontendShape,
    pub feature: FeatureId,
    pub kind: FeatureCoverageFindingKind,
}

/// The coverage matrix for one declaration against the full feature set, in
/// canonical [`FeatureId::ALL`] order.
#[must_use]
pub fn feature_coverage_report(
    declaration: &FeatureCoverageDeclaration,
) -> Vec<(FeatureId, FeatureCoverageStatus)> {
    feature_coverage_report_over(declaration, FeatureId::ALL)
}

/// The drift findings alone.
#[must_use]
pub fn feature_coverage_drift(
    report: &[(FeatureId, FeatureCoverageStatus)],
) -> Vec<(FeatureId, FeatureCoverageStatus)> {
    report
        .iter()
        .copied()
        .filter(|(_, status)| status.is_drift())
        .collect()
}

/// The one-call gate payload: fold drift plus the shape-discipline rules
/// (reference role, reference-shape totality, non-empty explanations), reusing
/// the capability-registry semantics. An empty result is the only passing
/// state.
#[must_use]
pub fn feature_coverage_findings(
    declaration: &FeatureCoverageDeclaration,
) -> Vec<FeatureCoverageFinding> {
    let drift_kind = |status: FeatureCoverageStatus| match status {
        FeatureCoverageStatus::Missing => Some(FeatureCoverageFindingKind::Missing),
        FeatureCoverageStatus::Duplicated => Some(FeatureCoverageFindingKind::Duplicated),
        FeatureCoverageStatus::Unknown => Some(FeatureCoverageFindingKind::Unknown),
        FeatureCoverageStatus::Declared(_) => None,
    };
    let mut findings: Vec<FeatureCoverageFinding> = feature_coverage_report(declaration)
        .into_iter()
        .filter_map(|(feature, status)| {
            drift_kind(status).map(|kind| FeatureCoverageFinding {
                frontend: declaration.frontend,
                feature,
                kind,
            })
        })
        .collect();
    for (feature, status) in feature_coverage_report(declaration) {
        let FeatureCoverageStatus::Declared(support) = status else {
            continue;
        };
        let kind = if support == CapabilitySupport::Reference
            && !declaration.frontend.is_capability_reference_shape()
        {
            Some(FeatureCoverageFindingKind::ReferenceOutsideReferenceShape)
        } else if declaration.frontend.is_capability_reference_shape()
            && matches!(
                support,
                CapabilitySupport::Ported
                    | CapabilitySupport::Divergent { .. }
                    | CapabilitySupport::Native { .. }
            )
        {
            Some(FeatureCoverageFindingKind::ReferenceShapeCannotPort)
        } else if support.explanation().is_some_and(str::is_empty) {
            Some(FeatureCoverageFindingKind::EmptyExplanation)
        } else {
            None
        };
        if let Some(kind) = kind {
            findings.push(FeatureCoverageFinding {
                frontend: declaration.frontend,
                feature,
                kind,
            });
        }
    }
    findings.sort_by_key(|finding| finding.feature);
    findings.dedup();
    findings
}

/// The fold against a restricted known set - the seam that keeps the `Unknown`
/// path real (a declared feature outside `known` is reported, never silently
/// dropped).
fn feature_coverage_report_over(
    declaration: &FeatureCoverageDeclaration,
    known: &[FeatureId],
) -> Vec<(FeatureId, FeatureCoverageStatus)> {
    let mut report: Vec<(FeatureId, FeatureCoverageStatus)> = known
        .iter()
        .map(|feature| (*feature, FeatureCoverageStatus::Missing))
        .collect();
    for entry in &declaration.entries {
        match report
            .iter_mut()
            .find(|(feature, _)| *feature == entry.feature)
        {
            Some((_, status)) => {
                if status.is_explicit() || matches!(status, FeatureCoverageStatus::Duplicated) {
                    *status = FeatureCoverageStatus::Duplicated;
                } else {
                    *status = FeatureCoverageStatus::Declared(entry.support);
                }
            }
            None => report.push((entry.feature, FeatureCoverageStatus::Unknown)),
        }
    }
    report
}

#[cfg(test)]
#[path = "../tests/headless/ui_feature_coverage.rs"]
mod tests;
