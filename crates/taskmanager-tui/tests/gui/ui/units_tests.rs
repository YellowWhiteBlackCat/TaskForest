use super::*;

/// Base-10 magnitude formatter kept for the historical call shape: `KB`/`MB`/
/// `GB` (bytes) or `Kb`/`Mb`/`Gb` (bits) on the decimal 1000 ladder. Delegates
/// to the neutral core single source.
fn base10_bytes(value: u64, as_bits: bool) -> String {
    format_quantity_with(value, !as_bits, false, false)
}

/// The full GPUI Settings Units matrix, same assertions as the iced
/// frontend's parity test.
#[test]
fn unit_matrix_preference_switches_bytes_bits_and_base() {
    // memory_text_pref: bytes/base-2, bytes/base-10, bits/base-10,
    // bits/base-2.
    assert_eq!(memory_text_pref(1536, true, true), "1.5 KiB");
    assert_eq!(memory_text_pref(1536, true, false), "1.5 KB");
    assert_eq!(memory_text_pref(1536, false, false), "12.3 Kb");
    assert_eq!(memory_text_pref(2 * 1024 * 1024, false, true), "16.0 Mib");
    // quantity_text_pref: the same matrix for drive/network quantities.
    assert_eq!(quantity_text_pref(1_500_000, true, false), "1.5 MB");
    assert_eq!(quantity_text_pref(1_500_000, false, false), "12.0 Mb");
    assert_eq!(quantity_text_pref(1_500_000, true, true), "1.4 MiB");
    // The optional variant keeps the honest dash for an unobserved value.
    assert_eq!(quantity_text_optional(None, true, true), "—");
    assert_eq!(quantity_text_optional(Some(1536), true, true), "1.5 KiB");
    // base10_bytes: the decimal ladder with the correct case per unit.
    assert_eq!(base10_bytes(0, false), "0 B");
    assert_eq!(base10_bytes(1500, true), "12.0 Kb");
    assert_eq!(base10_bytes(2_000_000_000, false), "2.0 GB");
}

/// Parity with the neutral core single source: a fixed input × preference
/// matrix must render byte-identical to calling `taskmanager-core`
/// directly (the TUI reaches it through the application re-export), so
/// the same data + preference renders the same string in every frontend.
#[test]
fn unit_matrix_is_byte_identical_to_the_core_single_source() {
    for value in [0, 100, 512, 1536, 125_000, 1_500_000, 2_000_000_000] {
        for (use_bytes, use_base2) in [(true, true), (true, false), (false, true), (false, false)] {
            assert_eq!(
                memory_text_pref(value, use_bytes, use_base2),
                format_quantity_with(value, use_bytes, use_base2, false),
                "memory {value} B ({use_bytes}, {use_base2})"
            );
            assert_eq!(
                quantity_text_pref(value, use_bytes, use_base2),
                format_quantity_with(value, use_bytes, use_base2, false),
                "quantity {value} B ({use_bytes}, {use_base2})"
            );
        }
    }
}
