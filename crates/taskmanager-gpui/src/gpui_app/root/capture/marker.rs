use super::CaptureScenario;
use taskmanager_theme::Theme;
use taskmanager_theme::{LightDark, Skin};

pub(super) fn emit_marker(event: &'static str, scenario: Option<CaptureScenario>) {
    tracing::info!(
        target: "taskmanager::capture",
        "CAPTURE_MARKER event={event} scenario={}",
        scenario.map(CaptureScenario::token).unwrap_or("standard")
    );
}

pub(super) fn emit_theme_marker(scenario: Option<CaptureScenario>, theme: &Theme) {
    tracing::info!(
        target: "taskmanager::capture",
        "CAPTURE_MARKER event=theme_ready scenario={} theme={} high_contrast={}",
        scenario.map(CaptureScenario::token).unwrap_or("standard"),
        theme_token(theme.skin, theme.mode),
        theme.hc
    );
}

const fn theme_token(skin: Skin, mode: LightDark) -> &'static str {
    match (skin, mode) {
        (Skin::Gnome, LightDark::Light) => "gnome-light",
        (Skin::Gnome, LightDark::Dark) => "gnome-dark",
        (Skin::Gnome, LightDark::EyeForest) => "gnome-eyeforest",
        (Skin::Kde, LightDark::Light) => "kde-light",
        (Skin::Kde, LightDark::Dark) => "kde-dark",
        (Skin::Kde, LightDark::EyeForest) => "kde-eyeforest",
        (Skin::Windows, LightDark::Light) => "windows-light",
        (Skin::Windows, LightDark::Dark) => "windows-dark",
        (Skin::Windows, LightDark::EyeForest) => "windows-eyeforest",
        (Skin::Macos, LightDark::Light) => "macos-light",
        (Skin::Macos, LightDark::Dark) => "macos-dark",
        (Skin::Macos, LightDark::EyeForest) => "macos-eyeforest",
    }
}

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_gpui_app_root_capture_marker_tests.rs"]
mod tests;
