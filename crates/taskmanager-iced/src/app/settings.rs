//! Settings application + resolved-theme rebuilding for [`super::IcedApp`].
//! The persisted token mutation, the resolved-theme rebuild, and the theme
//! token↔enum mapping live here; extracted from [`super`] so the state module
//! stays the update/tick entry point.

use super::*;
use std::time::Duration;
use taskmanager_application::TelemetryInterval;
use taskmanager_theme::{
    FONT_MISANS_VF, FONT_ROBOTO_MONO, FontAvailability, FontPreference, resolve_fonts,
};

impl IcedApp {
    /// Apply one settings mutation through the bounded configuration client
    /// and rebuild the resolved theme snapshot. The
    /// theme swap alone re-renders every page (iced's `view` reads
    /// [`IcedApp::theme`] each frame).
    pub(super) fn apply_settings_change(&mut self, change: SettingsChange) {
        let mut config = self.config_draft();
        match change {
            SettingsChange::Skin(skin) => config.skin = skin.label().to_string(),
            SettingsChange::Mode(choice) => config.mode = choice.token().to_string(),
            SettingsChange::HighContrast(on) => config.hc = on,
            SettingsChange::UiFont(choice) => {
                config.ui_font = font_token(choice, taskmanager_theme::fonts::FONT_MISANS_VF);
            }
            SettingsChange::MonoFont(choice) => {
                config.mono_font = font_token(choice, taskmanager_theme::fonts::FONT_ROBOTO_MONO);
            }
            SettingsChange::CompactDensity(compact) => {
                config.density = if compact { "Compact" } else { "Comfortable" }.to_string();
            }
            SettingsChange::UiSize(size) => {
                config.ui_size = size.config_token().to_string();
            }
            SettingsChange::MemoryBytes(bytes) => config.memory_use_bytes = bytes,
            SettingsChange::MemoryBase2(base2) => config.memory_use_base2 = base2,
            SettingsChange::DriveBytes(bytes) => config.drive_use_bytes = bytes,
            SettingsChange::DriveBase2(base2) => config.drive_use_base2 = base2,
            SettingsChange::NetworkBytes(bytes) => config.network_use_bytes = bytes,
            SettingsChange::NetworkBase2(base2) => config.network_use_base2 = base2,
            SettingsChange::RefreshInterval(millis) => {
                config.refresh_ms = millis;
                self.shell
                    .set_telemetry_interval(TelemetryInterval::clamped(Duration::from_millis(
                        millis,
                    )));
            }
            SettingsChange::GraphDataPoints(points) => {
                config.graph_data_points = u32::try_from(points).unwrap_or(u32::MAX);
                // The persisted window sizes the SHARED history store the
                // Performance chart reads (G-02): one sanctioned series store
                // re-sized at the settings edge, keeping the newest samples.
                self.shell.set_history_capacity(points);
            }
            SettingsChange::NetworkDynamicScaling(on) => config.network_dynamic_scaling = on,
            SettingsChange::ShowDevice(kind, visible) => match kind {
                DeviceKind::Cpu => config.show_cpu = visible,
                DeviceKind::Memory => config.show_memory = visible,
                DeviceKind::Disks => config.show_disks = visible,
                DeviceKind::Network => config.show_network = visible,
                DeviceKind::NetworkWired => config.show_network_wired = visible,
                DeviceKind::NetworkWireless => config.show_network_wireless = visible,
                DeviceKind::NetworkVpn => config.show_network_vpn = visible,
                DeviceKind::NetworkVirtual => config.show_network_virtual = visible,
                DeviceKind::NetworkOther => config.show_network_other = visible,
                DeviceKind::Gpus => config.show_gpus = visible,
            },
            // Iced does not expose a portable text-raster mode API. Keep
            // legacy messages fail-closed at platform default rather than
            // persisting a token that cannot affect rendering.
            SettingsChange::TextRendering(_) => config.text_rendering.clear(),
            SettingsChange::Motion(policy) => {
                config.motion = super::motion::motion_token(policy).to_string();
            }
            SettingsChange::StartupPage(token) => config.startup_page = token.to_string(),
            SettingsChange::GrayZeroValues(on) => config.gray_zero_values = on,
            SettingsChange::ContinuousHistory(on) => config.history_persistence = on,
            SettingsChange::DesktopNotifications(on) => {
                config.notify_enabled = on;
            }
            SettingsChange::QuietHoursStart(hour) => {
                let start = u16::from(hour) * 60;
                let end = config.notify_quiet_hours.map_or(0, |(_, end)| end);
                config.notify_quiet_hours = (start != end).then_some((start, end));
            }
            SettingsChange::QuietHoursEnd(hour) => {
                let end = u16::from(hour) * 60;
                let start = config.notify_quiet_hours.map_or(0, |(start, _)| start);
                config.notify_quiet_hours = (start != end).then_some((start, end));
            }
            SettingsChange::Language(language) => {
                // The config snapshot application below updates the language
                // projection and shared catalog atomically with every other
                // persisted preference.
                config.language = Some(language.token().to_string());
            }
        }
        self.commit_config_draft(config);
    }

    /// Resolve one immutable coordinator snapshot into renderer-ready values.
    /// The caller commits this projection together with the canonical draft.
    pub(super) fn resolve_preferences_and_theme(
        &self,
        config: &Config,
    ) -> (PresentationPreferences, Theme) {
        let font_availability = self.configuration.preferences().font_availability.clone();
        let preferences = PresentationPreferences {
            font_availability: font_availability.clone(),
            skin: config.skin.clone(),
            mode: config.mode.clone(),
            hc: config.hc,
            ui_font: config.ui_font.clone(),
            mono_font: config.mono_font.clone(),
            density: config.density.clone(),
            ui_size: config.ui_size.clone(),
            memory_use_bytes: config.memory_use_bytes,
            memory_use_base2: config.memory_use_base2,
            drive_use_bytes: config.drive_use_bytes,
            drive_use_base2: config.drive_use_base2,
            network_use_bytes: config.network_use_bytes,
            network_use_base2: config.network_use_base2,
            refresh_ms: config.refresh_ms,
            graph_data_points: usize::try_from(config.graph_data_points).unwrap_or(60),
            network_dynamic_scaling: config.network_dynamic_scaling,
            show_cpu: config.show_cpu,
            show_memory: config.show_memory,
            show_disks: config.show_disks,
            show_network: config.show_network,
            show_network_wired: config.show_network_wired,
            show_network_wireless: config.show_network_wireless,
            show_network_vpn: config.show_network_vpn,
            show_network_virtual: config.show_network_virtual,
            show_network_other: config.show_network_other,
            show_gpus: config.show_gpus,
            text_rendering: String::new(),
            motion: config.motion.clone(),
            startup_page: config.startup_page.clone(),
            gray_zero_values: config.gray_zero_values,
            history_persistence: config.history_persistence,
            notify_enabled: config.notify_enabled,
            quiet_start: config
                .notify_quiet_hours
                .map_or(0, |(start, _)| u8::try_from(start / 60).unwrap_or(0)),
            quiet_end: config
                .notify_quiet_hours
                .map_or(0, |(_, end)| u8::try_from(end / 60).unwrap_or(0)),
        };
        let theme = build_resolved_theme(
            skin_from_token(config.skin.as_str()),
            ModeChoice::from_token(config.mode.as_str()),
            config.hc,
            config.ui_font.as_str(),
            config.mono_font.as_str(),
            &font_availability,
        );
        (preferences, theme)
    }
}

/// Parse a persisted `Config::skin` token into the theme enum. Unknown or
/// empty tokens fall back to GNOME (the theme crate's own fallback).
#[must_use]
pub(super) fn skin_from_token(token: &str) -> Skin {
    match token.to_ascii_lowercase().as_str() {
        "kde" => Skin::Kde,
        "windows" => Skin::Windows,
        "macos" => Skin::Macos,
        _ => Skin::Gnome,
    }
}

/// The persisted font token for one preference: the explicit system marker,
/// a selected system family, or the bundled family name.
fn font_token(choice: FontChoice, bundled_family: &'static str) -> String {
    match choice {
        FontChoice::System => "system".to_string(),
        FontChoice::Custom(family) => family.to_string(),
        FontChoice::Bundled => bundled_family.to_string(),
    }
}

/// Rebuild the resolved theme snapshot from the persisted config values.
/// Explicit `mode` tokens resolve to `Light`/`Dark`/`EyeForest` directly;
/// `"System"` follows the OS color-scheme observation through
/// [`super::appearance`] (Dark until the first platform event arrives).
/// Font tokens are checked against the cached installed-family catalog; an
/// empty token is the bundled product default and an unknown token degrades
/// to the skin's verified fallback.
#[must_use]
pub(super) fn build_resolved_theme(
    skin: Skin,
    mode: ModeChoice,
    high_contrast: bool,
    ui_font_token: &str,
    mono_font_token: &str,
    availability: &FontAvailability,
) -> Theme {
    Theme::build(
        skin,
        super::appearance::resolve_color_mode(mode),
        if high_contrast {
            HighContrast::On
        } else {
            HighContrast::Off
        },
        resolve_fonts(
            font_preference_from_tokens(ui_font_token, mono_font_token, availability),
            skin,
            availability,
        ),
    )
}

/// Convert persisted tokens into typed choices using only families observed by
/// the cached catalog. Empty tokens are the bundled product default; unknown
/// tokens degrade to `System` instead of becoming an unchecked font request.
fn font_preference_from_tokens(
    ui_token: &str,
    mono_token: &str,
    availability: &FontAvailability,
) -> FontPreference {
    let parse = |token: &str, bundled: &[&str]| {
        let token = token.trim();
        if token.is_empty() || bundled.iter().any(|name| name.eq_ignore_ascii_case(token)) {
            FontChoice::Bundled
        } else if token.eq_ignore_ascii_case("system") {
            FontChoice::System
        } else {
            availability.choice_for(token).unwrap_or(FontChoice::System)
        }
    };
    FontPreference {
        ui: parse(ui_token, &[FONT_MISANS_VF, FONT_ROBOTO_MONO]),
        mono: parse(mono_token, &[FONT_ROBOTO_MONO]),
    }
}
