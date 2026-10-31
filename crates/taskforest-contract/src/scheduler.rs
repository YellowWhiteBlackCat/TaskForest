//! Runtime-owned capability scheduling contract.
//!
//! The application owns request payloads and revisions. The platform runtime
//! owns only when a capability may run and whether a previous submission is
//! still active. This small contract lets the application translate an ECS
//! work plan into its existing typed request methods without exposing ECS or
//! native provider details to frontends.

use crate::{CapabilityId, RequestId, RequestScope};

/// External fact that may intentionally move a non-active capability back to
/// `Ready`. Neither trigger may replace an in-flight or stalled owner. The
/// runtime's own stall-abandonment deadline is a separate authority: after a
/// stalled owner has produced neither a terminal nor a progress outcome for
/// the configured `max_stalled_lifetime_ms`, the scheduler itself retires the
/// owner and requeues the route, so a lost executor cannot strand a
/// capability forever. Recovery triggers still never perform that retirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityRecoveryTrigger {
    /// A user explicitly retries a transient failure before its automatic
    /// retry deadline.
    ExplicitRetry,
    /// Permissions, dependencies, device identity, or another capability
    /// prerequisite changed outside the provider request lifecycle.
    CapabilityChanged,
}

/// Observable result of applying one recovery trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityRecoveryOutcome {
    /// The route is ready for exactly one normal typed submission.
    Ready,
    /// The current owner remains authoritative; no state changed.
    ActiveOwner,
    /// The route specifically requires `CapabilityChanged`, not blind retry.
    AwaitingCapabilityChange,
    /// The provider reported a failure that this runtime cannot recover.
    PermanentlyBlocked,
    /// No route owns the requested capability.
    UnknownCapability,
}

/// Maximum number of recent stall transitions retained in one diagnostics
/// snapshot. This is a transport bound, not a claim about active work count.
pub const MAX_RECENT_SCHEDULING_STALLS: usize = 64;

/// Maximum provider-panic notes retained in one diagnostics snapshot ring.
/// Like the stall ring this is a transport bound, not a rate claim.
pub const MAX_PROVIDER_PANIC_NOTES: usize = 8;

/// Character bound applied to one retained provider-panic message. Panic
/// payloads are unbounded text; the bounded diagnostic tail keeps only a
/// prefix of that size.
pub const MAX_PROVIDER_PANIC_MESSAGE_CHARS: usize = 256;

/// One bounded provider-panic diagnostic captured by the runtime's worker
/// isolation seam. The note is diagnostic only: the correlated publication
/// stays a typed `ProviderFault` failure, and this text never replaces it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderPanicNote {
    /// Name of the lane that isolated the panic.
    pub lane: String,
    /// Capability the panicking request addressed.
    pub capability: CapabilityId,
    /// Correlated request the panicking call served.
    pub request_id: RequestId,
    /// Downcast panic text, or the fixed `(non-string panic payload)`
    /// placeholder; truncated to [`MAX_PROVIDER_PANIC_MESSAGE_CHARS`].
    pub message: String,
    /// 1-based monotonic sequence of this panic within the runtime owner.
    pub sequence: u64,
}

/// Lifecycle partition affected by one observed scheduling stall.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchedulingScope {
    Capability,
    Target(RequestScope),
}

/// One bounded, implementation-neutral stalled-owner diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchedulingStall {
    pub capability: CapabilityId,
    pub request_id: RequestId,
    pub scope: SchedulingScope,
}

/// Stable product domain used to partition scheduling diagnostics without
/// exposing the runtime's ECS implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SchedulingDomain {
    System,
    Process,
    Storage,
    Service,
    Environment,
    Integration,
    Sensor,
    Power,
}

/// Cumulative work planned for one scheduling domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomainSchedulingSnapshot {
    pub domain: SchedulingDomain,
    pub planned_items: u64,
    pub active_targets: u64,
    pub active_target_limit: u64,
}

/// Current bounded-resource use and configured ceilings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SchedulingBudgetSnapshot {
    pub route_limit: u64,
    pub active_target_limit: u64,
    pub active_target_limit_per_capability: u64,
    pub active_target_limit_per_domain: u64,
    pub active_target_scope_bytes: u64,
    pub target_scope_byte_limit: u64,
    pub pending_deliveries: u64,
    pub pending_delivery_limit: u64,
    pub pending_control_deliveries: u64,
    pub pending_observation_deliveries: u64,
    pub control_delivery_reserve: u64,
    /// How long a stalled owner is retained for a possible late completion
    /// before the scheduler retires it and requeues the route.
    pub max_stalled_lifetime_ms: u64,
}

/// Pending event memory pressure observed at the bounded delivery boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventQueueSchedulingSnapshot {
    pub control_pending: u64,
    pub control_high_water: u64,
    pub observation_pending: u64,
    pub observation_high_water: u64,
    pub terminal_mailbox_pending: u64,
    pub terminal_mailbox_high_water: u64,
}

/// Exact admission-rejection counters retained by the scheduling runtime.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SchedulingAdmissionSnapshot {
    pub unknown_capability: u64,
    pub capability_in_flight: u64,
    pub capability_stalled: u64,
    pub capability_blocked: u64,
    pub duplicate_request: u64,
    pub target_in_flight: u64,
    pub target_capacity: u64,
    pub global_target_capacity: u64,
    pub domain_target_capacity: u64,
    pub target_scope_byte_capacity: u64,
    pub delivery_capacity: u64,
    pub control_delivery_capacity: u64,
    pub observation_delivery_capacity: u64,
    pub sideband_not_allowed: u64,
    pub invariant_violation: u64,
}

/// Bounded runtime scheduling diagnostics without exposing the ECS backend.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeSchedulingSnapshot {
    pub route_count: u64,
    pub active_target_jobs: u64,
    pub target_high_water: u64,
    pub ticks: u64,
    pub planned_items: u64,
    pub submissions: u64,
    pub completions: u64,
    pub requeues: u64,
    pub blocked: u64,
    pub stalled: u64,
    pub target_submissions: u64,
    pub target_completions: u64,
    pub target_cancellations: u64,
    pub target_stalled: u64,
    /// Owners currently quarantined in-place after lease expiry. They remain
    /// counted until their exact request publishes a terminal or progress
    /// outcome, or the runtime owner is dropped.
    pub active_stalled_capabilities: u64,
    pub active_stalled_targets: u64,
    /// Stalled owners that later produced an accepted terminal or progress
    /// publication without ever releasing ownership to a replacement.
    pub recovered_stalls: u64,
    pub target_recovered_stalls: u64,
    /// Stalled owners retired by the scheduler's own abandonment deadline
    /// (no terminal, no progress, no renewal inside the configured
    /// lifetime). Their delivery capacity is recycled and the route requeues.
    pub abandoned_stalls: u64,
    pub target_abandoned_stalls: u64,
    /// Terminal publications tolerated as stale: the publishing lane stayed
    /// alive, but the addressed owner had already been retired, so the
    /// publication changed no state.
    pub stale_terminal_publications: u64,
    /// Cumulative provider-lane thread exits since runtime construction.
    /// Teardown exits count too; any increment while the runtime runs marks
    /// a lane that stopped serving its capability family.
    pub worker_lane_exits: u64,
    /// Cumulative provider panics isolated by the worker catch-unwind seams.
    /// Saturating monotone counter; it never resets and is never fabricated
    /// from an empty diagnostic tail.
    pub provider_panics: u64,
    /// Most recent isolated provider panics, oldest first, bounded by
    /// [`MAX_PROVIDER_PANIC_NOTES`]. Each entry is context plus downcast
    /// payload text, not a typed failure replacement.
    pub recent_provider_panics: Vec<ProviderPanicNote>,
    /// Fixed-cardinality domain rollup. No request or target identity is
    /// retained here.
    pub domains: Vec<DomainSchedulingSnapshot>,
    pub budgets: SchedulingBudgetSnapshot,
    pub event_queues: EventQueueSchedulingSnapshot,
    pub admission: SchedulingAdmissionSnapshot,
    pub recent_stalls: Vec<SchedulingStall>,
}

/// Bounded scheduling seam between the platform runtime and application.
pub trait CapabilityScheduler: Send + Sync {
    /// Claim every capability whose ECS route is due according to the
    /// runtime-owned monotonic clock. `observed_at_wall_ms` is only the wall
    /// timestamp for any externally visible stalled-status update; it never
    /// advances cadence or leases. A returned capability is `Ready` until its
    /// typed request port claims the concrete request.
    fn poll_due(&self, observed_at_wall_ms: u64) -> Vec<CapabilityId>;

    /// Requeue a planned capability when the application could not construct
    /// or submit its typed request. The failure remains a scheduling outcome;
    /// it is never converted into a provider success.
    fn mark_submission_failed(&self, capability: &CapabilityId, failed_at_wall_ms: u64);

    /// Apply a user-selected cadence to one scheduled capability. `None`
    /// disables automatic scheduling while keeping explicit/manual requests
    /// valid through the typed port.
    fn set_cadence_ms(&self, capability: &CapabilityId, cadence_ms: Option<u64>);

    /// Apply an explicit recovery trigger without bypassing request ownership.
    /// Implementations must return [`CapabilityRecoveryOutcome::ActiveOwner`]
    /// for both in-flight and stalled work.
    fn request_recovery(
        &self,
        capability: &CapabilityId,
        trigger: CapabilityRecoveryTrigger,
    ) -> CapabilityRecoveryOutcome;

    /// Read a bounded diagnostic snapshot. Implementations must not expose an
    /// event log or retain unbounded target identities.
    fn scheduling_snapshot(&self) -> RuntimeSchedulingSnapshot;
}
