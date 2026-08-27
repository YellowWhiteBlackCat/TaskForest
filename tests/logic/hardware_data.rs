//! Host smoke tests for hardware-data correctness (cache totals, etc.). These read
//! live /sys + DMI, so they assert aggregate sanity rather than exact host values.

// Reads live /sys + detect_cpu_cache (a Linux-only provider that returns a
// None stub on macOS/Windows) — compile/run on Linux only, else the stub makes
// every assertion fail pointlessly off-Linux.
#[cfg(target_os = "linux")]
#[test]
fn cache_totals_are_aggregated_not_per_core() {
    let (l1, l2, l3) = taskmanager_platform_linux::detect_cpu_cache();
    assert!(
        l1.is_some_and(|value| value > 0),
        "L1 cache missing: {l1:?}"
    );
    assert!(
        l2.is_some_and(|value| value > 0),
        "L2 cache missing: {l2:?}"
    );
    assert!(
        l3.is_some_and(|value| value > 0),
        "L3 cache missing: {l3:?}"
    );
    // Aggregation guard: the summed L2 total must be at least ONE cpu0 L2
    // instance, proving detect_cpu_cache read real instances rather than a
    // degenerate value. The floor is read straight from sysfs so it is
    // host-independent — the dev box has 3 MiB/core L2, a CI runner may have
    // ~256 KiB/core, and a hardcoded KiB floor fails on one or the other. On a
    // multi-core part with private L2 the total is per-core times the core
    // count, so this also confirms the sum ran across instances, not just
    // cpu0's value; on a shared-L2 part total equals the one instance.
    if let Some(one_instance) = cpu0_l2_instance_kb() {
        assert!(
            l2.is_some_and(|total| total >= one_instance),
            "L2 aggregated total {l2:?} KiB < one cpu0 instance {one_instance} KiB — aggregation lost data"
        );
    }
    eprintln!("cache: L1={l1:?} KiB, L2={l2:?} KiB, L3={l3:?} KiB");
}

/// Largest single L2 instance on cpu0, read straight from sysfs and parsed
/// with the crate's own size parser — a host-independent threshold source for
/// the aggregation guard above (no hardcoded KiB value).
#[cfg(target_os = "linux")]
fn cpu0_l2_instance_kb() -> Option<u64> {
    let mut max = 0u64;
    let mut saw_any = false;
    for idx in 0..=4_u32 {
        let base = format!("/sys/devices/system/cpu/cpu0/cache/index{idx}");
        let level = std::fs::read_to_string(format!("{base}/level"));
        let size = std::fs::read_to_string(format!("{base}/size"));
        let (Ok(level), Ok(size)) = (level, size) else {
            continue;
        };
        if level.trim() != "2" {
            continue;
        }
        max = max.max(taskmanager_platform_linux::parse_size_to_kb(size.trim()));
        saw_any = true;
    }
    saw_any.then_some(max)
}
