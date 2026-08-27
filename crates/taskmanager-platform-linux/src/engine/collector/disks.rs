//! Linux storage collection composed from responsibility-owned submodules.

mod domain;
mod identity;
mod inventory;
mod mounts;
mod provenance;
mod scalars;
mod sysfs;

pub(super) use domain::collect_storage_domain;
pub(super) use scalars::DiskScalarState;

#[cfg(test)]
#[path = "../../../tests/headless/engine/collector/disks.rs"]
mod tests;
