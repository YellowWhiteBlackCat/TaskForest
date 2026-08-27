use gpui::AssetSource;

use super::TaskManagerAssets;

#[test]
fn every_declared_path_is_loadable_through_gpui() {
    let assets = TaskManagerAssets;
    let paths: Vec<_> = taskmanager_assets::all_asset_paths().collect();

    assert_eq!(paths.len(), 101);
    for path in paths {
        assert!(assets.load(path).unwrap().is_some());
    }
    assert!(assets.load("icons/not-present.svg").unwrap().is_none());
}

#[test]
fn directory_listing_exposes_the_embedded_asset_tree() {
    let assets = TaskManagerAssets;
    let root: Vec<_> = assets
        .list("")
        .unwrap()
        .into_iter()
        .map(|path| path.to_string())
        .collect();

    assert_eq!(root, ["domain", "icons"]);
    assert_eq!(assets.list("icons").unwrap().len(), 86);
    assert_eq!(assets.list("domain/").unwrap().len(), 15);
}
