//! Stable capability identifiers, runtime status, descriptors, and the read-only
//! catalog for typed application requests.
//!
//! Capability ownership belongs to the request type rather than to an OS
//! adapter, provider, or runtime channel.
//!
//! [`CapabilityId::EXPECTED_SURFACE`] is the product-expected capability
//! identity set: the capabilities the product promises to answer for on every
//! platform, whether the answer is a real observation or a typed absence. The
//! extensible [`CapabilityId`] remains open to vendor/diagnostic identities,
//! but those are not product promises. A platform that registers no source for
//! an expected capability must publish
//! [`CapabilityDescriptor::typed_absence`] - `Unsupported` with no provider
//! attribution - instead of omitting the capability silently.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use taskmanager_core::{FailureKind, ProviderId};

/// Extensible stable capability identifier.
///
/// Constants cover the shared product surface while native adapters may expose
/// additional diagnostic capabilities without requiring a vendor-specific
/// application build.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CapabilityId(Cow<'static, str>);

impl CapabilityId {
    pub const TELEMETRY_HOST: Self = Self::borrowed("telemetry.host");
    pub const TELEMETRY_CPU: Self = Self::borrowed("telemetry.cpu");
    pub const TELEMETRY_MEMORY: Self = Self::borrowed("telemetry.memory");
    pub const TELEMETRY_STORAGE: Self = Self::borrowed("telemetry.storage");
    pub const TELEMETRY_NETWORK: Self = Self::borrowed("telemetry.network");
    pub const TELEMETRY_GPU: Self = Self::borrowed("telemetry.gpu");
    /// On-demand per-engine GPU utilization rows via the privileged PMU
    /// helper seam (ADR-023, permission-model Boundary 2). Frontend-paced
    /// request/response lane: the unprivileged periodic path stays
    /// `telemetry.gpu`; this capability exists only where a provider can reach
    /// the PMU seam. Unregistered or pending adapters leave it absent/typed.
    pub const TELEMETRY_GPU_ENGINES: Self = Self::borrowed("telemetry.gpu.engines");
    /// On-demand SMBIOS memory slot/module inventory plus system/board
    /// identity facts via the privileged memory helper seam (ADR-023,
    /// permission-model Boundary 2).
    /// Frontend-paced request/response lane: the unprivileged periodic path
    /// (udev + world-readable DMI) stays `telemetry.memory`; this capability
    /// exists only where a provider can reach the SMBIOS helper seam, and the
    /// prompt fires only on an explicit user request.
    pub const TELEMETRY_MEMORY_SMBIOS: Self = Self::borrowed("telemetry.memory.smbios");
    /// On-demand CPU package power via the privileged RAPL helper seam
    /// (ADR-023, permission-model Boundary 2). The unprivileged periodic
    /// `power_w` projection stays on `telemetry.cpu`; this lane reads the
    /// root-only `energy_uj` counters through one bounded helper sample per
    /// explicit user request.
    pub const TELEMETRY_CPU_PACKAGE_POWER: Self = Self::borrowed("telemetry.cpu.package_power");
    /// On-demand CPU MSR readouts (package temperature, multipliers, Vcore)
    /// via the privileged MSR helper seam (ADR-023/048, permission-model
    /// Boundary 2). Frontend-paced request/response lane: the unprivileged
    /// periodic `telemetry.cpu` projections stay unchanged; this lane reads
    /// the root-only `/dev/cpu/N/msr` registers through one bounded helper
    /// invocation per explicit user request. Register fields the CPU does
    /// not implement stay typed-absent, never zero.
    pub const TELEMETRY_CPU_MSR: Self = Self::borrowed("telemetry.cpu.msr");
    /// NPU/AI accelerator device inventory (discovery-first).
    /// Enumerates accelerator devices with typed per-fact availability; live
    /// utilization stays typed inside the device model until a stable kernel
    /// interface exists. An empty device list is an honest no-NPU host, not a
    /// failure.
    pub const ACCELERATOR_NPU: Self = Self::borrowed("accelerator.npu");
    pub const HARDWARE_INVENTORY: Self = Self::borrowed("hardware.inventory");
    pub const CONTAINERS: Self = Self::borrowed("containers.rollup");
    pub const PROCESS_LIST: Self = Self::borrowed("process.list");
    pub const PROCESS_CONTROL: Self = Self::borrowed("process.control");
    pub const PROCESS_INSIGHTS_NETWORK: Self = Self::borrowed("process.insights.network");
    pub const PROCESS_INSIGHTS_GPU: Self = Self::borrowed("process.insights.gpu");
    pub const PROCESS_INSIGHTS_RESOURCES: Self = Self::borrowed("process.insights.resources");
    pub const PROCESS_INSIGHTS_ISOLATION: Self = Self::borrowed("process.insights.isolation");
    pub const PROCESS_INSIGHTS_THREADS: Self = Self::borrowed("process.insights.threads");
    pub const PROCESS_INSIGHTS_OPEN_FILES: Self = Self::borrowed("process.insights.open_files");
    pub const PROCESS_INSIGHTS_ENVIRONMENT: Self = Self::borrowed("process.insights.environment");
    pub const PROCESS_AFFINITY: Self = Self::borrowed("process.affinity");
    pub const PROCESS_AFFINITY_CONTROL: Self = Self::borrowed("process.affinity.control");
    pub const PROCESS_RESOURCE_CONTROL: Self = Self::borrowed("process.resource.control");
    /// System-level (no target): obtain `CAP_NET_RAW` via the OS-native prompt
    /// and restart the per-process byte-accounting capture with the escalated
    /// fd (ADR-023/024/025).
    pub const PROCESS_NETWORK_ESCALATION: Self = Self::borrowed("process.network.escalation");
    pub const SERVICES: Self = Self::borrowed("services");
    pub const SERVICE_DEPENDENCIES: Self = Self::borrowed("services.dependencies");
    pub const SERVICE_CONTROL: Self = Self::borrowed("services.control");
    pub const SERVICE_LOGS: Self = Self::borrowed("services.logs");
    pub const SERVICE_LOG_STREAM: Self = Self::borrowed("services.logs.stream");
    pub const STARTUP: Self = Self::borrowed("startup");
    pub const STARTUP_EVIDENCE: Self = Self::borrowed("startup.evidence");
    pub const STARTUP_CONTROL: Self = Self::borrowed("startup.control");
    pub const SESSIONS: Self = Self::borrowed("sessions");
    pub const SESSION_CONTROL: Self = Self::borrowed("sessions.control");
    pub const STORAGE_HEALTH: Self = Self::borrowed("storage.health");
    /// User-initiated directory usage analysis: bounded,
    /// cancellable directory scans with progress publications. An adapter
    /// without a provider does not leave the capability silently absent: the
    /// product surface publishes its typed absence
    /// ([`CapabilityDescriptor::typed_absence`]) until a real source
    /// registers.
    pub const DIRECTORY_USAGE: Self = Self::borrowed("filesystem.directory.usage");
    pub const SMART: Self = Self::borrowed("storage.smart");
    pub const SMART_CONTROL: Self = Self::borrowed("storage.smart.control");
    pub const SENSORS: Self = Self::borrowed("sensors");
    pub const POWER_SUPPLIES: Self = Self::borrowed("hardware.power-supplies");
    pub const COMMAND_LAUNCH: Self = Self::borrowed("shell.command.launch");
    pub const RESOURCE_REVEAL: Self = Self::borrowed("shell.resource.reveal");
    pub const URL_OPEN: Self = Self::borrowed("shell.url.open");
    pub const DESKTOP_APPEARANCE: Self = Self::borrowed("desktop.appearance");
    /// Deliver a desktop notification for a fired alert (extension capability).
    /// The request carries a de-duplication instance id so
    /// upstream gating (cooldown/quiet hours) stays pure and testable.
    pub const DESKTOP_NOTIFY: Self = Self::borrowed("alerts.notify");
    /// Mission Center-compatible first-run setup-script discovery and actions.
    /// This is deliberately separate from arbitrary shell command launch:
    /// native adapters must expose a fixed, auditable setup asset and helper.
    pub const FIRST_RUN_SETUP: Self = Self::borrowed("first-run.setup");

    // ── Deep-surface capability families (mostly Linux-only) ─────────────
    //
    // These identities exist so the deepest facts of the 225/Wave3/Wave4
    // surface have a product-level address instead of "no capability at all";
    // they include one windowing-system probe (`desktop.responsiveness`) that
    // is not OS-kernel specific. None of them is registered by a platform
    // adapter yet; until the matching request lane lands, every platform
    // publishes the product-surface typed absence
    // ([`CapabilityDescriptor::typed_absence`]): `Unsupported`, no provider
    // attribution, no fabricated value. The platform that eventually owns the
    // source registers a real route and upgrades only itself; where the source
    // sits behind the per-feature escalation seam (ADR-023), the adapter
    // publishes [`CapabilityStatus::RequiresEscalation`] instead.

    /// Per-domain Linux pressure-stall information (`/proc/pressure/*`, PSI) as
    /// its own capability lane. Host telemetry already carries a bounded
    /// pressure field; this identity addresses the dedicated pressure/stall
    /// surface, which Windows/macOS cannot source and therefore answer
    /// typed-absent.
    pub const TELEMETRY_PRESSURE: Self = Self::borrowed("telemetry.pressure");
    /// GPU hang/reset events (kernel TDR/hang reports) as a distinct
    /// reliability fact from utilization telemetry.
    pub const TELEMETRY_GPU_HANG: Self = Self::borrowed("telemetry.gpu.hang");
    /// Hardware thermal-throttle trigger counters (PROCHOT / `thermal_throttle`
    /// sysfs), distinct from temperature readouts.
    pub const TELEMETRY_CPU_THROTTLE: Self = Self::borrowed("telemetry.cpu.throttle");
    /// Per-process virtual-memory-area map (`smaps`/`smaps_rollup`):
    /// proportional-resident and per-mapping accounting.
    pub const MEMORY_VMA_MAP: Self = Self::borrowed("memory.vma-map");
    /// Swap compression facts (`zswap`, `zram`) and their device-side effect.
    pub const MEMORY_COMPRESSION: Self = Self::borrowed("memory.compression");
    /// Transparent huge-page accounting (anonymous and shmem huge pages).
    pub const MEMORY_HUGEPAGES: Self = Self::borrowed("memory.hugepages");
    /// Advisory file locks (`/proc/locks`) with owner and range identity.
    pub const FILESYSTEM_FILE_LOCKS: Self = Self::borrowed("filesystem.file-locks");
    /// Handles whose backing file was deleted while the descriptor stayed open.
    pub const FILESYSTEM_DELETED_HANDLES: Self = Self::borrowed("filesystem.deleted-handles");
    /// Descriptor-table limits (`RLIMIT_NOFILE` soft/hard) and their pressure.
    pub const FILESYSTEM_FD_LIMITS: Self = Self::borrowed("filesystem.fd-limits");
    /// Per-process scheduler policy/priority and I/O scheduling class facts.
    pub const PROCESS_SCHEDULING: Self = Self::borrowed("process.scheduling");
    /// Per-process out-of-memory criticality (`oom_score`/`oom_score_adj`).
    pub const PROCESS_OOM_SCORE: Self = Self::borrowed("process.oom-score");
    /// Per-thread context-switch accounting (voluntary and involuntary).
    pub const THREADS_CONTEXT_SWITCH: Self = Self::borrowed("threads.context-switch");
    /// Why a thread is not running (kernel wait channel / futex wait state).
    pub const THREADS_WAIT_CHANNEL: Self = Self::borrowed("threads.wait-channel");
    /// System-wide socket inventory via the kernel socket diagnostics
    /// interface, including unix-domain sockets.
    pub const NETWORK_SOCKET_INVENTORY: Self = Self::borrowed("network.socket-inventory");
    /// Control lane: destroy one local socket (an escalation-bound reset).
    pub const NETWORK_SOCKET_CONTROL: Self = Self::borrowed("network.socket-control");
    /// Traffic shaping / rate-limit state for interfaces and cgroups.
    pub const NETWORK_TRAFFIC_CONTROL: Self = Self::borrowed("network.traffic-control");
    /// Per-process logical versus physical I/O accounting.
    pub const STORAGE_IO_ACCOUNTING: Self = Self::borrowed("storage.io-accounting");
    /// Per-process I/O scheduling priority (best-effort class and level).
    pub const STORAGE_IO_PRIORITY: Self = Self::borrowed("storage.io-priority");
    /// Dirty-page writeback state and its pressure on storage latency.
    pub const STORAGE_WRITEBACK: Self = Self::borrowed("storage.writeback");
    /// NUMA node topology, distance, and per-node memory statistics.
    pub const NUMA_TOPOLOGY: Self = Self::borrowed("numa.topology");
    /// CPU idle-state (C-state) residency and latency facts.
    pub const POWER_C_STATES: Self = Self::borrowed("power.c-states");
    /// Platform power-profile state and switching (power-profiles D-Bus).
    pub const POWER_PROFILES: Self = Self::borrowed("power.profiles");
    /// Scheduled service timers and their next-elapse facts.
    pub const SERVICES_TIMERS: Self = Self::borrowed("services.timers");
    /// Socket-activated service units and their activation state.
    pub const SERVICES_SOCKET_ACTIVATION: Self = Self::borrowed("services.socket-activation");
    /// System/session bus topology, object introspection, and call rates.
    pub const IPC_DBUS: Self = Self::borrowed("ipc.dbus");
    /// POSIX inter-process objects (shared memory, message queues, semaphores).
    pub const IPC_POSIX: Self = Self::borrowed("ipc.posix");
    /// System V inter-process objects (shm, msg, sem).
    pub const IPC_SYSV: Self = Self::borrowed("ipc.sysv");
    /// Anonymous pipe topology between processes, including wait-for cycles.
    pub const IPC_PIPE_GRAPH: Self = Self::borrowed("ipc.pipe-graph");
    /// Hardware performance counters via the PMU (`perf_event_open`); the
    /// source is escalation-bound on hosts that gate PMU access.
    pub const PROFILING_PMU: Self = Self::borrowed("profiling.pmu");
    /// Off-CPU time attribution for scheduling and blocking analysis.
    pub const PROFILING_OFF_CPU: Self = Self::borrowed("profiling.off-cpu");
    /// Syscall frequency/latency distribution and slow-syscall traps.
    pub const PROFILING_SYSCALLS: Self = Self::borrowed("profiling.syscalls");
    /// Foreground-window responsiveness probes (hung/unresponsive application
    /// detection); the probe mechanism is windowing-system specific.
    pub const DESKTOP_RESPONSIVENESS: Self = Self::borrowed("desktop.responsiveness");
    /// Process lifecycle event capture (fork/exec/exit connector); the source
    /// is escalation-bound where the kernel requires a privileged listener.
    pub const HISTORY_PROCESS_EVENTS: Self = Self::borrowed("history.process-events");

    #[must_use]
    pub const fn borrowed(value: &'static str) -> Self {
        Self(Cow::Borrowed(value))
    }

    #[must_use]
    pub fn owned(value: impl Into<String>) -> Self {
        Self(Cow::Owned(value.into()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    /// Every capability identity the product promises to answer for on every
    /// platform: real observation where a source exists, typed absence
    /// otherwise.
    ///
    /// This is a product fact, not a platform claim. The array is the single
    /// authority for "which capabilities must appear in a runtime catalog at
    /// all"; the runtime catalog seeds one typed-absence descriptor per entry
    /// and lets a platform's real registration replace it. Vendor/diagnostic
    /// identities created with [`Self::borrowed`]/[`Self::owned`] remain
    /// supported but are deliberately outside the product promise.
    ///
    /// Every product capability constant must be listed here; the contract
    /// surface census and the per-platform catalog contracts fail when a
    /// registered capability is not product-expected.
    pub const EXPECTED_SURFACE: [Self; 81] = [
        Self::TELEMETRY_HOST,
        Self::TELEMETRY_CPU,
        Self::TELEMETRY_MEMORY,
        Self::TELEMETRY_STORAGE,
        Self::TELEMETRY_NETWORK,
        Self::TELEMETRY_GPU,
        Self::TELEMETRY_GPU_ENGINES,
        Self::TELEMETRY_MEMORY_SMBIOS,
        Self::TELEMETRY_CPU_PACKAGE_POWER,
        Self::TELEMETRY_CPU_MSR,
        Self::ACCELERATOR_NPU,
        Self::HARDWARE_INVENTORY,
        Self::CONTAINERS,
        Self::PROCESS_LIST,
        Self::PROCESS_CONTROL,
        Self::PROCESS_INSIGHTS_NETWORK,
        Self::PROCESS_INSIGHTS_GPU,
        Self::PROCESS_INSIGHTS_RESOURCES,
        Self::PROCESS_INSIGHTS_ISOLATION,
        Self::PROCESS_INSIGHTS_THREADS,
        Self::PROCESS_INSIGHTS_OPEN_FILES,
        Self::PROCESS_INSIGHTS_ENVIRONMENT,
        Self::PROCESS_AFFINITY,
        Self::PROCESS_AFFINITY_CONTROL,
        Self::PROCESS_RESOURCE_CONTROL,
        Self::PROCESS_NETWORK_ESCALATION,
        Self::SERVICES,
        Self::SERVICE_DEPENDENCIES,
        Self::SERVICE_CONTROL,
        Self::SERVICE_LOGS,
        Self::SERVICE_LOG_STREAM,
        Self::STARTUP,
        Self::STARTUP_EVIDENCE,
        Self::STARTUP_CONTROL,
        Self::SESSIONS,
        Self::SESSION_CONTROL,
        Self::STORAGE_HEALTH,
        Self::DIRECTORY_USAGE,
        Self::SMART,
        Self::SMART_CONTROL,
        Self::SENSORS,
        Self::POWER_SUPPLIES,
        Self::COMMAND_LAUNCH,
        Self::RESOURCE_REVEAL,
        Self::URL_OPEN,
        Self::DESKTOP_APPEARANCE,
        Self::DESKTOP_NOTIFY,
        Self::FIRST_RUN_SETUP,
        Self::TELEMETRY_PRESSURE,
        Self::TELEMETRY_GPU_HANG,
        Self::TELEMETRY_CPU_THROTTLE,
        Self::MEMORY_VMA_MAP,
        Self::MEMORY_COMPRESSION,
        Self::MEMORY_HUGEPAGES,
        Self::FILESYSTEM_FILE_LOCKS,
        Self::FILESYSTEM_DELETED_HANDLES,
        Self::FILESYSTEM_FD_LIMITS,
        Self::PROCESS_SCHEDULING,
        Self::PROCESS_OOM_SCORE,
        Self::THREADS_CONTEXT_SWITCH,
        Self::THREADS_WAIT_CHANNEL,
        Self::NETWORK_SOCKET_INVENTORY,
        Self::NETWORK_SOCKET_CONTROL,
        Self::NETWORK_TRAFFIC_CONTROL,
        Self::STORAGE_IO_ACCOUNTING,
        Self::STORAGE_IO_PRIORITY,
        Self::STORAGE_WRITEBACK,
        Self::NUMA_TOPOLOGY,
        Self::POWER_C_STATES,
        Self::POWER_PROFILES,
        Self::SERVICES_TIMERS,
        Self::SERVICES_SOCKET_ACTIVATION,
        Self::IPC_DBUS,
        Self::IPC_POSIX,
        Self::IPC_SYSV,
        Self::IPC_PIPE_GRAPH,
        Self::PROFILING_PMU,
        Self::PROFILING_OFF_CPU,
        Self::PROFILING_SYSCALLS,
        Self::DESKTOP_RESPONSIVENESS,
        Self::HISTORY_PROCESS_EVENTS,
    ];

    /// Whether this identity belongs to the product-expected surface.
    ///
    /// Membership is identity, never availability: an expected capability can
    /// be typed-absent on every platform. Unknown or vendor identities answer
    /// `false` instead of panicking.
    #[must_use]
    pub fn is_expected(&self) -> bool {
        Self::EXPECTED_SURFACE.contains(self)
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Static capability ownership for one typed application request.
///
/// The association belongs to the request type rather than to an OS adapter,
/// provider, or runtime channel. Native composition can therefore construct a
/// typed request lane without repeating a stringly capability identifier.
///
/// Test adapters and third-party capability extensions remain supported: a
/// local request type can implement this trait with its own stable,
/// platform-neutral [`CapabilityId`].
pub trait CapabilityRequest: Send + 'static {
    const CAPABILITY: CapabilityId;

    /// Whether this request family admits lifecycle-borrowing sideband
    /// messages. The default is fail-closed; only an audited idempotent
    /// request contract may opt in.
    const SIDEBAND_POLICY: SidebandPolicy = SidebandPolicy::Denied;

    /// Select the runtime lifecycle authority for this concrete request.
    ///
    /// Most facets allow only one in-flight request for the whole capability.
    /// Targeted jobs may instead name a stable target scope, while sideband
    /// control messages deliberately borrow the lifecycle of an existing job.
    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        Ok(RequestTracking::Capability)
    }
}

/// Admission policy for request messages that deliberately own no terminal
/// lifecycle of their own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SidebandPolicy {
    #[default]
    Denied,
    /// The sideband operation is idempotent even when its addressed owner is
    /// absent or has already retired.
    Idempotent,
}

/// Maximum UTF-8 wire size of one independently tracked target identity.
///
/// This is a product transport bound rather than an operating-system path
/// claim. Four KiB accommodates normal cross-platform paths and provider IDs
/// while preventing any one lifecycle key from dominating bounded runtime
/// storage. Larger native identities are rejected honestly before queue or ECS
/// admission; they are never truncated or hashed.
pub const MAX_REQUEST_SCOPE_BYTES: usize = 4 * 1024;

/// Why a request could not declare a trustworthy bounded lifecycle scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RequestTrackingError {
    MissingTargetIdentity,
    EmptyTargetScope,
    TargetScopeTooLong {
        actual_bytes: usize,
        max_bytes: usize,
    },
}

impl fmt::Display for RequestTrackingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTargetIdentity => formatter.write_str("target identity is unavailable"),
            Self::EmptyTargetScope => formatter.write_str("target scope is empty"),
            Self::TargetScopeTooLong {
                actual_bytes,
                max_bytes,
            } => write!(
                formatter,
                "target scope is {actual_bytes} bytes; maximum is {max_bytes} bytes"
            ),
        }
    }
}

impl std::error::Error for RequestTrackingError {}

/// Stable, opaque identity of one independently tracked runtime target.
///
/// The application owns how domain identity is encoded. The runtime only uses
/// the owned value for equality, ordering, and bounded admission; it never
/// interprets it as an OS locator.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestScope(Arc<str>);

impl RequestScope {
    pub fn try_owned(value: String) -> Result<Self, RequestTrackingError> {
        validate_request_scope(&value)?;
        Ok(Self(Arc::from(value)))
    }

    pub fn try_from_str(value: &str) -> Result<Self, RequestTrackingError> {
        validate_request_scope(value)?;
        Ok(Self(Arc::from(value)))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_request_scope(value: &str) -> Result<(), RequestTrackingError> {
    if value.is_empty() {
        return Err(RequestTrackingError::EmptyTargetScope);
    }
    let actual_bytes = value.len();
    if actual_bytes > MAX_REQUEST_SCOPE_BYTES {
        return Err(RequestTrackingError::TargetScopeTooLong {
            actual_bytes,
            max_bytes: MAX_REQUEST_SCOPE_BYTES,
        });
    }
    Ok(())
}

impl TryFrom<String> for RequestScope {
    type Error = RequestTrackingError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_owned(value)
    }
}

impl TryFrom<&str> for RequestScope {
    type Error = RequestTrackingError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from_str(value)
    }
}

/// Runtime lifecycle ownership requested by one typed payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestTracking {
    /// One in-flight request owns the entire capability.
    Capability,
    /// One in-flight request owns only this stable target scope.
    Target(RequestScope),
    /// An audited idempotent control message with no terminal lifecycle of its
    /// own. Runtime policy must explicitly allow the capability; bounded lane
    /// admission still applies, including when the addressed job has already
    /// ended or never existed.
    Sideband,
}

/// Runtime support state for one capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CapabilityStatus {
    Available,
    /// The capability produced a usable partial observation while at least one
    /// independently fallible source failed.
    Degraded(FailureKind),
    Unsupported,
    /// The capability needs a permission decision the current process cannot
    /// grant itself, and no escalation offer is part of this state. This is the
    /// capability-level twin of a hard `core::FailureKind::PermissionDenied`;
    /// when the per-feature escalation seam can still reach the data it is
    /// [`Self::RequiresEscalation`] instead, so consumers can tell the two
    /// states apart.
    PermissionRequired,
    /// The capability is absent because the unprivileged process lacks a
    /// privilege that the per-feature escalation seam (ADR-023,
    /// permission-model Boundary 2) can reach through the OS-native prompt.
    ///
    /// This is the capability-level twin of
    /// `core::FailureKind::RequiresEscalation`. It is deliberately not folded
    /// into [`Self::PermissionRequired`]: only this state proves that an
    /// escalation affordance exists, so a frontend may offer the one explicit
    /// prompt while never fabricating a value in the meantime.
    RequiresEscalation,
    MissingDependency,
    TemporarilyUnavailable,
    Stale,
}

/// Published public-commitment words for the capability-availability axis.
///
/// This array is a projection of [`CapabilityStatus`], not a second vocabulary:
/// the enum variants are the sole authority, and
/// `public_degradation_words_match_the_authoritative_enum` fails if a rename
/// ever desynchronizes these literals from the real variant names. The public
/// docs (`docs/RELEASE.md`, `docs/GLOSSARY.md`,
/// `docs/CROSSPLATFORM_STRATEGY.md`) must spell degradation reasons with these
/// exact words.
pub const PUBLIC_CAPABILITY_DEGRADATION_WORDS: [&str; 5] = [
    "Unsupported",
    "PermissionRequired",
    "RequiresEscalation",
    "MissingDependency",
    "TemporarilyUnavailable",
];

/// The permission word that belongs to the failure-reason axis rather than the
/// capability-availability axis.
///
/// `PermissionDenied` names `taskmanager_core::FailureKind::PermissionDenied`
/// (and `ProviderFailure::PermissionDenied`); the capability-level gate is
/// spelled [`CapabilityStatus::PermissionRequired`]. Published docs cite this
/// literal only when naming a failure reason so the two axes stay visibly
/// distinct.
pub const PUBLIC_FAILURE_AXIS_PERMISSION_WORD: &str = "PermissionDenied";

/// Runtime description of one independently usable capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityDescriptor {
    pub id: CapabilityId,
    pub status: CapabilityStatus,
    pub providers: Vec<ProviderId>,
    pub observed_at_ms: u64,
    pub last_success_at_ms: Option<u64>,
}

impl CapabilityDescriptor {
    /// The honest descriptor for a product-expected capability that this
    /// platform did not register.
    ///
    /// This is the single authority for the absence shape: `Unsupported`
    /// (no qualified source on this platform), no provider attribution, no
    /// observation time, and no last success. It is never a fabricated zero
    /// and never a runtime transient: callers must not confuse it with a
    /// registered-pending provider, which carries a real provider identity
    /// and a typed initial status.
    #[must_use]
    pub fn typed_absence(id: CapabilityId) -> Self {
        Self {
            id,
            status: CapabilityStatus::Unsupported,
            providers: Vec::new(),
            observed_at_ms: 0,
            last_success_at_ms: None,
        }
    }

    /// Whether this descriptor is the product-surface typed absence.
    ///
    /// The two decisive fields are the status and the empty provider
    /// attribution: a registered source always names its provider, so an
    /// `Unsupported` descriptor with a provider is a registered-pending lane,
    /// not an unregistered absence.
    #[must_use]
    pub fn is_typed_absence(&self) -> bool {
        self.status == CapabilityStatus::Unsupported && self.providers.is_empty()
    }
}

/// Deterministic runtime capability inventory.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySnapshot {
    entries: BTreeMap<CapabilityId, CapabilityDescriptor>,
}

impl CapabilitySnapshot {
    #[must_use]
    pub fn from_descriptors(descriptors: impl IntoIterator<Item = CapabilityDescriptor>) -> Self {
        let entries = descriptors
            .into_iter()
            .map(|descriptor| (descriptor.id.clone(), descriptor))
            .collect();
        Self { entries }
    }

    #[must_use]
    pub fn get(&self, id: &CapabilityId) -> Option<&CapabilityDescriptor> {
        self.entries.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &CapabilityDescriptor> {
        self.entries.values()
    }

    /// Every capability a real source registered on this platform.
    ///
    /// Registered descriptors always name their provider, so this iterator
    /// excludes the product-surface typed absence.
    pub fn registered(&self) -> impl Iterator<Item = &CapabilityDescriptor> {
        self.iter()
            .filter(|descriptor| !descriptor.is_typed_absence())
    }

    /// Every product-expected capability this platform did not register.
    ///
    /// These entries are the typed answer "no source here", never a runtime
    /// transient and never a fabricated value.
    pub fn typed_absences(&self) -> impl Iterator<Item = &CapabilityDescriptor> {
        self.iter()
            .filter(|descriptor| descriptor.is_typed_absence())
    }
}

/// Read-only capability source used by frontends and application policy.
pub trait CapabilityCatalog: Send + Sync {
    fn snapshot(&self) -> CapabilitySnapshot;
}
