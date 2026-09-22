//! Feature semantic specifications: what counts as delivered.
//!
//! Every [`FeatureId`] carries one explicit, toolkit-neutral
//! [`FeatureSemanticSpec`] ([`FeatureId::semantic_spec`]) defining the
//! user-facing surface a frontend must render before it may claim
//! `Reference`/`Ported`/`Native`. The type and its fold live here so the
//! coverage registry stays within the source line budget.

use super::FeatureId;

/// Explicit, toolkit-neutral semantic specification for a product feature.
///
/// It defines what "delivered" means for the shared coverage registry: the
/// user-facing surface a frontend must render before it may claim
/// [`crate::capabilities::CapabilitySupport::Reference`],
/// [`crate::capabilities::CapabilitySupport::Ported`], or
/// [`crate::capabilities::CapabilitySupport::Native`]. A narrower surface
/// declares [`crate::capabilities::CapabilitySupport::Divergent`] with the
/// missing part; an absent one declares
/// [`crate::capabilities::CapabilitySupport::Unsupported`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureSemanticSpec {
    /// The feature described by this specification.
    pub feature: FeatureId,
    /// The shared surface that counts as delivered for this feature.
    pub delivery_definition: &'static str,
}

impl FeatureId {
    /// The explicit, toolkit-neutral semantic specification defining what
    /// counts as delivered for this feature.
    ///
    /// A frontend declaration is honest only when its status matches this
    /// definition: a surface that renders less than the specification must
    /// declare [`crate::capabilities::CapabilitySupport::Divergent`] with the
    /// missing part, and an absent one
    /// [`crate::capabilities::CapabilitySupport::Unsupported`].
    #[must_use]
    pub const fn semantic_spec(self) -> FeatureSemanticSpec {
        match self {
            Self::ProcessSchedulerPolicy => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A per-process surface renders the observed scheduler \
                                      policy class from the shared process projection and \
                                      offers the typed policy-change action.",
            },
            Self::ProcessPriorityMapping => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A per-process surface renders the normalized \
                                      nice/priority tier from the shared process projection and \
                                      offers the typed priority-change action.",
            },
            Self::ProcessAffinityMask => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A per-process surface renders the CPU affinity mask over \
                                      the shared logical-core topology and offers the typed \
                                      affinity-change action.",
            },
            Self::ProcessTreeKill => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A per-process tree surface renders the shared process tree \
                                      and offers the typed whole-tree control action; the \
                                      descendant set is resolved by the shared projection, never \
                                      by the renderer.",
            },
            Self::ProcessAncestorLineage => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A process surface renders the ancestor lineage (the parent \
                                      chain up to its root) from the shared process projection.",
            },
            Self::MemoryBreakdownRssPss => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A memory surface renders the resident (RSS), \
                                      proportional (PSS), and private/unique (USS) \
                                      physical-memory facets and the derived-shared \
                                      share (`RSS - USS`, `shared_bytes`) of the shared \
                                      memory projection. The virtual address-space size \
                                      (VmSize) is explicitly OUTSIDE this definition: \
                                      the shared projection carries no such observation, \
                                      and the per-process swap charge is a separate \
                                      fact; an unobserved facet stays an honest absence, \
                                      never a fabricated zero.",
            },
            Self::MemoryVmaMap => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A memory surface renders the per-process virtual memory \
                                      area (VMA) map from the shared projection.",
            },
            Self::MemoryLeakTrend => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A memory surface renders a long-term memory-leak trend \
                                      derived from the shared history projection.",
            },
            Self::MemoryPageFaults => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A memory surface renders the shared per-process minor and \
                                      major page-fault counters; a rate timeline is not part of \
                                      this definition.",
            },
            Self::MemoryTransparentHugePages => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A memory surface renders the shared per-process anonymous \
                                      transparent-huge-page charge; a system-wide coverage \
                                      percentage is outside this definition.",
            },
            Self::HandleEnumeration => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A per-process open-handle surface enumerates the \
                                      structured file descriptors from the shared projection.",
            },
            Self::HandleTypeClassification => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The open-handle surface classifies each descriptor by its \
                                      resolved handle type from the shared projection.",
            },
            Self::DeletedFileHandleWatch => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The open-handle surface marks descriptors whose backing \
                                      file was deleted while still held.",
            },
            Self::HandleFdLimitSaturation => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The open-handle surface renders the process's descriptor \
                                      count against its soft/hard open-file limits from the \
                                      shared projection.",
            },
            Self::HandleReversePathSearch => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A system-wide handle search surface resolves a path query \
                                      to the processes holding matching descriptors from the \
                                      shared handle projection.",
            },
            Self::ThreadTopologyEnumeration => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A per-process thread surface enumerates threads with their \
                                      per-thread resource statistics from the shared projection.",
            },
            Self::ThreadRunqueueLatency => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The thread surface renders the per-thread runqueue wait \
                                      latency from the shared projection.",
            },
            Self::ThreadContextSwitchRates => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The thread surface renders voluntary and involuntary \
                                      context-switch rates from the shared projection.",
            },
            Self::ThreadUninterruptibleSleepDiagnosis => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A thread/process surface flags uninterruptible-sleep \
                                      (D-state) threads and renders the system-level \
                                      uninterruptible count from the shared projection.",
            },
            Self::ThreadWaitChannelClassification => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The thread surface renders the classified per-thread wait \
                                      kind (kernel lock / uninterruptible I/O / run queue / \
                                      other) from the shared thread projection; the raw kernel \
                                      symbol is optional evidence, not part of this definition.",
            },
            Self::ProcessNetworkThroughput => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A per-process network surface renders Rx/Tx throughput \
                                      from the shared projection.",
            },
            Self::SocketInventory => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A network surface renders the TCP/UDP/Unix socket \
                                      inventory from the shared projection.",
            },
            Self::ListeningPortTopology => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The network surface renders listening-port topology with \
                                      conflict warnings from the shared projection.",
            },
            Self::SocketRttMetrics => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A per-process network surface renders each connection's \
                                      observed round-trip time (`rtt_ms`) from the shared \
                                      connection projection.",
            },
            Self::SocketQueueBacklog => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The network surface renders each socket's send/receive \
                                      queue depth from the shared connection projection.",
            },
            Self::ProcessLogicalPhysicalIo => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A per-process storage surface renders logical versus \
                                      physical I/O accounting from the shared projection.",
            },
            Self::DiskDeviceTopology => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A storage surface renders the system storage device and \
                                      partition topology from the shared projection.",
            },
            Self::DiskIopsQueueLatency => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The storage surface renders per-disk IOPS, queue depth, \
                                      and latency from the shared projection.",
            },
            Self::DiskSmartHealth => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A storage surface renders each disk's SMART health \
                                      evidence - reported availability, temperature, percentage \
                                      used, available spare, power-on hours, unsafe shutdowns - \
                                      from the shared device projection.",
            },
            Self::SwapThroughputRate => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A memory surface renders the system swap-in and swap-out \
                                      throughput rates from the shared memory projection.",
            },
            Self::HardwareTopologyTree => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A hardware surface renders the socket -> NUMA -> core -> \
                                      thread topology tree from the shared projection.",
            },
            Self::CpuCacheTopology => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A CPU surface renders the multi-level cache capacity \
                                      readout from the shared CPU metrics projection: the \
                                      observed L1d (`l1d_cache_kb`), L1i (`l1i_cache_kb`), \
                                      L2 (`l2_cache_kb`), and L3 (`l3_cache_kb`) capacities. \
                                      Cache-sharing topology between cores or NUMA nodes is \
                                      explicitly OUTSIDE this delivery definition: rendering \
                                      only the capacities counts as delivered, and an \
                                      unobserved capacity is an honest absence, never a zero.",
            },
            Self::NumaMemoryDistribution => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A hardware surface renders the per-NUMA-node memory \
                                      distribution and locality from the shared hardware \
                                      projection: each node's local memory capacity \
                                      (`local_memory_bytes`) and locality/hit ratio \
                                      (`numa_hit_ratio_pct`), keyed by node identity \
                                      (`numa_node_id`/`numa_node_ids`). A package-level or \
                                      aggregate memory total does NOT satisfy this definition; \
                                      per-node distribution is required.",
            },
            Self::CpuHeterogeneousCoreClass => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A CPU surface renders the heterogeneous core-class \
                                      breakdown (performance / efficiency / low-power core \
                                      counts) from the shared hardware projection.",
            },
            Self::CpuCoreFrequency => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A CPU surface renders the observed per-core clock frequency \
                                      from the shared CPU metrics projection.",
            },
            Self::GpuAdapterEnumeration => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "An accelerator surface enumerates the dGPU/iGPU adapters \
                                      from the shared projection.",
            },
            Self::NpuTelemetry => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "An accelerator surface renders native NPU inventory and \
                                      utilization telemetry from the shared projection.",
            },
            Self::GpuEngineUtilization => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The accelerator surface renders the per-engine GPU \
                                      utilization breakdown from the shared projection.",
            },
            Self::GpuMemoryReadout => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "An accelerator surface renders the shared per-device GPU \
                                      memory usage (observed used bytes against the reported \
                                      total) from the device projection. The dedicated-versus-\
                                      shared VRAM partition is explicitly OUTSIDE this \
                                      definition.",
            },
            Self::ProcessGpuAttribution => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A process surface attributes GPU memory and engine usage to \
                                      the owning process from the shared per-process GPU \
                                      projection.",
            },
            Self::RaplPowerDraw => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A power surface renders the Intel/AMD RAPL package power \
                                      draw from the shared projection.",
            },
            Self::ThermalZoneSensors => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A thermal surface traverses the system thermal-zone sensors \
                                      and names each reading's source from the shared projection.",
            },
            Self::CpuCStateAnalysis => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A CPU surface renders the cpuidle C-state residency/usage \
                                      evidence from the shared CPU projection.",
            },
            Self::BatteryPowerInventory => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A power surface renders the power-supply inventory with \
                                      each battery's charge, charge/discharge power, voltage, \
                                      health, and cycle count from the shared power-supply \
                                      projection; an unobserved field stays an honest absence.",
            },
            Self::ThermalThrottleEvents => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A thermal surface renders the CPU package \
                                      thermal-throttle trigger counters (the cumulative \
                                      package and per-core event counts, \
                                      `package_throttle_count` / `core_throttle_count`) \
                                      from the shared CPU projection. The real-time \
                                      PROCHOT assertion state (`is_throttled`) is \
                                      explicitly OUTSIDE this definition: no provider \
                                      populates it, and the aggregate system-health \
                                      deduction that consumes it is not a thermal \
                                      surface.",
            },
            Self::LinuxNamespaceAudit => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A security surface renders the Linux namespace audit \
                                      (per-kind host/isolated inode status) from the shared \
                                      projection.",
            },
            Self::PosixCapabilitiesAudit => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The security surface decodes the POSIX capability masks \
                                      and flags dangerous capabilities from the shared projection.",
            },
            Self::SeccompFilterAudit => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The security surface renders the seccomp syscall-filter \
                                      audit from the shared projection.",
            },
            Self::SandboxEnvironmentDetection => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A process surface renders the detected sandbox/container \
                                      identity (typed isolation kind plus container id) for the \
                                      selected process from the shared isolation projection.",
            },
            Self::ProcessMasqueradingDetection => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The process surface renders the executable-name versus \
                                      command-line identity mismatch warning from the shared \
                                      command-identity comparison.",
            },
            Self::SystemdDependencyDag => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A services surface renders the systemd dependency DAG, \
                                      including ordering-cycle detection, from the shared \
                                      projection.",
            },
            Self::ServiceLogStream => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A services surface renders the sd-journal service log \
                                      stream with level filtering from the shared projection.",
            },
            Self::ServiceFailureDiagnosis => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The services surface renders the service failure root-cause \
                                      diagnosis from the shared projection.",
            },
            Self::ServiceInventoryStatus => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A services surface renders the service inventory with each \
                                      unit's identity and typed active state from the shared \
                                      services projection.",
            },
            Self::ServiceLifecycleControl => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The services surface offers the typed lifecycle control \
                                      actions (start/stop/restart plus enable/disable) against \
                                      the selected unit from the shared services projection.",
            },
            Self::PsiMultiWindowTelemetry => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A pressure surface renders the PSI stall windows (some and \
                                      full across the 10s/60s/5m horizons) from the shared \
                                      pressure projection.",
            },
            Self::MemoryThrashingHealthScore => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A health surface renders the shared `SystemHealthScore`: \
                                      the transparent score plus its auditable deduction bill, \
                                      including the memory full-stall (thrashing) deduction \
                                      `HealthDeductionKind::MemoryFullStall`. A bare score with \
                                      no deduction bill, or a bill that omits the full-stall \
                                      deduction, does NOT satisfy this definition.",
            },
            Self::UseBottleneckAttribution => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A pressure surface renders USE-methodology bottleneck \
                                      attribution from the shared projection.",
            },
            Self::PressureLoadAverageNormalized => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A pressure surface renders the logical-processor-normalized \
                                      load average (1m/5m/15m) together with its stated \
                                      normalization basis from the shared host projection.",
            },
            Self::UnresponsiveAppDetection => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A process surface marks unresponsive foreground \
                                      applications from the shared responsiveness projection.",
            },
            Self::DbusServiceTopology => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "An IPC surface renders the D-Bus system/session service \
                                      topology from the shared projection.",
            },
            Self::DbusIntrospection => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The IPC surface browses D-Bus object introspection \
                                      (interfaces, methods, signals) from the shared projection.",
            },
            Self::PipeDeadlockDiagnosis => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The IPC surface renders the anonymous-pipe topology and \
                                      Tarjan deadlock diagnosis from the shared projection.",
            },
            Self::SharedMemorySegments => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "An IPC surface enumerates the POSIX/System V shared-memory \
                                      segments with their owners and attached processes from the \
                                      shared IPC projection.",
            },
            Self::UdsPeerTopology => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The IPC surface renders Unix-domain-socket peer topology \
                                      (listener and connected peers) from the shared IPC \
                                      projection.",
            },
            Self::PmuCounterAbstraction => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A profiling surface renders the hardware PMU counter \
                                      abstraction (cycles/instructions/branch/cache misses) from \
                                      the shared projection.",
            },
            Self::SyscallDistributionProfiling => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The profiling surface renders the per-process syscall \
                                      frequency/latency distribution from the shared projection.",
            },
            Self::SlowSyscallTrap => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The profiling surface renders slow-syscall trap records \
                                      from the shared projection.",
            },
            Self::ProcessEventTrace => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A tracing surface streams process fork/exec/exit events from \
                                      the shared event projection.",
            },
            Self::CpuFlameGraph => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "The tracing surface renders an interactive CPU flame graph \
                                      from the shared stack-sample projection.",
            },
            Self::MultiResolutionRingBuffer => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A history surface renders the multi-resolution ring-buffer \
                                      series over the shared history projection.",
            },
            Self::MultiFormatExport => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A history surface renders the structured multi-format export \
                                      engine over the shared projection.",
            },
            Self::TimeTravelScrubber => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A history surface renders the interactive time-travel \
                                      scrubber that restores the whole system snapshot to a \
                                      selected instant.",
            },
            Self::TelemetryPercentileAggregation => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A history surface renders percentile aggregates (P50/P90/P99) \
                                      over the shared history projection.",
            },
            Self::HistoryChartImageExport => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A history surface exports the rendered charts as a \
                                      vector/bitmap image from the shared history projection.",
            },
        }
    }
}
