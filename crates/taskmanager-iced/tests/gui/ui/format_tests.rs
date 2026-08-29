use super::*;

/// Parity with the neutral core single source: a fixed input × preference
/// matrix must render byte-identical to calling `taskmanager-core`
/// directly, so the same data + preference renders the same string in
/// every frontend.
#[test]
fn quantity_ladders_are_byte_identical_to_the_core_single_source() {
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
