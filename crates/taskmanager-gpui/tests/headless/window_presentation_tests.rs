use super::*;
#[cfg(target_os = "linux")]
use taskmanager_app_host::LayerShellAnchor;
#[cfg(target_os = "linux")]
use taskmanager_app_host::LayerShellKeyboardInteractivity;
#[cfg(target_os = "linux")]
use taskmanager_app_host::LayerShellLayer;
#[cfg(target_os = "linux")]
use taskmanager_app_host::LayerShellMargins;
// The neutral presentation contract is platform-independent, but the module
// above imports `LayerShellSpec` only for Linux (its only live consumer).
// Import it here from the app-host contract directly so these headless
// contract tests compile on every platform, not just where the gated
// re-import exists.
use taskmanager_app_host::LayerShellSpec;

#[cfg(target_os = "linux")]
#[test]
fn neutral_layer_profile_maps_to_gpui_layer_shell_fields() {
    let spec = LayerShellSpec::new("taskforest.panel")
        .expect("valid namespace")
        .with_layer(LayerShellLayer::Overlay)
        .with_anchor(LayerShellAnchor::TOP | LayerShellAnchor::RIGHT)
        .with_size(1_280, 720)
        .with_margins(LayerShellMargins::new(1, 2, 3, 4))
        .with_exclusive_zone(32)
        .expect("valid exclusive zone")
        .with_keyboard_interactivity(LayerShellKeyboardInteractivity::OnDemand);

    let gpui::WindowKind::LayerShell(options) = to_gpui(&WindowPresentation::layer_shell(spec))
    else {
        panic!("layer-shell contract must remain a layer-shell GPUI request");
    };

    use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer};
    assert_eq!(options.namespace, "taskforest.panel");
    assert_eq!(options.layer, Layer::Overlay);
    assert!(options.anchor.contains(Anchor::TOP | Anchor::RIGHT));
    assert_eq!(options.exclusive_zone, Some(gpui::px(32.0)));
    assert_eq!(
        options.margin,
        Some((gpui::px(1.0), gpui::px(2.0), gpui::px(3.0), gpui::px(4.0)))
    );
    assert_eq!(
        options.keyboard_interactivity,
        KeyboardInteractivity::OnDemand
    );
}

#[test]
fn surface_role_keeps_standalone_default_and_selects_widget_explicitly() {
    assert_eq!(
        surface_role(&WindowPresentation::standalone()),
        GpuiSurfaceRole::Standalone
    );

    let spec = LayerShellSpec::desktop_widget("taskforest.widget").expect("valid namespace");
    assert_eq!(
        surface_role(&WindowPresentation::layer_shell(spec)),
        GpuiSurfaceRole::DesktopWidget
    );
}
