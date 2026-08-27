use super::*;

#[test]
fn every_semantic_icon_resolves_to_embedded_svg() {
    for semantic in IconId::ALL {
        assert!(
            asset_bytes(semantic).is_some(),
            "missing SVG for {semantic:?}"
        );
    }
}
