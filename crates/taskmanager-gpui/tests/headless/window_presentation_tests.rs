use super::*;
// The neutral presentation contract is platform-independent, but the module
// above imports `LayerShellSpec` only for Linux (its only live consumer).
// Import it here from the app-host contract directly so these headless
// contract tests compile on every platform, not just where the gated
// re-import exists.
use taskmanager_app_host::LayerShellSpec;

#[test]
fn neutral_layer_profile_reaches_gpui_without_losing_role_or_policy() {
    let spec = LayerShellSpec::new("taskforest.panel")
        .expect("valid namespace")
        .with_layer(taskmanager_app_host::LayerShellLayer::Overlay)
        .with_anchor(
            taskmanager_app_host::LayerShellAnchor::TOP
                | taskmanager_app_host::LayerShellAnchor::RIGHT,
        )
        .with_size(1_280, 720)
        .with_margins(taskmanager_app_host::LayerShellMargins::new(1, 2, 3, 4))
        .with_exclusive_zone(32)
        .expect("valid exclusive zone")
        .with_keyboard_interactivity(
            taskmanager_app_host::LayerShellKeyboardInteractivity::OnDemand,
        )
        .with_output(
            taskmanager_app_host::LayerShellOutput::named("DP-1").expect("valid output name"),
        )
        .with_fallback(taskmanager_app_host::LayerShellFallbackPolicy::Unavailable);

    let gpui::WindowPresentation::LayerShell(options) =
        to_gpui(&WindowPresentation::layer_shell(spec))
    else {
        panic!("layer-shell contract must remain a layer-shell GPUI request");
    };

    assert_eq!(options.namespace(), "taskforest.panel");
    assert_eq!(options.layer(), LayerShellLayer::Overlay);
    assert_eq!(options.anchor(), 0b0011);
    assert_eq!(options.size(), (1_280, 720));
    assert_eq!(options.margins(), (1, 2, 3, 4));
    assert_eq!(options.exclusive_zone(), 32);
    assert_eq!(
        options.keyboard_interactivity(),
        LayerShellKeyboardInteractivity::OnDemand
    );
    assert_eq!(options.output(), Some("DP-1"));
    assert_eq!(options.fallback(), LayerShellFallback::Unavailable);
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
