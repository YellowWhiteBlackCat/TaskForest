use super::*;

const KIB: u64 = 1024;
const MIB: u64 = KIB * 1024;
const GIB: u64 = MIB * 1024;
const TIB: u64 = GIB * 1024;

/// The full bytes/bits × base-2/base-10 matrix on representative byte
/// counts, including the tier boundaries — the string contract every
/// frontend's parity test repeats verbatim.
#[test]
fn quantity_matrix_matches_the_shared_ladder_spec() {
    // Base-2 bytes: the historical `presentation::bytes` ladder.
    assert_eq!(format_quantity_with(0, true, true, false), "0 B");
    assert_eq!(format_quantity_with(512, true, true, false), "512 B");
    assert_eq!(format_quantity_with(1023, true, true, false), "1023 B");
    assert_eq!(format_quantity_with(KIB, true, true, false), "1.0 KiB");
    assert_eq!(format_quantity_with(1536, true, true, false), "1.5 KiB");
    assert_eq!(format_quantity_with(MIB, true, true, false), "1.0 MiB");
    assert_eq!(
        format_quantity_with(512 * MIB, true, true, false),
        "512.0 MiB"
    );
    assert_eq!(format_quantity_with(GIB, true, true, false), "1.0 GiB");
    assert_eq!(
        format_quantity_with(16 * GIB, true, true, false),
        "16.0 GiB"
    );
    // Base-2 bits: the 8× value with lowercase suffixes.
    assert_eq!(format_quantity_with(1536, false, true, false), "12.0 Kib");
    assert_eq!(
        format_quantity_with(2 * MIB, false, true, false),
        "16.0 Mib"
    );
    assert_eq!(format_quantity_with(0, false, true, false), "0 b");
    // Base-10 bytes.
    assert_eq!(format_quantity_with(0, true, false, false), "0 B");
    assert_eq!(format_quantity_with(999, true, false, false), "999 B");
    assert_eq!(format_quantity_with(1_500, true, false, false), "1.5 KB");
    assert_eq!(
        format_quantity_with(1_500_000, true, false, false),
        "1.5 MB"
    );
    assert_eq!(
        format_quantity_with(2_000_000_000, true, false, false),
        "2.0 GB"
    );
    // Base-10 bits.
    assert_eq!(format_quantity_with(1500, false, false, false), "12.0 Kb");
    assert_eq!(
        format_quantity_with(1_500_000, false, false, false),
        "12.0 Mb"
    );
}

/// The ladder tops out at the g-tier like the shared `bytes` formatter:
/// larger magnitudes keep dividing into the g-unit readout instead of
/// growing a new tier (documented quirk, preserved for parity).
#[test]
fn terabyte_and_above_stay_on_the_g_tier() {
    assert_eq!(format_quantity_with(TIB, true, true, false), "1024.0 GiB");
    assert_eq!(
        format_quantity_with(1_000_000_000_000, true, false, false),
        "1000.0 GB"
    );
}

/// Per-second quantities append `/s` after the unit on every ladder.
#[test]
fn per_second_appends_the_rate_suffix() {
    assert_eq!(format_quantity_with(1536, true, true, true), "1.5 KiB/s");
    assert_eq!(
        format_quantity_with(125_000, false, false, true),
        "1.0 Mb/s"
    );
    assert_eq!(format_quantity_with(100, false, false, true), "800 b/s");
    assert_eq!(format_quantity_with(0, false, false, true), "0 b/s");
}

/// The family entry resolves the right preference pair per family and the
/// pair formatter composes both sides on the same ladder.
#[test]
fn family_resolution_and_pairs() {
    let prefs = UnitPreferences {
        memory_use_base2: false,
        drive_use_bytes: false,
        drive_use_base2: false,
        network_use_bytes: true,
        network_use_base2: true,
        ..UnitPreferences::default()
    };
    // Memory keeps bytes but switches to base-10.
    assert_eq!(format_memory(MIB, &prefs), "1.0 MB");
    // Drive switches to bits, base-10.
    assert_eq!(
        format_quantity(1_500_000, QuantityFamily::Drive, false, &prefs),
        "12.0 Mb"
    );
    // Network switches to bytes, base-2.
    assert_eq!(
        format_quantity(1536, QuantityFamily::Network, false, &prefs),
        "1.5 KiB"
    );
    // Pairs format both sides on the family ladder.
    assert_eq!(
        format_quantity_pair(2 * GIB, 8 * GIB, QuantityFamily::Memory, false, &prefs),
        "2.1 GB / 8.6 GB"
    );
    assert_eq!(
        format_quantity_pair(0, GIB, QuantityFamily::Memory, false, &prefs),
        "0 B / 1.1 GB"
    );
}

/// Defaults are the Mission Center parity: memory/drive bytes+base-2,
/// network bits+base-10.
#[test]
fn defaults_follow_mission_center_parity() {
    let prefs = UnitPreferences::default();
    assert_eq!(format_memory(16 * GIB, &prefs), "16.0 GiB");
    assert_eq!(
        format_quantity(2_000_000_000, QuantityFamily::Drive, true, &prefs),
        "1.9 GiB/s"
    );
    assert_eq!(format_byte_rate(1_000_000, &prefs), "8.0 Mb/s");
    assert_eq!(format_byte_rate(125_000, &prefs), "1.0 Mb/s");
    assert_eq!(format_byte_rate(0, &prefs), "0 b/s");
}

/// `From<&Config>` extracts exactly the six persisted unit fields.
#[test]
fn config_maps_to_unit_preferences() {
    let mut config = Config::default();
    assert_eq!(UnitPreferences::from(&config), UnitPreferences::default());
    config.memory_use_bytes = false;
    config.drive_use_base2 = false;
    config.network_use_bytes = true;
    let prefs = UnitPreferences::from(&config);
    assert_eq!(prefs.settings(QuantityFamily::Memory), (false, true));
    assert_eq!(prefs.settings(QuantityFamily::Drive), (true, false));
    assert_eq!(prefs.settings(QuantityFamily::Network), (true, false));
}

/// Non-finite float inputs fail closed to the shared dash instead of a
/// fabricated number; finite graph samples project through the ladder.
#[test]
fn f64_entry_fails_closed_for_non_finite_samples() {
    let prefs = UnitPreferences::default();
    assert_eq!(
        format_quantity_f64(f64::NAN, QuantityFamily::Network, true, &prefs),
        "—"
    );
    assert_eq!(
        format_quantity_f64(f64::INFINITY, QuantityFamily::Memory, false, &prefs),
        "—"
    );
    // 1.0 decimal MB of network traffic on the default bits/base-10 pair.
    assert_eq!(
        format_quantity_f64(1_000_000.0, QuantityFamily::Network, true, &prefs),
        "8.0 Mb/s"
    );
}

/// Extreme counts never panic and stay finite: the ladder is pure float
/// arithmetic on the byte value (8× for bits stays representable).
#[test]
fn extreme_counts_never_panic() {
    let prefs = UnitPreferences::default();
    for value in [u64::MAX, u64::MAX - 1, 1 << 53, (1 << 53) + 1, 0, 1] {
        for family in [
            QuantityFamily::Memory,
            QuantityFamily::Drive,
            QuantityFamily::Network,
        ] {
            for per_second in [false, true] {
                let text = format_quantity(value, family, per_second, &prefs);
                assert!(!text.is_empty());
            }
        }
    }
    assert_eq!(format_memory(u64::MAX, &prefs), "17179869184.0 GiB");
}
