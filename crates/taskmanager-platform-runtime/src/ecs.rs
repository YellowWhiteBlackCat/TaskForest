//! Mandatory Bevy ECS runtime kernel for capability work scheduling.
//!
//! This module is deliberately narrower than the platform runtime itself. It
//! models when a capability may be submitted and whether its previous work is
//! in flight; it does not own provider facts, OS I/O, request revisions, or
//! the application projection. The existing typed ports and lanes remain the
//! execution boundary beneath the typed contracts.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, LockResult, Mutex, MutexGuard};

use bevy_app::prelude::{App, Update};
use bevy_ecs::component::Component;
use bevy_ecs::prelude::{Entity, Resource};
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use taskmanager_application::{
    CapabilityId, DomainSchedulingSnapshot, MAX_RECENT_SCHEDULING_STALLS, ProviderId, RequestId,
    RequestScope, RuntimeSchedulingSnapshot, SchedulingAdmissionSnapshot, SchedulingBudgetSnapshot,
    SchedulingDomain, SchedulingScope, SchedulingStall, SidebandPolicy,
};

use crate::config::{CapabilityRoute, DeliveryClass, RuntimeBudgets, RuntimeDomain};

#[cfg(test)]
#[path = "../tests/headless/ecs_abandonment.rs"]
mod abandonment_tests;
#[cfg(test)]
#[path = "../tests/headless/ecs_benchmark.rs"]
mod benchmark;
mod domain;
mod lifecycle;
#[cfg(test)]
#[path = "../tests/headless/ecs_replay.rs"]
mod replay;
mod scheduling_systems;
#[cfg(test)]
#[path = "../tests/headless/ecs_state_machine.rs"]
mod state_machine;
mod target_jobs;
#[cfg(test)]
#[path = "../tests/headless/ecs_scheduler.rs"]
mod tests;

use target_jobs::{
    TargetJobRegistry, abandon_stalled_target_jobs_system, mark_stalled_target_jobs_system,
};

const DEFAULT_RETRY_INTERVAL_MS: u64 = 1_000;
pub(crate) const DEFAULT_IN_FLIGHT_LEASE_MS: u64 = 30_000;

#[derive(Component)]
struct CapabilityNode {
    capability: CapabilityId,
    provider: ProviderId,
    delivery: crate::config::DeliveryClass,
    domain: RuntimeDomain,
    cadence_ms: Option<u64>,
    sideband_policy: SidebandPolicy,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum WorkState {
    Waiting,
    Ready,
    InFlight {
        request_id: RequestId,
        deadline_ms: u64,
    },
    /// The worker has not produced a terminal publication inside its lease.
    /// The original request remains authoritative and a late completion may
    /// still recover it; new submissions stay rejected to prevent overlap.
    /// Past `abandon_at_ms` the scheduler itself retires the owner (see
    /// [`StallPolicy`]), because an executor that stopped without publishing
    /// must not strand the capability forever.
    Stalled {
        request_id: RequestId,
        abandon_at_ms: u64,
    },
    Blocked(BlockedReason),
}

/// Active lifecycle phase that owned an accepted completion or lease renewal.
///
/// Keeping this phase typed prevents a late completion from being flattened
/// into an unrelated boolean and makes recovered-stall accounting exhaustive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnedWorkPhase {
    InFlight,
    Stalled,
}

impl OwnedWorkPhase {
    const fn recovered_stall_count(self) -> u64 {
        match self {
            Self::InFlight => 0,
            Self::Stalled => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockedReason {
    Permanent,
    AwaitingCapabilityChange,
}

/// Exact reason an ECS lifecycle claim was rejected.
///
/// The channel boundary may intentionally collapse recoverable contention to
/// the public `Busy` vocabulary, while diagnostics and headless behavior tests
/// retain the precise scheduler cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EcsAdmissionError {
    UnknownCapability,
    CapabilityInFlight,
    CapabilityStalled,
    CapabilityBlocked,
    DuplicateRequest,
    TargetInFlight,
    TargetCapacity,
    GlobalTargetCapacity,
    DomainTargetCapacity,
    TargetScopeByteCapacity,
    ControlDeliveryCapacity,
    ObservationDeliveryCapacity,
    SidebandNotAllowed,
    InvariantViolation,
}

/// Lifecycle owner accepted for one terminal provider publication.
///
/// The verdict deliberately exposes no Bevy entity. Callers only need to know
/// whether the terminal publication belonged to the capability route or to an
/// independently tracked target job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompletionOwner {
    Capability,
    Target,
}

/// Exact reason a terminal publication could not complete ECS lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompletionRejection {
    UnknownCapability,
    InactiveOwner,
    RequestMismatch,
    InvariantViolation,
}

/// Typed result of validating and applying one terminal publication.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompletionVerdict {
    Accepted(CompletionOwner),
    Rejected(CompletionRejection),
}

impl CompletionVerdict {
    #[must_use]
    pub(crate) const fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted(_))
    }
}

#[derive(Component, Clone, Copy)]
struct DueAt(u64);

#[derive(Resource, Default)]
struct SchedulerClock {
    monotonic_now_ms: u64,
}

#[derive(Resource, Default)]
struct DueWork {
    items: Vec<EcsWorkItem>,
}

#[derive(Resource, Default)]
struct StalledWork {
    subjects: Vec<StalledSubject>,
}

/// Scheduler-owned stall retention window, mirrored from
/// `RuntimeBudgets::max_stalled_lifetime_ms` for the ECS systems.
#[derive(Resource, Clone, Copy, Debug)]
struct StallPolicy {
    lifetime_ms: u64,
}

/// Scheduler-owned retry backoff, mirrored from the scheduler's
/// `retry_interval_ms` for the ECS systems.
#[derive(Resource, Clone, Copy, Debug)]
struct RetryIntervalMs(u64);

#[derive(Resource, Default)]
struct AbandonedWork {
    subjects: Vec<StalledSubject>,
}

/// Exact lifecycle partition whose observation lease expired.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StalledSubject {
    Capability {
        capability: CapabilityId,
        request_id: RequestId,
    },
    Target {
        capability: CapabilityId,
        request_id: RequestId,
        scope: RequestScope,
    },
}

#[derive(Resource, Default)]
struct StallDiagnostics {
    recent: VecDeque<StalledSubject>,
}

impl StallDiagnostics {
    fn record(&mut self, subject: StalledSubject) {
        if self.recent.len() == MAX_RECENT_SCHEDULING_STALLS {
            self.recent.pop_front();
        }
        self.recent.push_back(subject);
    }
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EcsDiagnostics {
    route_entities: u64,
    duplicate_routes: u64,
    ticks: u64,
    planned_items: u64,
    submissions: u64,
    completions: u64,
    requeues: u64,
    blocked: u64,
    stalled: u64,
    target_submissions: u64,
    target_completions: u64,
    target_cancellations: u64,
    target_stalled: u64,
    recovered_stalls: u64,
    target_recovered_stalls: u64,
    abandoned_stalls: u64,
    target_abandoned_stalls: u64,
    target_high_water: u64,
    admission_unknown_capability: u64,
    admission_capability_in_flight: u64,
    admission_capability_stalled: u64,
    admission_capability_blocked: u64,
    admission_duplicate_request: u64,
    admission_target_in_flight: u64,
    admission_target_capacity: u64,
    admission_global_target_capacity: u64,
    admission_domain_target_capacity: u64,
    admission_target_scope_byte_capacity: u64,
    admission_control_delivery_capacity: u64,
    admission_observation_delivery_capacity: u64,
    admission_sideband_not_allowed: u64,
    admission_invariant_violation: u64,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum EcsScheduleSet {
    Lifecycle,
    Plan,
    Domain,
}

/// Typed work intent emitted by the ECS scheduler.
///
/// The intent carries enough runtime attribution for a future bridge to select
/// an existing typed request port. It is not an OS request and does not contain
/// provider payload data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EcsWorkItem {
    pub(crate) capability: CapabilityId,
    pub(crate) provider: ProviderId,
    pub(crate) delivery: crate::config::DeliveryClass,
    pub(crate) domain: RuntimeDomain,
}

/// Bounded typed output of one ECS scheduling pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EcsWorkPlan {
    pub(crate) items: Vec<EcsWorkItem>,
    pub(crate) stalled: Vec<StalledSubject>,
}

/// Runtime-local plugin seam for domain systems.
///
/// A plugin may add ECS systems and schedule constraints, but it cannot bypass
/// the typed runtime bridge or acquire an OS handle. Domain plugins will be
/// added incrementally after their existing lane behavior has parity coverage.
trait RuntimeEcsPlugin {
    fn build(&self, app: &mut App);
}

struct CapabilityLifecyclePlugin;

impl RuntimeEcsPlugin for CapabilityLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                EcsScheduleSet::Lifecycle,
                EcsScheduleSet::Plan,
                EcsScheduleSet::Domain,
            )
                .chain(),
        );
        app.add_systems(
            Update,
            (
                scheduling_systems::mark_stalled_system,
                mark_stalled_target_jobs_system,
                scheduling_systems::abandon_stalled_system,
                abandon_stalled_target_jobs_system,
                scheduling_systems::mark_due_system,
            )
                .chain()
                .in_set(EcsScheduleSet::Lifecycle),
        );
        app.add_systems(
            Update,
            scheduling_systems::order_due_system.in_set(EcsScheduleSet::Plan),
        );
    }
}

fn install_runtime_plugins(app: &mut App) {
    let plugins: [&dyn RuntimeEcsPlugin; 2] =
        [&CapabilityLifecyclePlugin, &domain::DomainDiagnosticsPlugin];
    for plugin in plugins {
        plugin.build(app);
    }
}

/// Internal capability scheduler kernel bootstrapped by a headless Bevy App.
///
/// The type is visible as an opt-in runtime seam, while construction from
/// native provider routes stays crate-private. A future benchmark surface must
/// receive an explicitly approved route contract rather than leaking the
/// runtime's provider-registration internals.
pub(crate) struct RuntimeEcsScheduler {
    world: World,
    schedule: bevy_ecs::schedule::Schedule,
    entities: BTreeMap<CapabilityId, Entity>,
    target_jobs: TargetJobRegistry,
    delivery_reservations: BTreeMap<(CapabilityId, RequestId), DeliveryReservation>,
    budgets: RuntimeBudgets,
    retry_interval_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeliveryReservationState {
    Active,
    TerminalClaimed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeliveryReservation {
    class: DeliveryClass,
    state: DeliveryReservationState,
}

/// Shared scheduler plus its single monotonic time authority.
///
/// Keeping the clock attached to the handle prevents request ports, health
/// publication, and scheduled polling from accidentally using envelope wall
/// timestamps for ECS lifecycle transitions.
#[derive(Clone)]
pub(crate) struct RuntimeEcsSchedulerHandle {
    scheduler: Arc<Mutex<RuntimeEcsScheduler>>,
    monotonic_clock_ms: fn() -> u64,
}

impl RuntimeEcsSchedulerHandle {
    pub(crate) fn new(routes: &[CapabilityRoute], monotonic_clock_ms: fn() -> u64) -> Self {
        Self::with_budgets(routes, monotonic_clock_ms, RuntimeBudgets::DEFAULT)
    }

    pub(crate) fn with_budgets(
        routes: &[CapabilityRoute],
        monotonic_clock_ms: fn() -> u64,
        budgets: RuntimeBudgets,
    ) -> Self {
        let initial_now_ms = monotonic_clock_ms();
        let scheduler = if budgets == RuntimeBudgets::DEFAULT {
            RuntimeEcsScheduler::from_runtime_routes(routes, initial_now_ms)
        } else {
            RuntimeEcsScheduler::from_runtime_routes_with_budgets(routes, initial_now_ms, budgets)
        };
        Self {
            scheduler: Arc::new(Mutex::new(scheduler)),
            monotonic_clock_ms,
        }
    }

    pub(crate) fn lock(&self) -> LockResult<MutexGuard<'_, RuntimeEcsScheduler>> {
        self.scheduler.lock()
    }

    pub(crate) fn now_ms(&self) -> u64 {
        (self.monotonic_clock_ms)()
    }
}

impl RuntimeEcsScheduler {
    pub(crate) fn from_runtime_routes(routes: &[CapabilityRoute], monotonic_now_ms: u64) -> Self {
        Self::from_runtime_routes_with_budgets(routes, monotonic_now_ms, RuntimeBudgets::DEFAULT)
    }

    pub(crate) fn from_runtime_routes_with_budgets(
        routes: &[CapabilityRoute],
        monotonic_now_ms: u64,
        budgets: RuntimeBudgets,
    ) -> Self {
        let mut app = App::new();
        app.world_mut()
            .insert_resource(SchedulerClock { monotonic_now_ms });
        app.world_mut().insert_resource(DueWork::default());
        app.world_mut().insert_resource(StalledWork::default());
        app.world_mut().insert_resource(AbandonedWork::default());
        app.world_mut().insert_resource(StallDiagnostics::default());
        app.world_mut().insert_resource(EcsDiagnostics::default());
        app.world_mut().insert_resource(StallPolicy {
            lifetime_ms: budgets.max_stalled_lifetime_ms,
        });

        let mut entities = BTreeMap::new();
        let mut duplicate_routes = 0_u64;
        for route in routes {
            if entities.contains_key(&route.capability) {
                duplicate_routes = duplicate_routes.saturating_add(1);
                continue;
            }
            let entity = app
                .world_mut()
                .spawn((
                    CapabilityNode {
                        capability: route.capability.clone(),
                        provider: route.provider.clone(),
                        delivery: route.delivery,
                        domain: route.domain,
                        cadence_ms: route.cadence_ms,
                        sideband_policy: route.sideband_policy,
                    },
                    WorkState::Waiting,
                    DueAt(if route.cadence_ms.is_some() {
                        monotonic_now_ms
                    } else {
                        u64::MAX
                    }),
                ))
                .id();
            entities.insert(route.capability.clone(), entity);
        }
        {
            let mut diagnostics = app.world_mut().resource_mut::<EcsDiagnostics>();
            diagnostics.route_entities = entities.len() as u64;
            diagnostics.duplicate_routes = duplicate_routes;
        }

        install_runtime_plugins(&mut app);
        // `bevy_app::App` owns a runner that is intentionally not `Send`; the
        // runtime scheduler is shared by worker threads and must remain
        // `Send + Sync`. Keep the App as the composition host, then transfer
        // its configured world and Update schedule into the worker-safe kernel.
        let schedule = app
            .get_schedule_mut(Update)
            .map(std::mem::take)
            .unwrap_or_default();
        let world = std::mem::replace(app.world_mut(), World::new());
        let retry_interval_ms = DEFAULT_RETRY_INTERVAL_MS;
        Self {
            world,
            schedule,
            entities,
            target_jobs: TargetJobRegistry::default(),
            delivery_reservations: BTreeMap::new(),
            budgets,
            retry_interval_ms,
        }
        .install_retry_policy()
    }

    /// Mirror the retry backoff into the world so the ECS abandonment system
    /// requeues with the same cadence the lifecycle methods use.
    fn install_retry_policy(mut self) -> Self {
        let retry_interval_ms = self.retry_interval_ms;
        self.world_mut()
            .insert_resource(RetryIntervalMs(retry_interval_ms));
        self
    }

    /// Run one scheduling pass and preserve provider attribution for the
    /// typed runtime bridge.
    pub(crate) fn tick_plan(&mut self, monotonic_now_ms: u64) -> EcsWorkPlan {
        self.world_mut()
            .resource_mut::<SchedulerClock>()
            .monotonic_now_ms = monotonic_now_ms;
        self.world_mut().resource_mut::<DueWork>().items.clear();
        self.world_mut()
            .resource_mut::<StalledWork>()
            .subjects
            .clear();
        self.world_mut()
            .resource_mut::<AbandonedWork>()
            .subjects
            .clear();
        self.schedule.run(&mut self.world);
        self.retire_abandoned_work();
        let items = self.world().resource::<DueWork>().items.clone();
        let mut stalled = self.world().resource::<StalledWork>().subjects.clone();
        stalled.sort();
        stalled.dedup();
        {
            let mut history = self.world_mut().resource_mut::<StallDiagnostics>();
            for subject in &stalled {
                history.record(subject.clone());
            }
        }
        let mut diagnostics = self.world_mut().resource_mut::<EcsDiagnostics>();
        diagnostics.ticks = diagnostics.ticks.saturating_add(1);
        diagnostics.planned_items = diagnostics.planned_items.saturating_add(items.len() as u64);
        EcsWorkPlan { items, stalled }
    }

    /// Post-pass owned by the scheduler struct (the ECS systems cannot reach
    /// the registries): recycle delivery capacity and despawn abandoned
    /// target-job entities for every owner the systems retired this tick.
    fn retire_abandoned_work(&mut self) {
        let abandoned = {
            let mut resource = self.world_mut().resource_mut::<AbandonedWork>();
            std::mem::take(&mut resource.subjects)
        };
        for subject in abandoned {
            match subject {
                StalledSubject::Capability {
                    capability,
                    request_id,
                } => {
                    self.release_delivery(&capability, request_id);
                }
                StalledSubject::Target {
                    capability,
                    request_id,
                    ..
                } => {
                    self.remove_abandoned_target_job(&capability, request_id);
                    self.release_delivery(&capability, request_id);
                }
            }
        }
    }

    pub(crate) fn scheduling_snapshot(&self) -> RuntimeSchedulingSnapshot {
        let diagnostics = *self.world().resource::<EcsDiagnostics>();
        let (active_stalled_capabilities, active_stalled_targets) = self
            .world()
            .iter_entities()
            .filter(|entity| {
                entity
                    .get::<WorkState>()
                    .is_some_and(|state| matches!(state, WorkState::Stalled { .. }))
            })
            .fold((0_u64, 0_u64), |(capabilities, targets), entity| {
                if entity.get::<target_jobs::TargetJobNode>().is_some() {
                    (capabilities, targets.saturating_add(1))
                } else {
                    (capabilities.saturating_add(1), targets)
                }
            });
        let domain_diagnostics = *self.world().resource::<domain::DomainDiagnostics>();
        let domains = RuntimeDomain::ALL
            .into_iter()
            .map(|domain| DomainSchedulingSnapshot {
                domain: scheduling_domain(domain),
                planned_items: domain_diagnostics.planned_items(domain),
                active_targets: self.target_jobs.active_in_domain(domain) as u64,
                active_target_limit: self.budgets.active_target_limit_per_domain as u64,
            })
            .collect();
        let recent_stalls = self
            .world()
            .resource::<StallDiagnostics>()
            .recent
            .iter()
            .cloned()
            .map(|subject| match subject {
                StalledSubject::Capability {
                    capability,
                    request_id,
                } => SchedulingStall {
                    capability,
                    request_id,
                    scope: SchedulingScope::Capability,
                },
                StalledSubject::Target {
                    capability,
                    request_id,
                    scope,
                } => SchedulingStall {
                    capability,
                    request_id,
                    scope: SchedulingScope::Target(scope),
                },
            })
            .collect();
        RuntimeSchedulingSnapshot {
            route_count: diagnostics.route_entities,
            active_target_jobs: self.target_jobs.len() as u64,
            target_high_water: diagnostics.target_high_water,
            ticks: diagnostics.ticks,
            planned_items: diagnostics.planned_items,
            submissions: diagnostics.submissions,
            completions: diagnostics.completions,
            requeues: diagnostics.requeues,
            blocked: diagnostics.blocked,
            stalled: diagnostics.stalled,
            target_submissions: diagnostics.target_submissions,
            target_completions: diagnostics.target_completions,
            target_cancellations: diagnostics.target_cancellations,
            target_stalled: diagnostics.target_stalled,
            active_stalled_capabilities,
            active_stalled_targets,
            recovered_stalls: diagnostics.recovered_stalls,
            target_recovered_stalls: diagnostics.target_recovered_stalls,
            abandoned_stalls: diagnostics.abandoned_stalls,
            target_abandoned_stalls: diagnostics.target_abandoned_stalls,
            stale_terminal_publications: 0,
            worker_lane_exits: 0,
            provider_panics: 0,
            recent_provider_panics: Vec::new(),
            domains,
            budgets: SchedulingBudgetSnapshot {
                route_limit: self.budgets.route_limit as u64,
                active_target_limit: self.budgets.active_target_limit as u64,
                active_target_limit_per_capability: self.budgets.active_target_limit_per_capability
                    as u64,
                active_target_limit_per_domain: self.budgets.active_target_limit_per_domain as u64,
                active_target_scope_bytes: self.target_jobs.scope_bytes() as u64,
                target_scope_byte_limit: self.budgets.target_scope_byte_limit as u64,
                pending_deliveries: self.delivery_reservations.len() as u64,
                pending_delivery_limit: self.budgets.pending_delivery_limit as u64,
                pending_control_deliveries: self.pending_deliveries(DeliveryClass::Control) as u64,
                pending_observation_deliveries: self.pending_deliveries(DeliveryClass::Observation)
                    as u64,
                control_delivery_reserve: self.budgets.control_delivery_reserve as u64,
                max_stalled_lifetime_ms: self.budgets.max_stalled_lifetime_ms,
            },
            event_queues: Default::default(),
            admission: SchedulingAdmissionSnapshot {
                unknown_capability: diagnostics.admission_unknown_capability,
                capability_in_flight: diagnostics.admission_capability_in_flight,
                capability_stalled: diagnostics.admission_capability_stalled,
                capability_blocked: diagnostics.admission_capability_blocked,
                duplicate_request: diagnostics.admission_duplicate_request,
                target_in_flight: diagnostics.admission_target_in_flight,
                target_capacity: diagnostics.admission_target_capacity,
                global_target_capacity: diagnostics.admission_global_target_capacity,
                domain_target_capacity: diagnostics.admission_domain_target_capacity,
                target_scope_byte_capacity: diagnostics.admission_target_scope_byte_capacity,
                delivery_capacity: diagnostics
                    .admission_control_delivery_capacity
                    .saturating_add(diagnostics.admission_observation_delivery_capacity),
                control_delivery_capacity: diagnostics.admission_control_delivery_capacity,
                observation_delivery_capacity: diagnostics.admission_observation_delivery_capacity,
                sideband_not_allowed: diagnostics.admission_sideband_not_allowed,
                invariant_violation: diagnostics.admission_invariant_violation,
            },
            recent_stalls,
        }
    }

    fn world(&self) -> &World {
        &self.world
    }

    fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

const fn scheduling_domain(domain: RuntimeDomain) -> SchedulingDomain {
    match domain {
        RuntimeDomain::System => SchedulingDomain::System,
        RuntimeDomain::Process => SchedulingDomain::Process,
        RuntimeDomain::Storage => SchedulingDomain::Storage,
        RuntimeDomain::Service => SchedulingDomain::Service,
        RuntimeDomain::Environment => SchedulingDomain::Environment,
        RuntimeDomain::Integration => SchedulingDomain::Integration,
        RuntimeDomain::Sensor => SchedulingDomain::Sensor,
        RuntimeDomain::Power => SchedulingDomain::Power,
    }
}
