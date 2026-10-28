//! Semantic delivery definitions for Areas 11-15 (batch C).

use crate::feature_coverage::FeatureId;

#[must_use]
pub const fn delivery_definition(feature: FeatureId) -> &'static str {
    match feature {
        FeatureId::SystemdDependencyDag => {
            "A services surface renders the systemd dependency DAG, \
                                      including ordering-cycle detection, from the shared \
                                      projection."
        }
        FeatureId::ServiceLogStream => {
            "A services surface renders the sd-journal service log \
                                      stream with level filtering from the shared projection."
        }
        FeatureId::ServiceFailureDiagnosis => {
            "The services surface renders the service failure root-cause \
                                      diagnosis from the shared projection."
        }
        FeatureId::ServiceInventoryStatus => {
            "A services surface renders the service inventory with each \
                                      unit's identity and typed active state from the shared \
                                      services projection."
        }
        FeatureId::UserServiceAggregation => {
            "A services surface enumerates and aggregates both \
                                      system-level service units and user session service units \
                                      under a unified supervisor view with explicit scope indicators."
        }
        FeatureId::ServiceStateMachine => {
            "A services surface renders granular supervisor state \
                                      machine (load state, active state, sub-state) facts."
        }
        FeatureId::ServiceUnitFileState => {
            "A service details surface renders the unit file state \
                                      (enabled, disabled, static, masked) from the shared \
                                      service diagnostics projection."
        }
        FeatureId::ServiceLifecycleControl => {
            "The services surface offers the typed lifecycle control \
                                      actions (start/stop/restart plus enable/disable) against \
                                      the selected unit from the shared services projection."
        }
        FeatureId::ServiceCgroupResourceLimits => {
            "A service inspection surface renders the unit's observed \
                                      cgroup memory usage and ceiling from supervisor diagnostics."
        }
        FeatureId::ServiceRestartChurn => {
            "A service details surface renders service restart churn \
                                      indicators (restart count and start-limit-hit flag) from \
                                      the shared service diagnostics projection."
        }
        FeatureId::ServiceTimerSchedule => {
            "A service details surface renders the next timer trigger \
                                      schedule from the shared service diagnostics projection."
        }
        FeatureId::ServiceSocketActivation => {
            "A service details surface renders socket activation \
                                      relationships (triggering and triggered-by unit lists) \
                                      from the shared service diagnostics projection."
        }
        FeatureId::ServiceUnitFileDropIns => {
            "A service details surface displays unit configuration file \
                                      locations alongside active drop-in configuration overrides \
                                      (drop-ins), facilitating unit troubleshooting."
        }
        FeatureId::StartupImpactAssessment => {
            "A startup surface renders every startup item with its \
                                      measured boot-impact tier and duration when the provider \
                                      observed it, and a typed unknown reason otherwise; it never \
                                      fabricates a duration or a tier."
        }
        FeatureId::StartupItemControl => {
            "A startup surface exposes a reversible enable/disable \
                                      control per item; the action passes through the shared \
                                      typed confirmation and platform intent, and the source \
                                      configuration is soft-disabled, never deleted."
        }
        FeatureId::PsiMultiWindowTelemetry => {
            "A pressure surface renders the PSI stall windows (some and \
                                      full across the 10s/60s/5m horizons) from the shared \
                                      pressure projection."
        }
        FeatureId::MemoryThrashingHealthScore => {
            "A health surface renders the shared `SystemHealthScore`: \
                                      the transparent score plus its auditable deduction bill, \
                                      including the memory full-stall (thrashing) deduction \
                                      `HealthDeductionKind::MemoryFullStall`. A bare score with \
                                      no deduction bill, or a bill that omits the full-stall \
                                      deduction, does NOT satisfy this definition."
        }
        FeatureId::GlobalFdPoolCapacity => {
            "A system pressure or resource limits surface renders \
                                      system-wide file descriptor allocation against the kernel maximum \
                                      limit, diagnosing kernel file table saturation."
        }
        FeatureId::UseBottleneckAttribution => {
            "A pressure surface renders USE-methodology bottleneck \
                                      attribution from the shared projection."
        }
        FeatureId::PressureLoadAverageNormalized => {
            "A pressure surface renders the logical-processor-normalized \
                                      load average (1m/5m/15m) together with its stated \
                                      normalization basis from the shared host projection."
        }
        FeatureId::UnresponsiveAppDetection => {
            "A process surface marks unresponsive foreground \
                                      applications from the shared responsiveness projection."
        }
        FeatureId::PressureSaturationGrade => {
            "A health surface renders the shared red/yellow/green \
                                      saturation grade derived from the transparent system \
                                      health score and keeps an unobserved score as an honest \
                                      unknown rather than green."
        }
        FeatureId::SoftirqSaturationAnalysis => {
            "A system pressure surface monitors hardware IRQ and software \
                                      interrupt (softirq) CPU saturation, flagging bottlenecks \
                                      such as network receive processing."
        }
        FeatureId::PressureDStateRatio => {
            "A process surface renders the uninterruptible-sleep (D-state) \
                                      task count together with its share of the observed process \
                                      inventory; a real zero remains visible and an absent \
                                      inventory has no fabricated denominator."
        }
        FeatureId::FilesystemLockLatency => {
            "A storage saturation surface evaluates filesystem metadata \
                                      lock and Inode lock contention delays from storage pressure \
                                      metrics."
        }
        FeatureId::MemoryCompactionStall => {
            "A memory saturation surface tracks memory compaction stalls \
                                      and transparent huge-page allocation latency spikes from \
                                      kernel pressure telemetry."
        }
        FeatureId::OomImminentWarning => {
            "A system resilience surface evaluates memory thrashing and \
                                      direct page-scan surges to warn of imminent kernel \
                                      OOM-killer invocations before system freeze."
        }
        FeatureId::IpcBufferSaturation => {
            "An IPC diagnostics surface monitors buffer filling ratios \
                                      and pipe/queue backpressure saturation across inter-process \
                                      communication channels."
        }
        FeatureId::PressureNativeAlertIntegration => {
            "A desktop product drains typed alert-notification requests \
                                      through its native notification port and keeps the same \
                                      alert state visible in the tray/surface when delivery is \
                                      disabled, unavailable, or rate-limited."
        }
        FeatureId::DbusServiceTopology => {
            "An IPC surface renders the D-Bus system/session service \
                                      topology from the shared projection."
        }
        FeatureId::DbusIntrospection => {
            "The IPC surface browses D-Bus object introspection \
                                      (interfaces, methods, signals) from the shared projection."
        }
        FeatureId::DbusTrafficMonitoring => {
            "An IPC monitoring surface tracks real-time D-Bus message \
                                      flow, method call rates, and signal broadcast frequencies \
                                      across system and session buses."
        }
        FeatureId::SharedMemorySegments => {
            "An IPC surface enumerates the POSIX/System V shared-memory \
                                      segments with their owners and attached processes from the \
                                      shared IPC projection."
        }
        FeatureId::PosixSysvMessageQueues => {
            "An IPC diagnostics surface enumerates POSIX and System V \
                                      message queues, queue depth, unread message counts, and \
                                      maximum capacity limits."
        }
        FeatureId::NamedSemaphoresInventory => {
            "An IPC inspection surface enumerates system-wide POSIX and \
                                      System V named semaphores with value counts and holding \
                                      process associations."
        }
        FeatureId::PipeGraphVisualization => {
            "An IPC diagnostic surface models and renders the \
                                      cross-process anonymous pipe communication graph, mapping \
                                      pipe writers directly to their reader peers."
        }
        FeatureId::UdsPeerTopology => {
            "The IPC surface renders Unix-domain-socket peer topology \
                                      (listener and connected peers) from the shared IPC \
                                      projection."
        }
        FeatureId::WindowsNamedPipes => {
            "An IPC inspection surface enumerates Windows named pipe \
                                      and mailslot endpoints, server instances, client PIDs, \
                                      and duplex channel modes."
        }
        FeatureId::PipeDeadlockDiagnosis => {
            "The IPC surface renders the anonymous-pipe topology and \
                                      Tarjan deadlock diagnosis from the shared projection."
        }
        FeatureId::KernelEventNotifiers => {
            "An IPC telemetry surface maps cross-process asynchronous \
                                      notification primitives including eventfd counters and \
                                      signalfd descriptor bindings."
        }
        FeatureId::SharedMemorySecurityAudit => {
            "An IPC security surface audits POSIX and System V shared \
                                      memory segment file permissions, flagging world-readable \
                                      or unconfined global segments."
        }
        FeatureId::DbusActivationLatency => {
            "An IPC diagnostics surface measures on-demand D-Bus service \
                                      auto-activation launch duration and response times for \
                                      registered activatable units."
        }
        FeatureId::OrphanIpcReclamation => {
            "An IPC cleanup surface detects orphaned unattached shared \
                                      memory segments and zero-consumer semaphores, exposing \
                                      typed resource reclamation."
        }
        FeatureId::TopIpcThroughputProducers => {
            "An IPC forensics surface aggregates and ranks top \
                                      communicating processes by byte throughput across D-Bus, \
                                      anonymous pipes, and local domain sockets."
        }
        FeatureId::PmuCounterAbstraction => {
            "A profiling surface renders the hardware PMU counter \
                                      abstraction (cycles/instructions/branch/cache misses) from \
                                      the shared projection."
        }
        FeatureId::SyscallDistributionProfiling => {
            "The profiling surface renders the per-process syscall \
                                      frequency/latency distribution from the shared projection."
        }
        FeatureId::SlowSyscallTrap => {
            "The profiling surface renders slow-syscall trap records \
                                      from the shared projection."
        }
        FeatureId::OffCpuBlockingAnalysis => {
            "A dynamic profiling surface renders off-CPU time attribution \
                                      and blocking duration breakdowns, distinguishing runqueue \
                                      waiting from kernel lock and I/O sleeps."
        }
        FeatureId::ProcessFileActivityTrace => {
            "A dynamic tracing surface streams real-time process file activity \
                                      events, recording file open, read, write, and close operations \
                                      with microsecond timestamps."
        }
        FeatureId::SocketLifecycleAuditStream => {
            "A dynamic tracing surface audits network socket lifecycle transitions, \
                                      streaming connect, accept, send, and close events with \
                                      endpoint attribution."
        }
        FeatureId::ProcessEventTrace => {
            "A tracing surface streams process fork/exec/exit events from \
                                      the shared event projection."
        }
        FeatureId::CpuFlameGraph => {
            "The tracing surface renders an interactive CPU flame graph \
                                      from the shared stack-sample projection."
        }
        FeatureId::DynamicAllocationProfiling => {
            "A dynamic profiling surface traces memory allocation hotspots, \
                                      associating heap growth rates and allocation bursts with \
                                      symbolized call chains."
        }
        FeatureId::MutexContentionProfiler => {
            "A dynamic profiling surface measures mutex lock contention durations, \
                                      ranking synchronization bottlenecks by cumulative thread \
                                      wait latency."
        }
        FeatureId::PageFaultCallchainBacktrace => {
            "A dynamic tracing surface localizes major and minor page-fault occurrences, \
                                      attributing kernel fault triggers to symbolized user and \
                                      kernel callchains."
        }
        FeatureId::ContextSwitchTriggerAnalysis => {
            "A dynamic tracing surface categorizes voluntary versus involuntary \
                                      context-switch triggers, identifying lock waits, I/O sleeps, \
                                      and scheduler preemption."
        }
        FeatureId::FlightRecordingModeHook => {
            "A dynamic tracing surface activates low-overhead in-memory flight \
                                      recording with ring-buffered retention for post-incident \
                                      diagnostics."
        }
        FeatureId::TraceDataStandardExport => {
            "A dynamic tracing surface exports recorded execution events into \
                                      standard inter-tool trace formats including Chrome Trace \
                                      Event and Perfetto protobufs."
        }
        FeatureId::DynamicTracingCapabilityGate => {
            "A dynamic tracing surface audits runtime tracing privileges and kernel \
                                      security restrictions, probing BPF, perf_event_open, and \
                                      ptrace capabilities."
        }
        FeatureId::MultiResolutionRingBuffer => {
            "A history surface renders the multi-resolution ring-buffer \
                                      series over the shared history projection."
        }
        FeatureId::MultiFormatExport => {
            "A history surface renders the structured multi-format export \
                                      engine over the shared projection."
        }
        FeatureId::TimeTravelScrubber => {
            "A history surface renders the interactive time-travel \
                                      scrubber that restores the whole system snapshot to a \
                                      selected instant."
        }
        FeatureId::GhostProcessesCapture => {
            "A history analytics surface captures ephemeral and short-lived \
                                      ghost processes, retaining exited process identities, \
                                      runtimes, and exit codes for post-mortem analysis."
        }
        FeatureId::SystemSnapshotDiff => {
            "A history inspection surface computes full-system state differentials \
                                      between arbitrary timestamps, contrasting process sets, \
                                      resource totals, and system health."
        }
        FeatureId::BlackboxCrashPreservation => {
            "A history diagnostics surface automatically preserves pre-crash \
                                      blackbox telemetry to durable storage when anomalous \
                                      shutdown or fault thresholds are detected."
        }
        FeatureId::OfflineSessionRecording => {
            "A history persistence surface serializes telemetry sessions to compact \
                                      .tmrec recording files and loads historical records for \
                                      cross-machine analysis."
        }
        FeatureId::TelemetryPercentileAggregation => {
            "A history surface renders percentile aggregates (P50/P90/P99) \
                                      over the shared history projection."
        }
        FeatureId::EventTimelineBookmarks => {
            "A history annotation surface allows bookmarking key timestamps, \
                                      attaching custom incident labels, and jumping between \
                                      timeline markers."
        }
        FeatureId::LeaderboardRankEvolution => {
            "A history playback surface animates process resource consumption \
                                      leaderboard rank shifts and standing changes across \
                                      time series windows."
        }
        FeatureId::BenchmarkAbComparison => {
            "A history benchmarking surface executes automated performance \
                                      comparisons between baseline and trial workloads, \
                                      highlighting metric regressions."
        }
        FeatureId::HistoryChartImageExport => {
            "A history surface exports the rendered charts as a \
                                      vector/bitmap image from the shared history projection."
        }
        FeatureId::VariableSpeedReplayEngine => {
            "A history replay surface drives multi-speed playback simulation \
                                      (0.5x, 1x, 2x, 5x) across recorded telemetry streams \
                                      with play, pause, and rate controls."
        }
        FeatureId::EnterprisePrivacyRedaction => {
            "A history compliance surface applies rule-based privacy data \
                                      redaction to exported session audits, sanitizing usernames, \
                                      command arguments, and IP addresses."
        }
        FeatureId::HistoryLttbDownsampling => {
            "Every history chart uses the shared gap-preserving LTTB \
                                      projection when its samples exceed the render budget: \
                                      finite runs keep both endpoints and representative peaks, \
                                      original time positions stay ordered, and a missing-data \
                                      interval is never connected across."
        }
        _ => "unspecified",
    }
}
