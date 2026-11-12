use super::*;
use crate::asset_bytes;
use taskmanager_ui_contract::IconId;
use zhuge_ui_tokens::ALL_STATUS_KINDS;

/// Every transient status severity wears one semantic glyph, and no two
/// severities share it: the notice reads the same way in every frontend.
#[test]
fn every_status_kind_answers_with_its_own_semantic_icon() {
    let expected = [
        (StatusKind::Info, IconId::Health),
        (StatusKind::Success, IconId::CircleCheck),
        (StatusKind::Warning, IconId::TriangleAlert),
        (StatusKind::Danger, IconId::Alert),
    ];
    for (kind, icon) in expected {
        assert_eq!(status_icon(kind), icon, "{kind:?} must wear {icon:?}");
    }
}

/// The glyph a severity names must actually exist as an embedded asset: a
/// status binding that points at a missing plate would draw nothing.
#[test]
fn every_status_glyph_resolves_to_an_embedded_asset() {
    for kind in ALL_STATUS_KINDS {
        let icon = status_icon(kind);
        assert!(
            asset_bytes(icon).is_some(),
            "{kind:?} names {icon:?}, which has no embedded SVG"
        );
    }
}
