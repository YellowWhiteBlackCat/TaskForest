use super::*;

#[test]
fn details_text_contains_build_identity_and_distribution_metadata() {
    let text = details_text();
    assert!(text.starts_with(&format!("{}\n", product_name())));
    assert!(text.contains(VERSION));
    assert!(text.contains("Apache-2.0"));
    assert!(text.contains(REPOSITORY_URL));
}
