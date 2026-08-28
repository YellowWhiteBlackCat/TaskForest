//! GPUI composition for selecting a standalone or Wayland layer-shell host.
//!
//! The requested role remains the neutral `taskmanager-app-host` contract. This
//! module only maps the contract to GPUI's protocol-free adapter type and
//! provides an opt-in development switch for the first layer-shell slice.

use gpui::{
    LayerShellFallback, LayerShellKeyboardInteractivity, LayerShellLayer, LayerShellOptions,
};
#[cfg(target_os = "linux")]
use taskmanager_app_host::LayerShellSpec;
use taskmanager_app_host::WindowPresentation;
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
/// The GPUI Wayland adapter applies the contract's normal-window fallback when
/// the compositor does not expose the layer-shell global. Every other value,
/// including an unset variable, keeps the existing standalone host.
pub(crate) fn from_environment() -> WindowPresentation {
    #[cfg(target_os = "linux")]
    if std::env::var("TASKFOREST_WINDOW_HOST").as_deref() == Ok("layer-shell") {
        let Ok(spec) = LayerShellSpec::desktop_widget("taskforest-gpui-desktop-widget") else {
            warn!("unable to construct the layer-shell profile; using standalone host");
            return WindowPresentation::standalone();
        };

        return WindowPresentation::layer_shell(spec.with_keyboard_interactivity(
            taskmanager_app_host::LayerShellKeyboardInteractivity::OnDemand,
        ));
    }

    WindowPresentation::standalone()
}

/// Map the neutral app-host contract to GPUI's protocol-free window options.
pub(crate) fn to_gpui(presentation: &WindowPresentation) -> gpui::WindowPresentation {
    match presentation {
        WindowPresentation::Standalone => gpui::WindowPresentation::Standalone,
        WindowPresentation::LayerShell(spec) => {
            let options = LayerShellOptions::new(spec.namespace())
                .with_layer(match spec.layer() {
                    taskmanager_app_host::LayerShellLayer::Background => {
                        LayerShellLayer::Background
                    }
                    taskmanager_app_host::LayerShellLayer::Bottom => LayerShellLayer::Bottom,
                    taskmanager_app_host::LayerShellLayer::Top => LayerShellLayer::Top,
                    taskmanager_app_host::LayerShellLayer::Overlay => LayerShellLayer::Overlay,
                })
                .with_anchor(spec.anchor().bits())
                .with_size(spec.size().width(), spec.size().height())
                .with_margins((
                    spec.margins().top(),
                    spec.margins().right(),
                    spec.margins().bottom(),
                    spec.margins().left(),
                ))
                .with_exclusive_zone(spec.exclusive_zone())
                .with_keyboard_interactivity(match spec.keyboard_interactivity() {
                    taskmanager_app_host::LayerShellKeyboardInteractivity::None => {
                        LayerShellKeyboardInteractivity::None
                    }
                    taskmanager_app_host::LayerShellKeyboardInteractivity::Exclusive => {
                        LayerShellKeyboardInteractivity::Exclusive
                    }
                    taskmanager_app_host::LayerShellKeyboardInteractivity::OnDemand => {
                        LayerShellKeyboardInteractivity::OnDemand
                    }
                })
                .with_output(spec.output().name().map(str::to_owned))
                .with_fallback(match spec.fallback() {
                    taskmanager_app_host::LayerShellFallbackPolicy::NormalWindow => {
                        LayerShellFallback::NormalWindow
                    }
                    taskmanager_app_host::LayerShellFallbackPolicy::Unavailable => {
                        LayerShellFallback::Unavailable
                    }
                });

            gpui::WindowPresentation::LayerShell(options)
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/window_presentation_tests.rs"]
mod tests;
