//! Test-side runtime re-exports: one stable path for the registered test
//! modules to reach the production seam vocabulary and the test-only loop
//! wrappers.

pub(crate) use crate::runtime::seam::seam_support::run_event_loop;
pub(crate) use crate::runtime::seam::{
    EventReaction, TerminalEventSource, apply_terminal_event_with_plan,
};
