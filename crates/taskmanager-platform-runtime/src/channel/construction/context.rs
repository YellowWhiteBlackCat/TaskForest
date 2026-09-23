//! Shared lane-construction context for the channel runtime facet domains.
//!
//! `LaneContext` bundles the two bounded-queue capacities plus the scheduler
//! and lane-start registry every typed request port clones, so each facet
//! domain helper takes one reference instead of four arguments.

use std::sync::Arc;

use crate::delivery::LaneStartRegistry;
use crate::ecs::RuntimeEcsSchedulerHandle;

/// Shared inputs for opening one facet domain's bounded request lanes: the two
/// queue capacities plus the scheduler and lane-start registry every port
/// clones. Bundling them keeps each domain helper's signature small.
pub(super) struct LaneContext {
    pub(super) observation_capacity: usize,
    pub(super) control_capacity: usize,
    pub(super) ecs_scheduler: RuntimeEcsSchedulerHandle,
    pub(super) lane_starters: Arc<LaneStartRegistry>,
}
