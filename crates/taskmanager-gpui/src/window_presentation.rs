//! GPUI composition for selecting a standalone or Wayland layer-shell host.
//!
//! The requested role remains the neutral `taskmanager-app-host` contract. This
//! module only maps the contract to GPUI's protocol-free adapter type and
//! provides an opt-in development switch for the first layer-shell slice.

use gpui::WindowKind;
#[cfg(target_os = "linux")]
use gpui::{WindowBounds, point, px, size};
#[cfg(target_os = "linux")]
use taskmanager_app_host::LayerShellFallbackPolicy;
#[cfg(target_os = "linux")]
use taskmanager_app_host::LayerShellSpec;
use taskmanager_app_host::WindowPresentation;
#[cfg(target_os = "linux")]
use taskmanager_app_host::{LayerShellAnchor, LayerShellKeyboardInteractivity, LayerShellLayer};
#[cfg(target_os = "linux")]
use tracing::warn;

/// Content mode selected alongside the GPUI surface host.
///
/// The normal application remains the default. The widget mode is only
/// selected by the explicit layer-shell opt-in, so standalone launches keep
/// the existing root layout and window behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GpuiSurfaceRole {
    #[default]
    Standalone,
    DesktopWidget,
}

/// Derive the frontend content role from the neutral host request.
#[must_use]
pub(crate) fn surface_role(presentation: &WindowPresentation) -> GpuiSurfaceRole {
    match presentation {
        WindowPresentation::Standalone => GpuiSurfaceRole::Standalone,
        WindowPresentation::LayerShell(_) => GpuiSurfaceRole::DesktopWidget,
    }
}

/// Read the opt-in GPUI host selection from the process environment.
///
/// `TASKFOREST_WINDOW_HOST=layer-shell` requests the fixed-size desktop widget
/// layer surface on Linux.
/// The GPUI Wayland adapter uses the native layer-shell window kind. Every
/// other value, including an unset variable, keeps the existing standalone host.
pub(crate) fn from_environment() -> WindowPresentation {
    #[cfg(target_os = "linux")]
    if std::env::var("TASKFOREST_WINDOW_HOST").as_deref() == Ok("layer-shell") {
        let Ok(spec) = LayerShellSpec::desktop_widget("taskforest-gpui-desktop-widget") else {
            warn!("unable to construct the layer-shell profile; using standalone host");
            return WindowPresentation::standalone();
        };

        return WindowPresentation::layer_shell(
            spec.with_keyboard_interactivity(LayerShellKeyboardInteractivity::OnDemand),
        );
    }

    WindowPresentation::standalone()
}

/// Map the neutral app-host contract to GPUI's platform window kind.
pub(crate) fn to_gpui(presentation: &WindowPresentation) -> WindowKind {
    match presentation {
        WindowPresentation::Standalone => WindowKind::Normal,
        #[cfg(target_os = "linux")]
        WindowPresentation::LayerShell(spec) => {
            use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};

            let mut anchor = Anchor::empty();
            if spec.anchor().contains(LayerShellAnchor::TOP) {
                anchor |= Anchor::TOP;
            }
            if spec.anchor().contains(LayerShellAnchor::BOTTOM) {
                anchor |= Anchor::BOTTOM;
            }
            if spec.anchor().contains(LayerShellAnchor::LEFT) {
                anchor |= Anchor::LEFT;
            }
            if spec.anchor().contains(LayerShellAnchor::RIGHT) {
                anchor |= Anchor::RIGHT;
            }

            WindowKind::LayerShell(LayerShellOptions {
                namespace: spec.namespace().to_owned(),
                layer: match spec.layer() {
                    LayerShellLayer::Background => Layer::Background,
                    LayerShellLayer::Bottom => Layer::Bottom,
                    LayerShellLayer::Top => Layer::Top,
                    LayerShellLayer::Overlay => Layer::Overlay,
                },
                anchor,
                exclusive_zone: Some(px(spec.exclusive_zone() as f32)),
                exclusive_edge: None,
                margin: Some((
                    px(spec.margins().top() as f32),
                    px(spec.margins().right() as f32),
                    px(spec.margins().bottom() as f32),
                    px(spec.margins().left() as f32),
                )),
                keyboard_interactivity: match spec.keyboard_interactivity() {
                    LayerShellKeyboardInteractivity::None => KeyboardInteractivity::None,
                    LayerShellKeyboardInteractivity::Exclusive => KeyboardInteractivity::Exclusive,
                    LayerShellKeyboardInteractivity::OnDemand => KeyboardInteractivity::OnDemand,
                },
            })
        }
        #[cfg(not(target_os = "linux"))]
        WindowPresentation::LayerShell(_) => WindowKind::Normal,
    }
}

/// Apply a presentation to a GPUI window, including output selection and
/// layer-shell dimensions that now live on `WindowOptions`.
pub(crate) fn apply_to_window_options(
    presentation: &WindowPresentation,
    options: &mut gpui::WindowOptions,
    cx: &gpui::App,
) -> Result<(), String> {
    options.kind = to_gpui(presentation);
    #[cfg(target_os = "linux")]
    if let WindowPresentation::LayerShell(spec) = presentation {
        if let Some(output_name) = spec.output().name() {
            let expected = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, output_name.as_bytes());
            let display = cx
                .displays()
                .into_iter()
                .find(|display| display.uuid().is_ok_and(|id| id == expected));
            if let Some(display) = display {
                options.display_id = Some(display.id());
            } else if spec.fallback() == LayerShellFallbackPolicy::NormalWindow {
                options.kind = WindowKind::Normal;
            } else {
                return Err(format!(
                    "requested layer-shell output {output_name:?} is unavailable"
                ));
            }
        }
        if matches!(options.kind, WindowKind::LayerShell(_)) {
            let surface_size = spec.size();
            options.window_bounds = Some(WindowBounds::Windowed(gpui::Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(
                    px(surface_size.width() as f32),
                    px(surface_size.height() as f32),
                ),
            }));
        }
    }
    let _ = cx;
    Ok(())
}

/// Whether this presentation allows retrying through a standard desktop window.
#[must_use]
pub(crate) fn has_normal_window_fallback(presentation: &WindowPresentation) -> bool {
    #[cfg(target_os = "linux")]
    {
        matches!(
            presentation,
            WindowPresentation::LayerShell(spec)
                if spec.fallback() == LayerShellFallbackPolicy::NormalWindow
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = presentation;
        false
    }
}

/// Whether a failed GPUI layer-shell creation should use its contract fallback.
#[must_use]
pub(crate) fn should_fallback_to_normal_window(
    presentation: &WindowPresentation,
    error: &(dyn std::error::Error + 'static),
) -> bool {
    #[cfg(target_os = "linux")]
    {
        if !has_normal_window_fallback(presentation) {
            return false;
        }
        error_chain_has_layer_shell_unavailable(error)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (presentation, error);
        false
    }
}

#[cfg(target_os = "linux")]
fn error_chain_has_layer_shell_unavailable(error: &(dyn std::error::Error + 'static)) -> bool {
    if error
        .downcast_ref::<gpui::layer_shell::LayerShellNotSupportedError>()
        .is_some()
    {
        return true;
    }
    error
        .source()
        .is_some_and(error_chain_has_layer_shell_unavailable)
}

#[cfg(test)]
#[path = "../tests/headless/window_presentation_tests.rs"]
mod tests;
