//! Frontend-local settings types: the persisted-token mutation enum, the
//! color-scheme choice, the device-visibility toggle vocabulary, and the
//! process-details section tabs. These are renderer-local vocabulary the
//! shared shell never sees (ADR-027), extracted from [`super`] so the state
//! module stays under the repository's source-size budget.

use taskmanager_theme::tokens::{MotionPolicy, UiSize};
use taskmanager_theme::{FontChoice, Skin};

use crate::i18n::Language;

/// The process-details modal's section tab (GPUI `ProcessDetailsSection`
/// parity: Overview / Performance / Command / Insights). Frontend-local —
/// the shell only knows the properties overlay is open, never which tab
/// renders.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DetailsSection {
    #[default]
    Overview,
    Performance,
    Command,
    Insights,
}

impl DetailsSection {
    /// Every tab in selector order (the anti-报菜名 rule: the tab row, the
    /// focus-target registry and the tests iterate this list).
    pub const ALL: [Self; 4] = [
        Self::Overview,
        Self::Performance,
        Self::Command,
        Self::Insights,
    ];

    /// Stable non-localized identifier for focus-operation IDs.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Performance => "performance",
            Self::Command => "command",
            Self::Insights => "insights",
        }
    }
}

/// One settings mutation: the persisted token change plus the resolved theme
/// axis. The iced frontend owns the token↔enum mapping (the same split as
/// `gpui_app::root`); `Config` itself stays opaque to this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsChange {
    Skin(Skin),
    Mode(ModeChoice),
    HighContrast(bool),
    UiFont(FontChoice),
    MonoFont(FontChoice),
    /// `true` = `"Compact"` row density.
    CompactDensity(bool),
    UiSize(UiSize),
    /// `true` = memory in bytes, `false` = bits.
    MemoryBytes(bool),
    /// `true` = base-2 (MiB/GiB) memory ladder, `false` = base-10 (MB/GB).
    MemoryBase2(bool),
    /// `true` = drive sizes/rates in bytes, `false` = bits.
    DriveBytes(bool),
    /// `true` = base-2 drive ladder, `false` = base-10.
    DriveBase2(bool),
    /// `true` = network sizes/rates in bytes, `false` = bits (GPUI default).
    NetworkBytes(bool),
    /// `true` = base-2 network ladder, `false` = base-10.
    NetworkBase2(bool),
    /// Automatic telemetry refresh interval in milliseconds (the Settings
    /// refresh chooser). Clamped to the shared policy bounds when applied.
    RefreshInterval(u64),
    /// The Performance graph window length (clamped 10..=600 like GPUI).
    GraphDataPoints(usize),
    /// `true` = draw graph polylines through a Catmull-Rom spline.
    /// `true` = network graphs scale to the observed peak; `false` = scale to
    /// the negotiated link speed when one is known.
    NetworkDynamicScaling(bool),
    /// Show or hide one Performance-page device family (persisted, GPUI
    /// parity). Hiding a device removes it from the device rail.
    ShowDevice(DeviceKind, bool),
    /// Text-rendering token; only the empty platform-default token is currently
    /// supported by the Iced renderer.
    TextRendering(&'static str),
    /// The animation (motion) preference: the shared policy every iced
    /// animation (modal entrances, the warm-up spinner) follows. Persisted
    /// through the core `MOTION_*` token vocabulary and applied by the
    /// configuration snapshot edge.
    Motion(MotionPolicy),
    /// Startup-page token (`""` remember last / `"performance"` / `"apps"`).
    StartupPage(&'static str),
    /// `true` = dim current zero-valued resource cells on the Apps page.
    GrayZeroValues(bool),
    /// Continuous collector and durable application history opt-in.
    ContinuousHistory(bool),
    /// Desktop notification delivery opt-in for fired alerts (BN-07).
    DesktopNotifications(bool),
    /// Quiet-hours start hour (0..=23); equal start/end = no quiet hours.
    QuietHoursStart(u8),
    /// Quiet-hours end hour (0..=23); equal start/end = no quiet hours.
    QuietHoursEnd(u8),
    Language(Language),
}

/// One Performance-page device family toggle (GPUI Settings `devices` group).
/// The network sub-classes resolve onto `NetworkMetrics::adapter_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Cpu,
    Memory,
    Disks,
    Network,
    NetworkWired,
    NetworkWireless,
    NetworkVpn,
    NetworkVirtual,
    NetworkOther,
    Gpus,
}

impl DeviceKind {
    /// Every device toggle, in Settings row order (the anti-报菜名 rule: the
    /// settings view, the config projection and the tests iterate this list).
    pub const ALL: [Self; 10] = [
        Self::Cpu,
        Self::Memory,
        Self::Disks,
        Self::Network,
        Self::NetworkWired,
        Self::NetworkWireless,
        Self::NetworkVpn,
        Self::NetworkVirtual,
        Self::NetworkOther,
        Self::Gpus,
    ];

    /// Stable non-localized identifier for the settings chooser section
    /// (`FocusTarget::SettingsChoice`), mirroring the GPUI switch ids
    /// (`device-cpu`, `network-wired`, …).
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Cpu => "device-cpu",
            Self::Memory => "device-memory",
            Self::Disks => "device-disks",
            Self::Network => "device-network",
            Self::NetworkWired => "network-wired",
            Self::NetworkWireless => "network-wireless",
            Self::NetworkVpn => "network-vpn",
            Self::NetworkVirtual => "network-virtual",
            Self::NetworkOther => "network-other",
            Self::Gpus => "device-gpus",
        }
    }
}

/// The color-scheme preference with the frontend-local `System` resolution.
/// `System` keeps the currently resolved mode when no native appearance
/// provider exists (the iced slice has none yet); the stored token is
/// `"System"` regardless. EyeForest is a product-owned palette shared with
/// GPUI and the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModeChoice {
    #[default]
    Light,
    Dark,
    EyeForest,
    System,
}

impl ModeChoice {
    /// The persisted token for one choice (matches the shared
    /// `Config::mode` vocabulary).
    pub const fn token(self) -> &'static str {
        match self {
            ModeChoice::Light => "Light",
            ModeChoice::Dark => "Dark",
            ModeChoice::EyeForest => "EyeForest",
            ModeChoice::System => "System",
        }
    }

    /// Parse a stored `Config::mode` token back into a choice; an empty or
    /// unknown token behaves like `System` (the legacy first-launch sentinel).
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        match token.to_ascii_lowercase().as_str() {
            "light" => ModeChoice::Light,
            "dark" => ModeChoice::Dark,
            "eyeforest" | "eye-forest" => ModeChoice::EyeForest,
            _ => ModeChoice::System,
        }
    }
}
