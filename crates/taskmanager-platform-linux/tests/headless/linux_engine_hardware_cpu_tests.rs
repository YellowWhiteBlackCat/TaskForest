//! Pure CPU sysfs parsing regressions (zero host dependencies).

use super::*;
use taskmanager_core::MAX_TRACKED_LOGICAL_CPUS;

#[test]
fn parse_cpulist_decodes_kernel_lists_and_skips_malformed_tokens() {
    assert_eq!(parse_cpulist("0-3,5,7-9"), vec![0, 1, 2, 3, 5, 7, 8, 9]);
    assert_eq!(parse_cpulist(" 2 "), vec![2]);
    assert_eq!(parse_cpulist("0-3 , 5"), vec![0, 1, 2, 3, 5]);
    assert!(parse_cpulist("").is_empty());
    // Inverted ranges and unparseable tokens stay skipped.
    assert!(parse_cpulist("3-1,notanumber").is_empty());
}

#[test]
fn parse_cpulist_truncates_runaway_ranges_at_the_tracked_cpu_ceiling() {
    // A malformed sysfs range near u32::MAX must not materialize billions of
    // ids: the output truncates at exactly MAX_TRACKED_LOGICAL_CPUS ids.
    let runaway = parse_cpulist("0-4294967295");
    assert_eq!(runaway.len(), MAX_TRACKED_LOGICAL_CPUS);
    assert_eq!(runaway.first(), Some(&0));
    assert_eq!(
        runaway.last(),
        Some(&((MAX_TRACKED_LOGICAL_CPUS - 1) as u32))
    );

    // Tokens after a capped range are ignored under the same total ceiling.
    let capped = parse_cpulist(&format!("0-{},9", u32::MAX));
    assert_eq!(capped.len(), MAX_TRACKED_LOGICAL_CPUS);

    // Singles alone obey the same total ceiling.
    let singles = (0..=(MAX_TRACKED_LOGICAL_CPUS as u32 + 64))
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(parse_cpulist(&singles).len(), MAX_TRACKED_LOGICAL_CPUS);
}
