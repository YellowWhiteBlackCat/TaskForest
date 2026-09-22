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
            Self::MemoryBreakdownRssPss => FeatureSemanticSpec {
                feature: self,
                delivery_definition: "A memory surface renders the resident/shared/private and \
                                      virtual-memory breakdown facets of the shared memory \
                                      projection.",
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
        }
    }
}
