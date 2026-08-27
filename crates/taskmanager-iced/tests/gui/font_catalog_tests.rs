use super::*;

#[test]
fn bundled_fixture_has_product_faces_but_no_custom_choices() {
    let availability = bundled_only();
    assert!(availability.embedded_fonts_ready());
    assert!(availability.custom_families().is_empty());
}
