//! Portable safe-I/O provider implementations shared by native adapters.
//!
//! This crate is deliberately narrower than the provider SPI and the runtime:
//! it contains only concrete, bounded I/O implementations already required by
//! at least two operating-system adapters. It may own the generic lifecycle
//! boundary for a caller-supplied fixed-argument command, but never chooses a
//! platform tool, parses native output, or assigns capability/business meaning.
//! OS discovery, scheduling, product policy, and miscellaneous helpers do not
//! belong here.

#![forbid(unsafe_code)]

mod battery;
mod command;
mod directory_usage;
mod edid;

pub use battery::collect_battery_snapshot;
pub use command::{
    BoundedCommandError, BoundedCommandSpawner, BoundedOutput, MAX_CAPTURED_STREAM_BYTES,
    MAX_CAPTURED_TOTAL_BYTES, OwnedProcessTree, SpawnedCommand, run_with_spawner, run_with_timeout,
};
pub use directory_usage::DirectoryUsageScanner;
pub use edid::{EdidFacts, parse_edid};

#[cfg(test)]
#[path = "../tests/common/test_support.rs"]
pub(crate) mod test_support;
