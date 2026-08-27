//! Host-neutral conformance scenarios shared by native platform adapters.
//!
//! Each OS adapter owns its fixtures and composition; this crate owns the
//! assertions they share: capability-surface honesty, typed-failure
//! attribution, process-row invariants, and the live-runtime drain contract.
//! It deliberately contains no OS I/O, no `cfg(target_os)` branches, and no
//! provider implementation, so the same scenario runs on Linux, Windows, and
//! macOS real-machine gates.

#![forbid(unsafe_code)]

mod capability;
mod identity;
mod process;
mod smoke;
mod source;

pub use capability::assert_fresh_surface_descriptors;
pub use identity::assert_identity_change_is_side_effect_free;
pub use process::assert_process_rows_consistent;
pub use smoke::{
    LiveDrain, assert_live_smoke_ok, batch_event_count, collect_process_rows, drain_until,
    drain_until_process_rows,
};
pub use source::assert_device_discovery_consistent;
