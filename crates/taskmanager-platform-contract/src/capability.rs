//! Stable capability identifiers, runtime status, descriptors, and the read-only
//! catalog for typed application requests.
//!
//! Capability ownership belongs to the request type rather than to an OS
//! adapter, provider, or runtime channel.

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
    /// cancellable directory scans with progress publications. An absent
    /// provider (e.g. Windows adapter) simply leaves this capability absent.
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
    PermissionRequired,
    MissingDependency,
    TemporarilyUnavailable,
    Stale,
}

/// Runtime description of one independently usable capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityDescriptor {
    pub id: CapabilityId,
    pub status: CapabilityStatus,
    pub providers: Vec<ProviderId>,
    pub observed_at_ms: u64,
    pub last_success_at_ms: Option<u64>,
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
}

/// Read-only capability source used by frontends and application policy.
pub trait CapabilityCatalog: Send + Sync {
    fn snapshot(&self) -> CapabilitySnapshot;
}
