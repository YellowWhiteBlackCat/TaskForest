//! Semantic delivery definitions for Areas 1-5 (batch A).

use crate::feature_coverage::FeatureId;

#[must_use]
pub const fn delivery_definition(feature: FeatureId) -> Option<&'static str> {
    let def = match feature {
        FeatureId::ProcessSchedulerPolicy => {
            "A per-process surface renders the observed scheduler \
                                      policy class from the shared process projection and \
                                      offers the typed policy-change action."
        }
        FeatureId::ProcessPriorityMapping => {
            "A per-process surface renders the normalized \
                                      nice/priority tier from the shared process projection and \
                                      offers the typed priority-change action."
        }
        FeatureId::ProcessEfficiencyMode => {
            "A per-process surface renders the observed efficiency \
                                      mode from the shared process projection and offers the \
                                      typed efficiency-mode control action."
        }
        FeatureId::ProcessAffinityMask => {
            "A per-process surface renders the CPU affinity mask over \
                                      the shared logical-core topology and offers the typed \
                                      affinity-change action."
        }
        FeatureId::ProcessTreeKill => {
            "A per-process tree surface renders the shared process tree \
                                      and offers the typed whole-tree control action; the \
                                      descendant set is resolved by the shared projection, never \
                                      by the renderer."
        }
        FeatureId::ProcessTwoPhaseTermination => {
            "A process control surface executes two-phase graceful \
                                      process termination, dispatching a graceful shutdown \
                                      signal followed by automatic escalation to hard termination \
                                      if the process does not exit within a bounded timeout."
        }
        FeatureId::ProcessCgroupResourceLimiting => {
            "A process resource management surface renders and allows \
                                      configuring cgroups v2 resource limits (such as cpu.max, \
                                      memory.high, and memory.max) for the selected process \
                                      cgroup hierarchy."
        }
        FeatureId::ProcessOomScore => {
            "A per-process inspection surface renders the observed \
                                      OOM score from the shared process projection, keeping \
                                      unsupported platforms as an honest dash."
        }
        FeatureId::ProcessTrimWorkingSet => {
            "A per-process surface triggers active physical memory working-set \
                                      trimming over the shared process control lane, reporting \
                                      typed Unsupported on non-supported platforms."
        }
        FeatureId::ProcessCoreDump => {
            "A per-process diagnostics surface generates on-demand crash core dumps / \
                                      minidumps over the shared process control lane with modal \
                                      confirmation."
        }
        FeatureId::ProcessAutoRestartRule => {
            "A process supervisor surface registers daemon-level auto-restart rules \
                                      triggered upon observed process exit events from the shared \
                                      history event stream."
        }
        FeatureId::ProcessFreezeThawControl => {
            "A process control surface offers typed suspend (freeze) \
                                      and resume (thaw) actions for selected processes."
        }
        FeatureId::ProcessCustomLabel => {
            "A per-process surface assigns and persists custom category labels, \
                                      highlight badges, and alias tags over the shared process \
                                      projection."
        }
        FeatureId::ProcessAncestorLineage => {
            "A process surface renders the ancestor lineage (the parent \
                                      chain up to its root) from the shared process projection."
        }
        FeatureId::ProcessCompoundFilter => {
            "A process table surface parses and evaluates compound \
                                      search expressions with typed qualifiers (pid:, user:, \
                                      status:, cmd:, name:) over the shared process projection."
        }
        FeatureId::MemoryBreakdownRssPss => {
            "A memory surface renders the resident (RSS), \
                                      proportional (PSS), and private/unique (USS) \
                                      physical-memory facets and the derived-shared \
                                      share (`RSS - USS`, `shared_bytes`) of the shared \
                                      memory projection. The virtual address-space size \
                                      (VmSize) is explicitly OUTSIDE this definition: \
                                      the shared projection carries no such observation, \
                                      and the per-process swap charge is a separate \
                                      fact; an unobserved facet stays an honest absence, \
                                      never a fabricated zero."
        }
        FeatureId::MemoryProcessSwapCharge => {
            "A per-process surface renders the swap charged to the \
                                      process from the shared process-row projection \
                                      (`swap_bytes`), kept distinct from RSS/PSS and from the \
                                      system swap-in/out throughput rates; an unobserved charge \
                                      renders the shared dash, never a fabricated zero."
        }
        FeatureId::MemorySmapsRollupTiering => {
            "A memory inspection surface provides two-tier virtual memory \
                                      accounting, streaming lightweight rollup summaries during \
                                      routine polling and lazily resolving detailed smaps \
                                      area metrics on drilldown."
        }
        FeatureId::MemoryVmaMap => {
            "A memory surface renders the per-process virtual memory \
                                      area (VMA) map from the shared projection."
        }
        FeatureId::SharedMemorySegmentCrossAudit => {
            "A memory forensics surface cross-references shared memory \
                                      segments across process boundaries, correlating POSIX \
                                      shm, SysV IPC, and memfd mappings with multiple PID \
                                      references."
        }
        FeatureId::MemoryDirtyPagePressure => {
            "A memory telemetry surface computes private and shared dirty \
                                      memory accumulation against writeback pressure thresholds, \
                                      alerting on uncommitted storage flush backlogs."
        }
        FeatureId::MemoryLeakTrend => {
            "A memory surface renders a long-term memory-leak trend \
                                      derived from the shared history projection."
        }
        FeatureId::MemoryPageFaults => {
            "A memory surface renders the shared per-process minor and \
                                      major page-fault counters; a rate timeline is not part of \
                                      this definition."
        }
        FeatureId::MemoryCompressionZswap => {
            "A memory performance surface renders compressed swap facts \
                                      (zram used/capacity/ratio) and zswap cache enablement."
        }
        FeatureId::MemoryTransparentHugePages => {
            "A memory surface renders the shared per-process anonymous \
                                      transparent-huge-page charge; a system-wide coverage \
                                      percentage is outside this definition."
        }
        FeatureId::UserspaceAllocatorInference => {
            "A memory diagnostics surface inspects address space layout \
                                      signatures to infer runtime allocator heap topologies, \
                                      arena counts, and fragmentation metrics."
        }
        FeatureId::WindowsCommitKernelPools => {
            "A Windows memory telemetry surface breaks down commit charge \
                                      limits, paged pool residency, and non-paged kernel pool \
                                      allocations from platform memory descriptors."
        }
        FeatureId::MacosMemoryClassification => {
            "A macOS memory telemetry surface provides classified physical memory \
                                      breakdowns, rendering compressed, wired, and resident \
                                      page allocations."
        }
        FeatureId::MemoryReadOnlyStringProbing => {
            "A memory forensic surface provides safe read-only memory string \
                                      and signature probing with strict privilege boundaries \
                                      and non-intrusive safety guards."
        }
        FeatureId::VmaSnapshotDiff => {
            "A memory analysis surface compares two virtual memory address \
                                      space snapshots, highlighting newly mapped regions, \
                                      permission mutations, and released segments."
        }
        FeatureId::CodeSectionIntegrityAudit => {
            "A memory security surface audits mapped executable code sections \
                                      against on-disk binary baselines, detecting unauthorized \
                                      runtime code modifications."
        }
        FeatureId::HandleEnumeration => {
            "A per-process open-handle surface enumerates the \
                                      structured file descriptors from the shared projection."
        }
        FeatureId::HandleTypeClassification => {
            "The open-handle surface classifies each descriptor by its \
                                      resolved handle type from the shared projection."
        }
        FeatureId::FileLocksAudit => {
            "A storage or open files inspection surface renders active \
                                      kernel advisory and lease file locks (owner PID, lock \
                                      type, and byte range), identifying locked and exclusive \
                                      file handles."
        }
        FeatureId::DeletedFileHandleWatch => {
            "The open-handle surface marks descriptors whose backing \
                                      file was deleted while still held."
        }
        FeatureId::HandleFdLimitSaturation => {
            "The open-handle surface renders the process's descriptor \
                                      count against its soft/hard open-file limits from the \
                                      shared projection."
        }
        FeatureId::PipePeerPidLookup => {
            "A handle diagnostics surface cross-references anonymous \
                                      pipe inode identities across all processes to resolve \
                                      and link pipe writer and reader peer PIDs."
        }
        FeatureId::UdsPeerAssociation => {
            "A handle diagnostics surface associates Unix-domain socket \
                                      file descriptors with their peer socket endpoints and \
                                      connected partner processes."
        }
        FeatureId::DeviceNodeHandleInspection => {
            "A handle inspection surface classifies character and block \
                                      device node handles, distinguishing GPU, hardware \
                                      acceleration, and virtualization devices."
        }
        FeatureId::HandleReversePathSearch => {
            "A system-wide handle search surface resolves a path query \
                                      to the processes holding matching descriptors from the \
                                      shared handle projection."
        }
        FeatureId::EmergencyCloseHandleAction => {
            "A privileged process control surface offers an emergency \
                                      handle closure action with modal confirmation and safety \
                                      safeguards."
        }
        FeatureId::HandleLeakTrendAnalysis => {
            "A resource limits diagnostics surface evaluates rolling \
                                      descriptor and handle allocations to warn against monotonic \
                                      handle leakage."
        }
        FeatureId::WindowsAccessTokenAudit => {
            "A security inspection surface decodes Windows process \
                                      access tokens, user security identifiers (SIDs), and \
                                      elevated privilege token flags."
        }
        FeatureId::MemfdSealsAudit => {
            "A memory handle surface inspects anonymous memory files \
                                      (memfd_create) and decodes active seal flags (F_SEAL_WRITE, \
                                      F_SEAL_GROW, F_SEAL_SHRINK)."
        }
        FeatureId::MountNamespaceRootLookup => {
            "A filesystem namespace surface resolves process root \
                                      mount namespace boundaries, contrasting containerized \
                                      paths against host system roots."
        }
        FeatureId::EpollSubscriptionHotspots => {
            "A handle inspection surface monitors epoll and kqueue event-loop \
                                      subscriptions, rendering monitored file descriptor counts, \
                                      event interest masks, and readiness hotspots."
        }
        FeatureId::ThreadTopologyEnumeration => {
            "A per-process thread surface enumerates threads with their \
                                      per-thread resource statistics from the shared projection."
        }
        FeatureId::ThreadRunqueueLatency => {
            "The thread surface renders the per-thread runqueue wait \
                                      latency from the shared projection."
        }
        FeatureId::ThreadContextSwitchRates => {
            "The thread surface renders voluntary and involuntary \
                                      context-switch rates from the shared projection."
        }
        FeatureId::ThreadUninterruptibleSleepDiagnosis => {
            "A thread/process surface flags uninterruptible-sleep \
                                      (D-state) threads and renders the system-level \
                                      uninterruptible count from the shared projection."
        }
        FeatureId::ThreadWaitChannelClassification => {
            "The thread surface renders the classified per-thread wait \
                                      kind (kernel lock / uninterruptible I/O / run queue / \
                                      other) from the shared thread projection; the raw kernel \
                                      symbol is optional evidence, not part of this definition."
        }
        FeatureId::ThreadCallstackSampling => {
            "A thread diagnostic surface captures safe user-space and kernel \
                                      callstack snapshots, resolving symbolized call chains \
                                      with privilege-aware graceful degradation."
        }
        FeatureId::ThreadFutexContention => {
            "A thread synchronization surface diagnoses futex wait parameters \
                                      and lock contention rates, attributing waiting threads to \
                                      candidate lock owners."
        }
        FeatureId::ThreadStackLayoutGuard => {
            "A thread memory surface audits stack allocation boundaries, \
                                      active stack consumption depth, and guard page \
                                      protection against overflow."
        }
        FeatureId::ThreadSchedulingNiceAffinity => {
            "A thread execution control surface inspects and orchestrates \
                                      individual thread nice priorities and CPU core affinity \
                                      masks."
        }
        FeatureId::ThreadPoolWorkerHeuristics => {
            "A thread analysis surface classifies active pool workers versus \
                                      idle standby threads via CPU rate changes and state \
                                      transitions."
        }
        FeatureId::ThreadSuspendResumeControl => {
            "A thread control surface provides targeted execution suspension \
                                      and resumption for individual threads with modal safety \
                                      confirmation."
        }
        FeatureId::ThreadZombieDetection => {
            "A thread diagnostic surface detects unreaped zombie threads \
                                      and abnormal thread exits, alerting on thread lifecycle \
                                      leaks."
        }
        FeatureId::ThreadLocalStorageAudit => {
            "A thread memory surface audits thread-local storage (TLS) area \
                                      sizing, estimating static TLS blocks and dynamic per-thread \
                                      storage allocations."
        }
        FeatureId::ThreadChurnWarning => {
            "A thread diagnostic surface analyzes thread lifecycle churn rates, \
                                      flagging rapid short-lived thread turnover bursts and \
                                      antipattern thread allocation spikes."
        }
        FeatureId::ThreadCpuSliceHeatmap => {
            "A thread performance surface renders normalized per-thread CPU \
                                      contribution slices and core distribution ratios from \
                                      aggregate execution sampling telemetry."
        }
        FeatureId::ProcessNetworkThroughput => {
            "A per-process network surface renders Rx/Tx throughput \
                                      from the shared projection."
        }
        FeatureId::SocketInventory => {
            "A network surface renders the TCP/UDP/Unix socket \
                                      inventory from the shared projection."
        }
        FeatureId::SocketStateTracking => {
            "A process network connection surface renders the typed \
                                      TCP connection state from the shared process network \
                                      projection."
        }
        FeatureId::ListeningPortTopology => {
            "The network surface renders listening-port topology with \
                                      conflict warnings from the shared projection."
        }
        FeatureId::SocketRttMetrics => {
            "A per-process network surface renders each connection's \
                                      observed round-trip time (`rtt_ms`) from the shared \
                                      connection projection."
        }
        FeatureId::SocketQueueBacklog => {
            "The network surface renders each socket's send/receive \
                                      queue depth from the shared connection projection."
        }
        FeatureId::DnsReverseLookup => {
            "A network monitoring surface performs non-blocking \
                                      asynchronous reverse PTR DNS resolution with LRU caching \
                                      to present human-readable remote domain names."
        }
        FeatureId::GeoIpAsnLookup => {
            "A network inspection surface identifies remote IP geographical \
                                      location (country/city) and autonomous system number \
                                      (ASN/ISP) from local lookup databases."
        }
        FeatureId::SocketConnectionAuditFeed => {
            "A network diagnostics surface streams real-time connection \
                                      establishment and teardown audit events from socket \
                                      delta tracking."
        }
        FeatureId::MaliciousConnectionDetection => {
            "A network security surface flags anomalous outbound ports, \
                                      suspicious endpoints, and heuristic network threat \
                                      indicators across active sockets."
        }
        FeatureId::NetworkSocketReset => {
            "A network connection management surface offers a typed \
                                      emergency socket reset/drop action for selected open \
                                      sockets, draining through the kernel socket control lane."
        }
        FeatureId::NetworkTrafficShaping => {
            "A process or cgroup network surface renders traffic control \
                                      classification and bandwidth rate-limit constraints from the \
                                      platform network traffic control lane."
        }
        FeatureId::ApplicationProtocolInference => {
            "A network inspection surface infers application-layer \
                                      protocols (such as HTTP, HTTPS, SSH, DNS, and Redis) \
                                      from destination port numbers and socket characteristics."
        }
        FeatureId::OrphanSocketLeakDetection => {
            "A network diagnostics surface identifies half-closed \
                                      (CLOSE_WAIT) connection leaks and unassociated orphan \
                                      sockets lacking an active owner process."
        }
        FeatureId::NetworkRollingBandwidth => {
            "A network performance surface renders real-time rolling \
                                      Rx and Tx bandwidth curves over time from observed network \
                                      telemetry, distinguishing receiving and transmitting throughput rates."
        }
        _ => return None,
    };
    Some(def)
}
