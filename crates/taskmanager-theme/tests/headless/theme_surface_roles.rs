use crate::surface::{GlassSurfaceRole, SurfaceElevation};
use crate::theme::GlassTier;

#[test]
fn every_surface_role_resolves_its_shared_tier_and_elevation() {
    let expected = [
        (
            GlassSurfaceRole::Chrome,
            GlassTier::Sidebar,
            SurfaceElevation::Inset,
        ),
        (
            GlassSurfaceRole::PageHeader,
            GlassTier::Card,
            SurfaceElevation::Inset,
        ),
        (
            GlassSurfaceRole::Menu,
            GlassTier::Card,
            SurfaceElevation::Floating,
        ),
        (
            GlassSurfaceRole::Toast,
            GlassTier::Card,
            SurfaceElevation::Floating,
        ),
        (
            GlassSurfaceRole::Tooltip,
            GlassTier::Card,
            SurfaceElevation::Floating,
        ),
        (
            GlassSurfaceRole::Dialog,
            GlassTier::Modal,
            SurfaceElevation::Floating,
        ),
        (
            GlassSurfaceRole::InlinePlot,
            GlassTier::Card,
            SurfaceElevation::Inset,
        ),
    ];

    assert_eq!(GlassSurfaceRole::ALL.len(), expected.len());
    for (role, tier, elevation) in expected {
        assert!(GlassSurfaceRole::ALL.contains(&role));
        assert_eq!(role.tier(), tier, "{role:?} tier");
        assert_eq!(role.elevation(), elevation, "{role:?} elevation");
        assert_eq!(
            role.elevation().casts_shadow(),
            elevation == SurfaceElevation::Floating,
            "{role:?} shadow projection"
        );
    }
}

#[test]
fn all_roles_are_covered_once() {
    for (index, role) in GlassSurfaceRole::ALL.iter().enumerate() {
        assert!(
            !GlassSurfaceRole::ALL[..index].contains(role),
            "role {role:?} appears more than once"
        );
    }
}
