use super::*;

#[test]
fn invalid_native_requests_are_rejected_before_adapter_selection() {
    assert!(matches!(
        LayerShellSpec::new("  "),
        Err(LayerShellSpecError::EmptyNamespace)
    ));
    assert!(matches!(
        LayerShellOutput::named("\t"),
        Err(LayerShellSpecError::EmptyOutputName)
    ));

    let valid = LayerShellSpec::new("taskforest.panel").expect("valid namespace");
    assert!(matches!(
        valid.with_exclusive_zone(-2),
        Err(LayerShellSpecError::InvalidExclusiveZone(-2))
    ));
}

#[test]
fn surface_role_selection_is_explicit_and_lossless() {
    let spec = LayerShellSpec::new("taskforest.panel")
        .expect("valid namespace")
        .with_layer(LayerShellLayer::Overlay)
        .with_size(1_280, 720)
        .with_keyboard_interactivity(LayerShellKeyboardInteractivity::OnDemand);
    let requested = WindowPresentation::layer_shell(spec.clone());

    assert!(WindowPresentation::standalone().as_layer_shell().is_none());
    assert_eq!(requested.as_layer_shell(), Some(&spec));
}

#[test]
fn anchor_input_rejects_bits_outside_the_protocol_domain() {
    assert!(LayerShellAnchor::from_bits(0b1_0000).is_none());

    let anchored = LayerShellAnchor::TOP | LayerShellAnchor::BOTTOM;
    assert!(anchored.contains(LayerShellAnchor::TOP));
    assert!(anchored.contains(LayerShellAnchor::BOTTOM));
}

#[test]
fn compositor_selected_axes_require_opposite_anchors() {
    let spec = LayerShellSpec::new("taskforest.panel")
        .expect("valid namespace")
        .with_anchor(LayerShellAnchor::TOP | LayerShellAnchor::LEFT);

    assert_eq!(
        spec.validate(),
        Err(LayerShellSpecError::InvalidAnchorForZeroWidth)
    );
}

#[test]
fn desktop_widget_profile_has_bounded_geometry() {
    let spec = LayerShellSpec::desktop_widget("taskforest.widget").expect("valid namespace");

    assert_eq!(spec.layer(), LayerShellLayer::Top);
    assert_eq!(
        spec.anchor(),
        LayerShellAnchor::TOP | LayerShellAnchor::RIGHT
    );
    assert_eq!(spec.size(), LayerShellSize::new(520, 360));
    assert_eq!(spec.margins(), LayerShellMargins::new(16, 16, 16, 16));
    assert_eq!(spec.exclusive_zone(), 0);
    assert_eq!(spec.validate(), Ok(()));
}
