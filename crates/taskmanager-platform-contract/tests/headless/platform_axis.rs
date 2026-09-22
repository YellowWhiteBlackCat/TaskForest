//! Platform axis identity tests: totality, uniqueness and stable machine ids.
//!
//! `PlatformAxis::id` matches every variant without a wildcard, so a new
//! platform role fails to compile until every report site names it. These
//! tests pin the runtime half: `ALL` carries each variant exactly once and the
//! machine ids round-trip, so a manifest or ledger cannot silently address a
//! platform that does not exist.

use super::*;

/// `ALL` is total and duplicate-free: non-empty, every id unique and non-empty,
/// and the pinned count makes adding or dropping a platform role a conscious
/// contract change.
#[test]
fn all_platform_axes_are_total_and_unique() {
    assert_eq!(PlatformAxis::ALL.len(), 3);
    let mut ids: Vec<_> = PlatformAxis::ALL.iter().map(|axis| axis.id()).collect();
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count, "platform axis ids must be unique");
    assert!(
        PlatformAxis::ALL.iter().all(|axis| !axis.id().is_empty()),
        "every platform axis names a stable machine id"
    );
}

/// The machine ids are the contract: renaming one is a hard cutover of every
/// manifest, ledger and report that references it.
#[test]
fn platform_axis_ids_are_stable() {
    assert_eq!(PlatformAxis::Linux.id(), "linux");
    assert_eq!(PlatformAxis::Windows.id(), "windows");
    assert_eq!(PlatformAxis::Macos.id(), "macos");
}

/// Every variant is registered in `ALL` exactly once and resolves from its own
/// id; unknown ids never resolve to a fabricated platform.
#[test]
fn platform_axis_covers_every_variant_exactly_once() {
    for axis in PlatformAxis::ALL {
        assert_eq!(
            PlatformAxis::ALL
                .iter()
                .filter(|candidate| **candidate == axis)
                .count(),
            1,
            "platform axis {} must appear exactly once in ALL",
            axis.id()
        );
        assert_eq!(PlatformAxis::from_id(axis.id()), Some(axis));
    }
    assert_eq!(PlatformAxis::from_id("bsd"), None);
    assert_eq!(PlatformAxis::from_id(""), None);
}

/// Display uses the stable machine id, so report text and manifest ids cannot
/// drift into two spellings.
#[test]
fn display_uses_the_stable_machine_id() {
    for axis in PlatformAxis::ALL {
        assert_eq!(axis.to_string(), axis.id());
    }
}
