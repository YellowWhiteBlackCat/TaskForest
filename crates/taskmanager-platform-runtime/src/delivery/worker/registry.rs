//! Restart gates and idle retirement for on-demand provider lanes.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::time::Duration;

use crossbeam_channel::{Receiver, select};
use taskmanager_platform_contract::CapabilityId;

use super::{WorkerRuntime, WorkerSpawnError};
use crate::channel::Queued;
use crate::delivery::publisher::RuntimeEventPublisher;

const LAZY_LANE_IDLE_TIMEOUT: Duration = Duration::from_secs(15);

/// Shared start gates attached to every present request port.
///
/// A lane is registered exactly once, but its worker can be started and
/// retired repeatedly. The request channel and provider closure therefore
/// remain canonical while the expensive OS thread and allocator arena exist
/// only while the capability is active.
pub(crate) struct LaneStartRegistry {
    slots: Mutex<HashMap<CapabilityId, Arc<LaneStartSlot>>>,
    worker: OnceLock<Weak<WorkerRuntime>>,
}

impl Default for LaneStartRegistry {
    fn default() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            worker: OnceLock::new(),
        }
    }
}

impl LaneStartRegistry {
    pub(crate) fn bind_worker(&self, worker: &Arc<WorkerRuntime>) {
        let _ = self.worker.set(Arc::downgrade(worker));
    }

    fn worker(&self, lane: &str) -> Result<Arc<WorkerRuntime>, WorkerSpawnError> {
        self.worker
            .get()
            .and_then(Weak::upgrade)
            .ok_or_else(|| WorkerSpawnError::OwnerGone {
                worker: lane.to_owned(),
            })
    }

    fn register<F>(
        &self,
        capability: CapabilityId,
        starter: F,
    ) -> Result<Arc<LaneStartSlot>, WorkerSpawnError>
    where
        F: Fn(Arc<LaneStartSlot>) -> Result<(), WorkerSpawnError> + Send + Sync + 'static,
    {
        let slot = Arc::new(LaneStartSlot {
            starter: Arc::new(starter),
            state: Mutex::new(LaneStartState::Dormant),
            changed: Condvar::new(),
        });
        let mut slots = self
            .slots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slots
            .insert(capability.clone(), Arc::clone(&slot))
            .is_some()
        {
            return Err(WorkerSpawnError::Registration {
                worker: capability.to_string(),
                message: "capability already has a registered lane".to_owned(),
            });
        }
        drop(slots);
        if matches!(lane_residency(&capability), LaneResidency::Resident) {
            slot.ensure_started()?;
        }
        Ok(slot)
    }

    /// Start the registered lane for the first request, or leave construction
    /// tests and partially assembled handles alone until registration finishes.
    pub(crate) fn ensure_started(&self, capability: &CapabilityId) -> Result<(), WorkerSpawnError> {
        let slot = self
            .slots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(capability)
            .cloned();
        match slot {
            Some(slot) => slot.ensure_started(),
            None => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LaneResidency {
    Resident,
    OnDemand,
}

fn lane_residency(capability: &CapabilityId) -> LaneResidency {
    match capability.as_str() {
        // These are the only lanes needed for the always-visible dashboard
        // refresh path. They stay alive after their first request so the
        // normal cadence never pays repeated thread-start latency.
        "telemetry.host" | "telemetry.cpu" | "telemetry.memory" | "telemetry.storage"
        | "telemetry.network" | "telemetry.gpu" | "process.list" => LaneResidency::Resident,
        _ => LaneResidency::OnDemand,
    }
}

type LaneStarter =
    dyn Fn(Arc<LaneStartSlot>) -> Result<(), WorkerSpawnError> + Send + Sync + 'static;

pub(crate) struct LaneStartSlot {
    starter: Arc<LaneStarter>,
    state: Mutex<LaneStartState>,
    changed: Condvar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LaneStartState {
    Dormant,
    Starting,
    Running,
}

impl LaneStartSlot {
    fn ensure_started(self: &Arc<Self>) -> Result<(), WorkerSpawnError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            match *state {
                LaneStartState::Running => return Ok(()),
                LaneStartState::Starting => {
                    state = self
                        .changed
                        .wait(state)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
                LaneStartState::Dormant => {
                    *state = LaneStartState::Starting;
                    break;
                }
            }
        }
        drop(state);

        let result = (self.starter)(Arc::clone(self));
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A very short-lived worker may already have retired while the
        // starter was returning. Preserve Dormant in that case so a later
        // request can start it again instead of falsely reporting Running.
        if *state == LaneStartState::Starting {
            *state = if result.is_ok() {
                LaneStartState::Running
            } else {
                LaneStartState::Dormant
            };
        }
        self.changed.notify_all();
        result
    }

    fn mark_stopped(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(*state, LaneStartState::Starting | LaneStartState::Running) {
            *state = LaneStartState::Dormant;
        }
        self.changed.notify_all();
    }
}

pub(crate) fn recv_or_shutdown_with_idle<R>(
    receiver: &Receiver<Queued<R>>,
    shutdown: &Receiver<()>,
    idle_timeout: Option<Duration>,
) -> Option<Queued<R>> {
    let Some(idle_timeout) = idle_timeout else {
        return super::recv_or_shutdown(receiver, shutdown);
    };
    if super::shutdown_requested(shutdown) {
        return None;
    }
    let idle = crossbeam_channel::after(idle_timeout);
    select! {
        recv(shutdown) -> _ => None,
        recv(receiver) -> queued => queued.ok(),
        recv(idle) -> _ => None,
    }
}

/// Start one provider lane immediately for test owners, or register a
/// restartable on-demand worker for a production runtime.
pub(crate) fn spawn_or_register_lane<R, F, Run>(
    workers: &WorkerRuntime,
    capability: Option<CapabilityId>,
    receiver: Receiver<Queued<R>>,
    publisher: Arc<RuntimeEventPublisher>,
    execute: F,
    run: Run,
) -> Result<(), WorkerSpawnError>
where
    R: Send + 'static,
    F: Send + 'static,
    Run: Fn(
            Receiver<Queued<R>>,
            Arc<Mutex<F>>,
            Arc<RuntimeEventPublisher>,
            Receiver<()>,
            Option<Duration>,
        ) + Send
        + Sync
        + 'static,
{
    let lane = capability
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| super::worker_name::<R>());
    let idle_timeout = capability.as_ref().and_then(|capability| {
        matches!(lane_residency(capability), LaneResidency::OnDemand)
            .then_some(LAZY_LANE_IDLE_TIMEOUT)
    });
    let receiver_for_start = receiver.clone();
    let execute = Arc::new(Mutex::new(execute));
    let run = Arc::new(run);
    if let (Some(starters), Some(capability)) = (workers.lane_starters(), capability) {
        let starter_ref = Arc::downgrade(&starters);
        let lane_for_start = lane.clone();
        let run_for_start = Arc::clone(&run);
        let execute_for_start = Arc::clone(&execute);
        let publisher_for_start = Arc::clone(&publisher);
        starters.register(capability, move |slot| {
            let starters = starter_ref
                .upgrade()
                .ok_or_else(|| WorkerSpawnError::OwnerGone {
                    worker: lane_for_start.clone(),
                })?;
            let worker = starters.worker(&lane_for_start)?;
            let receiver = receiver_for_start.clone();
            let execute = Arc::clone(&execute_for_start);
            let publisher = Arc::clone(&publisher_for_start);
            let run = Arc::clone(&run_for_start);
            worker.spawn(lane_for_start.clone(), move |shutdown| {
                run(receiver, execute, publisher, shutdown, idle_timeout);
                slot.mark_stopped();
            })
        })?;
        Ok(())
    } else {
        workers.spawn(lane, move |shutdown| {
            run(receiver_for_start, execute, publisher, shutdown, None);
        })
    }
}
