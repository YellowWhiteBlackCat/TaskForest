use std::collections::HashSet;

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

/// The semantic ids the owner chain introduced resolve through the registry
/// to the shared asset paths — the asset table knows each one, so every
/// frontend draws the same glyph for the same semantic id.
#[test]
fn the_owner_chain_icons_resolve_to_their_shared_asset_paths() {
    let expected = [
        (IconId::Terminal, icon_path::TERMINAL),
        (IconId::Display, icon_path::DISPLAY),
        (IconId::Docker, icon_path::DOCKER),
        (IconId::Podman, icon_path::PODMAN),
        (IconId::Lock, icon_path::LOCK),
    ];
    for (semantic, asset_path) in expected {
        assert_eq!(
            path(semantic),
            asset_path,
            "{semantic:?} must resolve to {asset_path}"
        );
        assert!(
            asset_bytes(semantic).is_some(),
            "{semantic:?} has no embedded SVG at {asset_path}"
        );
    }
}

/// Each of the new semantics owns a glyph of its own: two ids sharing one
/// path would let two frontends draw different marks for the same meaning.
#[test]
fn the_owner_chain_icons_never_share_one_glyph() {
    let ids = [
        IconId::Terminal,
        IconId::Display,
        IconId::Docker,
        IconId::Podman,
        IconId::Lock,
    ];
    let mut paths = HashSet::new();
    for semantic in ids {
        assert!(
            paths.insert(path(semantic)),
            "{semantic:?} reuses another semantic id's glyph"
        );
    }
}
