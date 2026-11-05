use std::collections::HashSet;
use std::str;

use taskmanager_assets::product::GPUI_APP_ID;
use taskmanager_assets::product::GPUI_NAME;
use taskmanager_assets::product::ICED_APP_ID;
use taskmanager_assets::product::ICED_NAME;
use taskmanager_assets::product::NAME;
use taskmanager_assets::product::TAGLINE_EN;
use taskmanager_assets::product::TAGLINE_ZH;
use taskmanager_assets::product::ZH_NAME;
use taskmanager_assets::{
    GLASS_NOISE_RGBA_SIZE, TASKMANAGER_ICON_PATHS, UI_ICON_RGBA_PATHS, UI_ICON_RGBA_SIZE,
    all_asset_paths, asset_bytes, embedded_fonts, glass_noise_rgba, product_logo_bytes,
    product_logo_svg, ui_icon_rgba,
};

#[test]
fn product_identity_uses_the_task_forest_brand() {
    assert_eq!(NAME, "TaskForest");
    assert_eq!(ZH_NAME, "任务森林");
    assert_eq!(GPUI_NAME, "TaskForestG");
    assert_eq!(ICED_NAME, "TaskForestI");
    assert_eq!(GPUI_APP_ID, "io.github.YellowWhiteBlackCat.TaskForestG");
    assert_eq!(ICED_APP_ID, "io.github.YellowWhiteBlackCat.TaskForestI");
    assert!(TAGLINE_EN.contains("system monitor"));
    assert!(TAGLINE_ZH.contains("护眼"));
}

#[test]
fn every_declared_path_is_unique_and_loadable() {
    let paths: Vec<_> = all_asset_paths().collect();
    let unique: HashSet<_> = paths.iter().copied().collect();

    assert_eq!(paths.len(), 105);
    assert_eq!(unique.len(), paths.len());
    for path in paths {
        assert!(
            asset_bytes(path).is_some(),
            "missing embedded asset: {path}"
        );
    }
    assert!(asset_bytes("icons/not-present.svg").is_none());
}

/// Every registered plate decodes to exactly one RGBA plate of the shared
/// icon edge size. The raster frontend's image store rejects any other
/// length, so a drifted bitmap must fail here instead of at draw time.
#[test]
fn every_registered_plate_is_exactly_one_rgba_plate() {
    let expected = UI_ICON_RGBA_SIZE as usize * UI_ICON_RGBA_SIZE as usize * 4;
    assert_eq!(
        expected, 5184,
        "the 36px plate edge carries 36*36*4 RGBA bytes"
    );
    let unique: HashSet<_> = UI_ICON_RGBA_PATHS.iter().copied().collect();
    assert_eq!(
        unique.len(),
        UI_ICON_RGBA_PATHS.len(),
        "a plate key is registered twice"
    );
    for path in UI_ICON_RGBA_PATHS {
        let rgba =
            ui_icon_rgba(path).unwrap_or_else(|| panic!("{path} is registered without a bitmap"));
        assert_eq!(
            rgba.len(),
            expected,
            "{path} plate is {} bytes, expected {expected}",
            rgba.len()
        );
    }
}

#[test]
fn glass_noise_is_one_tileable_neutral_rgba_plate() {
    let bytes = glass_noise_rgba();
    let expected = GLASS_NOISE_RGBA_SIZE as usize * GLASS_NOISE_RGBA_SIZE as usize * 4;
    assert_eq!(bytes.len(), expected);
    let mut non_transparent = 0;
    let mut min_alpha = u8::MAX;
    let mut max_alpha = u8::MIN;
    let mut min_luminance = u8::MAX;
    let mut max_luminance = u8::MIN;
    for &[red, green, blue, alpha] in bytes.as_chunks::<4>().0 {
        assert!(
            red >= 240 && green >= 240 && blue >= 240,
            "grain stays neutral/white so the tier ink is its only tint"
        );
        assert!(
            red == green && green == blue,
            "grain is neutral: ({red}, {green}, {blue})"
        );
        if alpha > 0 {
            non_transparent += 1;
        }
        min_alpha = min_alpha.min(alpha);
        max_alpha = max_alpha.max(alpha);
        min_luminance = min_luminance.min(red);
        max_luminance = max_luminance.max(red);
    }
    assert_eq!(non_transparent, expected / 4, "every pixel is opaque grain");
    assert!(
        max_alpha == min_alpha,
        "alpha is the renderer-owned strength, not baked into the plate"
    );
    assert!(
        max_luminance - min_luminance > 8,
        "the noise has visible luminance variation without becoming a wash"
    );
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
fn embedded_fonts_are_valid_truetype_in_role_order() {
    let blobs = embedded_fonts();
    // One blob per bundled role, UI first then monospace. The family NAMES are
    // the theme owner's public collection (ADR-063); the `taskmanager-gates`
    // census welds this count to it, so bytes and names cannot drift apart.
    assert_eq!(
        blobs.len(),
        2,
        "the bundled collection is one UI + one mono face"
    );
    for blob in &blobs {
        // TrueType collection / TrueType sfnt magic (0x00010000 or 'true').
        let magic = u32::from_be_bytes([blob[0], blob[1], blob[2], blob[3]]);
        assert!(
            magic == 0x0001_0000 || magic == 0x7472_7565,
            "font blob does not start with a TrueType magic: {magic:#010x}"
        );
        assert!(blob.len() > 1_000, "font blob is suspiciously small");
    }
}

#[test]
fn product_logo_is_valid_svg_and_non_empty() {
    let svg_str = product_logo_svg();
    assert!(svg_str.contains("<svg"));
    assert!(svg_str.contains("TaskForest"));

    let bytes = product_logo_bytes();
    assert_eq!(bytes, svg_str.as_bytes());

    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(bytes, &options);
    assert!(
        tree.is_ok(),
        "product_logo_svg must parse as valid vector tree"
    );
}
