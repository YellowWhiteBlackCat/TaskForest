use super::*;

#[test]
fn every_semantic_icon_has_an_iced_asset_handle() {
    for id in IconId::ALL {
        assert!(
            taskmanager_icons::asset_bytes(id).is_some(),
            "Iced SVG adapter has no embedded asset for {id:?}"
        );
        let _element = icon(&Theme::dark(), id, 16.0);
    }
}
