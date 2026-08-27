//! Deterministic, headless lifecycle replay for ECS runtime verification.
//!
//! Replay steps carry only the same typed scheduler inputs that the live
//! runtime already observes. They never contain OS handles, provider payloads,
//! application revisions, or frontend state, so a replay cannot become a
//! second fact authority.

use taskmanager_application::{CapabilityId, RequestId};

use crate::health::CapabilityHealth;

use super::{EcsWorkPlan, RuntimeEcsScheduler};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ReplayStep {
    Tick {
        now_ms: u64,
    },
    Submitted {
        capability: CapabilityId,
        request_id: RequestId,
        submitted_at_ms: u64,
    },
    Health {
        capability: CapabilityId,
        request_id: RequestId,
        health: CapabilityHealth,
        observed_at_ms: u64,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ReplayReceipt {
    pub(super) plans: Vec<EcsWorkPlan>,
    pub(super) accepted_submissions: u64,
    pub(super) accepted_health: u64,
}

pub(super) fn run(
    scheduler: &mut RuntimeEcsScheduler,
    steps: impl IntoIterator<Item = ReplayStep>,
) -> ReplayReceipt {
    let mut receipt = ReplayReceipt::default();
    for step in steps {
        match step {
            ReplayStep::Tick { now_ms } => receipt.plans.push(scheduler.tick_plan(now_ms)),
            ReplayStep::Submitted {
                capability,
                request_id,
                submitted_at_ms,
            } => {
                receipt.accepted_submissions += u64::from(scheduler.reserve_submission(
                    &capability,
                    request_id,
                    submitted_at_ms,
                ));
            }
            ReplayStep::Health {
                capability,
                request_id,
                health,
                observed_at_ms,
            } => {
                receipt.accepted_health += u64::from(
                    scheduler
                        .record_health(&capability, request_id, health, observed_at_ms)
                        .is_accepted(),
                );
            }
        }
    }
    receipt
}
