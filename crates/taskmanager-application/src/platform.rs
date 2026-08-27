//! Composable platform capability facets.
//!
//! `PlatformHandle` is assembled from independent request ports. A native
//! platform, sandbox, or remote runtime therefore implements only the
//! capabilities it actually has.

mod client;
pub use client::{
    AutomaticSchedule, AutomaticScheduleProfile, PlatformClient, automatic_cadence_ms,
    automatic_schedules,
};
mod event_batch;
pub use event_batch::*;
mod facets;
pub use facets::*;
mod handle;
pub use handle::*;
mod process_insights_projection;
pub use process_insights_projection::*;
mod smart_projection;
pub use smart_projection::*;
mod startup_evidence_projection;
pub use startup_evidence_projection::*;
mod startup_timeline_projection;
pub use startup_timeline_projection::*;
mod system_telemetry_projection;
pub use system_telemetry_projection::*;
