//! test-intent: behavior

use super::*;

#[test]
fn build_metadata_and_copy_payload_have_one_product_authority() {
    let metadata = metadata("0.2.0", "Apache-2.0", "https://example.test/taskforest");
    let text = metadata.details_text();
    assert!(text.starts_with(&format!("{}\n", metadata.name)));
    assert!(text.contains("0.2.0"));
    assert!(text.contains("Apache-2.0"));
    assert!(text.contains("https://example.test/taskforest"));
}
