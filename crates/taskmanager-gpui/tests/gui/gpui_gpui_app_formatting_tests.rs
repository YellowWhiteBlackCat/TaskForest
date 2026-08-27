use super::*;

const TWO_POW_53: u64 = 1 << 53;
const GIB_IN_BYTES: u64 = 1024 * 1024 * 1024;

#[test]
fn gib_conversion_keeps_integer_precision_past_2_pow_53() {
    // A bare cast rounds odd byte counts down at 2^53 (ULP = 2 there), so
    // 2^53 + 1 collapses onto 2^53 — the precision loss this module exists
    // to confine.
    assert_eq!((TWO_POW_53 + 1) as f64, TWO_POW_53 as f64);
    // The split conversion keeps the whole-GiB part exact, and sub-GiB
    // remainders survive when the fraction is representable.
    assert_eq!(bytes_to_gib(TWO_POW_53), (TWO_POW_53 / GIB_IN_BYTES) as f64);
    assert_eq!(
        bytes_to_gib(TWO_POW_53 + 1024) - bytes_to_gib(TWO_POW_53),
        1024.0 / GIB
    );
}

#[test]
fn extreme_byte_counts_never_overflow_or_grow_non_finite() {
    assert_eq!(bytes_to_gib(u64::MAX), 17179869184.0);
    assert!(bytes_to_gib(u64::MAX).is_finite());
    assert!(bytes_to_mib(u64::MAX).is_finite());
}

#[test]
fn human_string_tiers_follow_binary_boundaries() {
    assert_eq!(bytes_to_human(0), "0 B");
    assert_eq!(bytes_to_human(1023), "1023 B");
    assert_eq!(bytes_to_human(1024), "1.0 KiB");
    assert_eq!(bytes_to_human(1536), "1.5 KiB");
    assert_eq!(bytes_to_human(1024 * 1024), "1.0 MiB");
    assert_eq!(bytes_to_human(512 * 1024 * 1024), "512.0 MiB");
    assert_eq!(bytes_to_human(1024 * 1024 * 1024), "1.0 GiB");
    assert_eq!(bytes_to_human(1536 * 1024 * 1024), "1.5 GiB");
}

#[test]
fn gib_mib_readout_preserves_legacy_two_tier_behavior() {
    assert_eq!(format_gib_mib(0), "0.0 MiB");
    assert_eq!(format_gib_mib(500 * 1024 * 1024), "500.0 MiB");
    assert_eq!(format_gib_mib(1024 * 1024 * 1024), "1.0 GiB");
    assert_eq!(format_gib_mib(3 * 1024 * 1024 * 1024), "3.0 GiB");
}

#[test]
fn whole_unit_and_pair_readouts() {
    assert_eq!(
        format_gib_whole(3 * GIB_IN_BYTES + 400 * 1024 * 1024),
        "3 GiB"
    );
    assert_eq!(format_mib_whole(300 * 1024 * 1024), "300 MiB");
    assert_eq!(format_mib_2(1536 * 1024), "1.50 MiB");
    assert_eq!(format_mb_decimal(1_500_000), "1.5 MB");
    assert_eq!(format_decimal_memory(500_000_000), "500 MB");
    assert_eq!(format_decimal_memory(2_000_000_000), "2.0 GB");
    assert_eq!(
        format_gib_pair(2 * GIB_IN_BYTES, 8 * GIB_IN_BYTES),
        "2.0 / 8.0 GiB"
    );
}

#[test]
fn percent_and_rate_readouts() {
    assert_eq!(bytes_percent(256 * 1024 * 1024, 1024 * 1024 * 1024), 25.0);
    assert_eq!(format_bytes_rate(0), "0 KB/s");
    assert_eq!(format_bytes_rate(1_500_000), "1.5 MB/s");
    assert_eq!(format_bit_rate(125_000), "1.0 Mbps");
    assert_eq!(format_bit_rate(100), "800 bps");
    assert_eq!(format_gigabytes_per_sec(2_000_000_000), "2.0 GB/s");
}

#[test]
fn mission_center_units_match_default_memory_drive_and_network_choices() {
    let units = DisplayUnits::default();
    // Memory: legacy Mission Center ladder this wave (follow-up replaces
    // the out-of-file call sites together with their pinned tests).
    assert_eq!(
        units.format(16 * GIB_IN_BYTES, UnitKind::Memory, false),
        "16.0 GiB"
    );
    // Drive/Network: the neutral core ladder (TUI/Iced parity).
    assert_eq!(
        units.format(2_000_000_000, UnitKind::Drive, true),
        "1.9 GiB/s"
    );
    assert_eq!(units.format(1_000_000, UnitKind::Network, true), "8.0 Mb/s");
}

#[test]
fn mission_center_units_switch_bytes_bits_and_base_without_changing_source_value() {
    let units = DisplayUnits {
        memory_use_bytes: false,
        memory_use_base2: false,
        drive_use_bytes: true,
        drive_use_base2: false,
        network_use_bytes: true,
        network_use_base2: true,
    };
    assert_eq!(units.format(1_000_000, UnitKind::Memory, false), "8.00 Mb");
    assert_eq!(units.format(1_000_000, UnitKind::Drive, true), "1.0 MB/s");
    assert_eq!(units.format_network_graph_megabytes(1.0), "976.6 KiB/s");
}

#[test]
fn unit_formatter_fails_closed_for_non_finite_graph_samples() {
    assert_eq!(
        DisplayUnits::default().format_network_graph_megabytes(f32::NAN),
        "—"
    );
}

/// Parity with the neutral core single source: for a fixed input ×
/// preference matrix, the Drive and Network families must be byte-identical
/// to calling `taskmanager-core` directly. This is the acceptance test
/// that the same data + same preference renders the same string in every
/// frontend.
#[test]
fn drive_and_network_families_are_byte_identical_to_the_core_single_source() {
    let mut units = DisplayUnits::default();
    for (use_bytes, use_base2) in [(true, true), (true, false), (false, true), (false, false)] {
        units.drive_use_bytes = use_bytes;
        units.drive_use_base2 = use_base2;
        units.network_use_bytes = use_bytes;
        units.network_use_base2 = use_base2;
        let prefs = units.preferences();
        for value in [
            0,
            100,
            512,
            1536,
            125_000,
            1_500_000,
            2_000_000_000,
            16 * GIB_IN_BYTES,
        ] {
            for per_second in [false, true] {
                for kind in [UnitKind::Drive, UnitKind::Network] {
                    let family = DisplayUnits::family(kind);
                    assert_eq!(
                        units.format(value, kind, per_second),
                        crate::core::units::format_quantity(value, family, per_second, &prefs),
                        "{kind:?} {value} B ({use_bytes}, {use_base2}, {per_second})"
                    );
                }
            }
            assert_eq!(
                units.format_pair(value, value * 2, UnitKind::Network, true),
                crate::core::units::format_quantity_pair(
                    value,
                    value * 2,
                    QuantityFamily::Network,
                    true,
                    &prefs
                )
            );
        }
        // The megabyte-valued network graph-sample entry projects through
        // the same core ladder.
        assert_eq!(
            units.format_network_graph_megabytes(1.0),
            crate::core::units::format_quantity_f64(
                1_000_000.0,
                QuantityFamily::Network,
                true,
                &prefs
            )
        );
    }
}

/// The Memory family is the documented divergence: it keeps the Mission
/// Center adaptive ladder until the follow-up wave replaces its call sites
/// (`perf_views.rs:optional_memory`, `perf_views/memory_details.rs`) and
/// their pinned expectations together.
#[test]
fn memory_family_keeps_the_legacy_ladder_pending_call_site_replacement() {
    let units = DisplayUnits::default();
    assert_eq!(units.format(0, UnitKind::Memory, false), "0 KiB");
    assert_eq!(
        units.format_pair(0, GIB_IN_BYTES, UnitKind::Memory, false),
        "0 KiB / 1.00 GiB"
    );
    assert_eq!(
        units.format(16 * GIB_IN_BYTES, UnitKind::Memory, false),
        "16.0 GiB"
    );
}

#[test]
fn gpu_identity_text_uses_the_product_for_both_page_and_sidebar_consumers() {
    let mut resolved = GpuMetrics::new("", "Intel Xe Graphics");
    resolved.marketing_name = Some("Arc B390".into());
    resolved.driver = Some("xe".into());
    assert_eq!(
        gpu_identity_text(&resolved, 0),
        ("Arc B390".into(), "Intel Xe Graphics".into())
    );

    let mut generic = GpuMetrics::new("", "Intel Xe Graphics");
    generic.driver = Some("xe".into());
    assert_eq!(
        gpu_identity_text(&generic, 0),
        ("Intel Xe Graphics".into(), String::new()),
        "the kernel driver must not become a page subtitle"
    );
}
