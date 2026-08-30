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
    assert_eq!(
        units::bytes_to_gib(TWO_POW_53),
        (TWO_POW_53 / GIB_IN_BYTES) as f64
    );
    assert_eq!(
        units::bytes_to_gib(TWO_POW_53 + 1024) - units::bytes_to_gib(TWO_POW_53),
        1024.0 / GIB_IN_BYTES as f64
    );
}

#[test]
fn extreme_byte_counts_never_overflow_or_grow_non_finite() {
    assert_eq!(units::bytes_to_gib(u64::MAX), 17179869184.0);
    assert!(units::bytes_to_gib(u64::MAX).is_finite());
    assert!(units::bytes_to_mib(u64::MAX).is_finite());
}

#[test]
fn mission_center_units_match_default_memory_drive_and_network_choices() {
    let units = UnitPreferences::default();
    // Memory: legacy Mission Center ladder this wave (follow-up replaces
    // the out-of-file call sites together with their pinned tests).
    assert_eq!(
        units.format_quantity(16 * GIB_IN_BYTES, QuantityFamily::Memory, false),
        "16.0 GiB"
    );
    // Drive/Network: the neutral core ladder (TUI/Iced parity).
    assert_eq!(
        units.format_quantity(2_000_000_000, QuantityFamily::Drive, true),
        "1.9 GiB/s"
    );
    assert_eq!(
        units.format_quantity(1_000_000, QuantityFamily::Network, true),
        "8.0 Mb/s"
    );
}

#[test]
fn mission_center_units_switch_bytes_bits_and_base_without_changing_source_value() {
    let units = UnitPreferences {
        memory_use_bytes: false,
        memory_use_base2: false,
        drive_use_bytes: true,
        drive_use_base2: false,
        network_use_bytes: true,
        network_use_base2: true,
    };
    assert_eq!(
        units.format_quantity(1_000_000, QuantityFamily::Memory, false),
        "8.0 Mb"
    );
    assert_eq!(
        units.format_quantity(1_000_000, QuantityFamily::Drive, true),
        "1.0 MB/s"
    );
    assert_eq!(
        crate::gpui_app::formatting::format_network_graph_megabytes(units, 1.0),
        "976.6 KiB/s"
    );
}

#[test]
fn unit_formatter_fails_closed_for_non_finite_graph_samples() {
    assert_eq!(
        crate::gpui_app::formatting::format_network_graph_megabytes(
            UnitPreferences::default(),
            f32::NAN
        ),
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
    let mut units = UnitPreferences::default();
    for (use_bytes, use_base2) in [(true, true), (true, false), (false, true), (false, false)] {
        units.drive_use_bytes = use_bytes;
        units.drive_use_base2 = use_base2;
        units.network_use_bytes = use_bytes;
        units.network_use_base2 = use_base2;
        let prefs = units;
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
                for kind in [QuantityFamily::Drive, QuantityFamily::Network] {
                    let family = kind;
                    assert_eq!(
                        units.format_quantity(value, kind, per_second),
                        taskmanager_core::core::units::format_quantity(
                            value, family, per_second, &prefs
                        ),
                        "{kind:?} {value} B ({use_bytes}, {use_base2}, {per_second})"
                    );
                }
            }
            assert_eq!(
                units.format_quantity_pair(value, value * 2, QuantityFamily::Network, true),
                taskmanager_core::core::units::format_quantity_pair(
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
            crate::gpui_app::formatting::format_network_graph_megabytes(units, 1.0),
            taskmanager_core::core::units::format_quantity_f64(
                1_000_000.0,
                QuantityFamily::Network,
                true,
                &prefs
            )
        );
    }
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
