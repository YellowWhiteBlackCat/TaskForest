use super::per_core_cell_label;

#[test]
fn per_core_cell_label_keeps_every_available_current_fact() {
    assert_eq!(
        per_core_cell_label(Some(42.0), Some(45.0), Some(3200)),
        "42 % · 3.20 GHz · 45 °C"
    );
    assert_eq!(
        per_core_cell_label(Some(7.0), Some(45.0), None),
        "7 % · 45 °C"
    );
    assert_eq!(
        per_core_cell_label(None, Some(45.0), None),
        "45 °C",
        "a lone temperature remains an honest current per-core fact"
    );
    assert_eq!(per_core_cell_label(None, None, None), "—");
}
