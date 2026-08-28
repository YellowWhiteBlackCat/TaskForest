//! ECS scheduling systems for due work, stalls, and abandonment.

use super::*;
use crate::config::DeliveryClass;
use bevy_ecs::prelude::{Query, Res, ResMut};

pub(super) fn mark_stalled_system(
    clock: Res<SchedulerClock>,
    stall_policy: Res<StallPolicy>,
    mut stalled_work: ResMut<StalledWork>,
    mut diagnostics: ResMut<EcsDiagnostics>,
    mut capabilities: Query<(&CapabilityNode, &mut WorkState)>,
) {
    for (node, mut state) in &mut capabilities {
        if let Some(request_id) =
            state.expire_lease(clock.monotonic_now_ms, stall_policy.lifetime_ms)
        {
            stalled_work.subjects.push(StalledSubject::Capability {
                capability: node.capability.clone(),
                request_id,
            });
            diagnostics.stalled = diagnostics.stalled.saturating_add(1);
        }
    }
}

pub(super) fn mark_due_system(
    clock: Res<SchedulerClock>,
    mut due_work: ResMut<DueWork>,
    mut capabilities: Query<(&CapabilityNode, &DueAt, &mut WorkState)>,
) {
    for (node, due_at, mut state) in &mut capabilities {
        if *state == WorkState::Waiting && due_at.0 <= clock.monotonic_now_ms {
            *state = WorkState::Ready;
            due_work.items.push(EcsWorkItem {
                capability: node.capability.clone(),
                provider: node.provider.clone(),
                delivery: node.delivery,
                domain: node.domain,
            });
        }
    }
}

pub(super) fn order_due_system(mut due_work: ResMut<DueWork>) {
    due_work.items.sort_by(|left, right| {
        delivery_rank(left.delivery)
            .cmp(&delivery_rank(right.delivery))
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.capability.cmp(&right.capability))
    });
    due_work
        .items
        .dedup_by(|left, right| left.capability == right.capability);
}

/// Retire capability-level stalled owners whose abandonment deadline passed.
/// The route requeues with the ordinary retry backoff; delivery capacity is
/// recycled by `tick_plan`'s post-pass over [`AbandonedWork`]. A very late
/// completion of the retired request is then a tolerated stale publication.
pub(super) fn abandon_stalled_system(
    clock: Res<SchedulerClock>,
    retry: Res<RetryIntervalMs>,
    mut abandoned: ResMut<AbandonedWork>,
    mut diagnostics: ResMut<EcsDiagnostics>,
    mut capabilities: Query<(&CapabilityNode, &mut WorkState, &mut DueAt)>,
) {
    for (node, mut state, mut due_at) in &mut capabilities {
        let WorkState::Stalled {
            request_id,
            abandon_at_ms,
        } = *state
        else {
            continue;
        };
        if abandon_at_ms > clock.monotonic_now_ms {
            continue;
        }
        *state = WorkState::Waiting;
        due_at.0 = clock
            .monotonic_now_ms
            .saturating_add(retry.0)
            .max(abandon_at_ms);
        abandoned.subjects.push(StalledSubject::Capability {
            capability: node.capability.clone(),
            request_id,
        });
        diagnostics.abandoned_stalls = diagnostics.abandoned_stalls.saturating_add(1);
    }
}

const fn delivery_rank(delivery: DeliveryClass) -> u8 {
    match delivery {
        DeliveryClass::Control => 0,
        DeliveryClass::Observation => 1,
    }
}
