use std::collections::HashSet;
use std::str;

use taskmanager_assets::{
    EMBEDDED_FONT_FAMILIES, TASKMANAGER_ICON_PATHS, all_asset_paths, asset_bytes, embedded_fonts,
};

#[test]
fn product_identity_uses_the_task_forest_brand() {
    assert_eq!(taskmanager_assets::product::NAME, "TaskForest");
    assert_eq!(taskmanager_assets::product::ZH_NAME, "任务森林");
    assert_eq!(taskmanager_assets::product::GPUI_NAME, "TaskForestG");
    assert_eq!(taskmanager_assets::product::ICED_NAME, "TaskForestI");
    assert_eq!(
        taskmanager_assets::product::GPUI_APP_ID,
        "io.github.YellowWhiteBlackCat.TaskForestG"
    );
    assert_eq!(
        taskmanager_assets::product::ICED_APP_ID,
        "io.github.YellowWhiteBlackCat.TaskForestI"
    );
    assert!(taskmanager_assets::product::TAGLINE_EN.contains("system monitor"));
    assert!(taskmanager_assets::product::TAGLINE_ZH.contains("护眼"));
}

#[test]
fn every_declared_path_is_unique_and_loadable() {
    let paths: Vec<_> = all_asset_paths().collect();
    let unique: HashSet<_> = paths.iter().copied().collect();

    assert_eq!(paths.len(), 101);
    assert_eq!(unique.len(), paths.len());
    for path in paths {
        assert!(
            asset_bytes(path).is_some(),
            "missing embedded asset: {path}"
        );
    }
    assert!(asset_bytes("icons/not-present.svg").is_none());
}

#[test]
fn every_asset_is_valid_tintable_svg() {
    let options = usvg::Options::default();

    for path in all_asset_paths() {
        let bytes = asset_bytes(path).expect("declared asset must be embedded");
        let source = str::from_utf8(bytes).unwrap();
        assert!(
            source.is_ascii(),
            "{path} must not contain emoji or embedded text"
        );
        assert!(source.contains("<svg"), "{path} is not SVG");
        assert!(source.contains("currentColor"), "{path} is not tintable");
        assert!(
            !source.contains("fill=\"#"),
            "{path} has a fixed fill color"
        );
        assert!(
            !source.contains("stroke=\"#"),
            "{path} has a fixed stroke color"
        );
        assert!(!source.contains("<script"), "{path} contains a script");
        assert!(
            !source.contains("href="),
            "{path} contains an external reference"
        );
        usvg::Tree::from_data(bytes, &options)
            .unwrap_or_else(|error| panic!("{path} is invalid SVG: {error}"));
    }
}

#[test]
fn domain_asset_paths_are_complete() {
    assert_eq!(TASKMANAGER_ICON_PATHS.len(), 15);
    assert!(
        TASKMANAGER_ICON_PATHS
            .iter()
            .all(|path| path.starts_with("domain/"))
    );
    assert!(
        TASKMANAGER_ICON_PATHS
            .iter()
            .all(|path| asset_bytes(path).is_some())
    );
}

#[test]
fn embedded_fonts_are_valid_truetype_with_expected_families() {
    let blobs = embedded_fonts();
    assert_eq!(blobs.len(), 2);
    for blob in &blobs {
        // TrueType collection / TrueType sfnt magic (0x00010000 or 'true').
        let magic = u32::from_be_bytes([blob[0], blob[1], blob[2], blob[3]]);
        assert!(
            magic == 0x0001_0000 || magic == 0x7472_7565,
            "font blob does not start with a TrueType magic: {magic:#010x}"
        );
        assert!(blob.len() > 1_000, "font blob is suspiciously small");
    }
    assert!(EMBEDDED_FONT_FAMILIES.contains(&"MiSans VF"));
    assert!(EMBEDDED_FONT_FAMILIES.contains(&"Roboto Mono"));
}
