//! Bounded request-lane API and runtime composition.

mod construction;
mod facet_attach;
mod lanes;
mod port;

pub use construction::{ChannelRuntime, RuntimeBudgetField, RuntimeConstructionError};
pub use lanes::{Queued, RuntimeLanes};
