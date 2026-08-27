//! source-inspection: static-policy
//!
//! Network hot-path allocation gate (design-debt #2 closure).
//!
//! The 1s network tick rebuilds the sysfs inventory, counter state, and the
//! wire `NetworkMetrics` rows. This gate pins the **true allocation points**
//! (`to_string` / `to_owned` / `format!` — `Arc::clone` is a refcount bump, not
//! an allocation) on the per-tick path so a regression is caught at review
//! time, exactly like the other source-level contract gates in this suite.
//!
//! After the #2b closure the per-interface wire build allocates **zero**
//! strings: the identity is precomputed once per inventory
//! (`SysfsInterface::stable_id`), the wire fields are `Arc<str>` moved out of
//! the inventory, and addresses/wireless/iw observations are consumed by move
//! instead of cloned. The remaining allocations below are: two lifecycle-event
//! conversions (by_device re-key, reset_absent), one discovered-devices
//! conversion per tick, the address-formatter rows in the sysfs parser, and
//! one tracker initialization. Any growth must come with a justification.

use std::fs;
use std::path::PathBuf;

fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn allocation_points(path: &str) -> usize {
    let source = fs::read_to_string(repository().join(path))
        .unwrap_or_else(|error| panic!("{path}: {error}"));
    let production = source
        .split("#[cfg(test)]")
        .next()
        .expect("production source");
    production
        .lines()
        .filter(|line| {
            line.contains(".to_string()")
                || line.contains(".to_owned()")
                || line.contains("format!(")
        })
        .count()
}

#[test]
fn network_tick_wire_build_stays_allocation_light() {
    let base = "crates/taskmanager-platform-linux/src/engine/collector/network";
    // The per-tick assemble path must not grow allocations: after #2b the wire
    // rows are built from Arc<str> moves only.
    assert!(
        allocation_points(&format!("{base}.rs")) <= 3,
        "network.rs grew hot-path allocations — Arc<str> wire moves are the contract"
    );
    // Counter tracking allocates once per tracker initialization only.
    assert!(
        allocation_points(&format!("{base}/counters.rs")) <= 1,
        "counters.rs grew hot-path allocations — stable_id is precomputed once per inventory"
    );
    // The sysfs parser keeps its parse-time conversions (address formatting,
    // driver vendor join, ssid extraction); these are source-side necessities.
    assert!(
        allocation_points(&format!("{base}/sources.rs")) <= 6,
        "sources.rs grew parse-time allocations beyond the documented six"
    );
}
