//! Frontend-local settings types: the persisted-token mutation enum, the
//! color-scheme choice, and the re-exports of the shared process-details
//! section vocabulary. These are renderer-local vocabulary the shared shell
//! never sees (ADR-027), extracted from [`super`] so the state module stays
//! under the repository's source-size budget.
//!
//! The device-visibility toggle vocabulary is NOT local: the ten category
//! switches are a product fact owned by
//! `taskforest_core::core::config::DeviceVisibilityKey`, so this crate
//! spends that owner instead of declaring its own list.

use taskmanager_theme::tokens::{MotionPolicy, UiSize};
use taskmanager_theme::{FontChoice, ModePreference, Skin};

use taskforest_core::core::config::DeviceVisibilityKey;
use taskmanager_application::i18n::Language;

/// One settings mutation: the persisted token change plus the resolved theme
/// axis. The token↔enum mapping is the theme owner's public codec (ADR-063);
/// `Config` itself stays opaque to this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsChange {
    Skin(Skin),
    Mode(ModePreference),
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
    /// `true` = ease the one-sample Performance graph window shift.
    SlidingGraphs(bool),
    /// `true` = network graphs scale to the observed peak; `false` = scale to
    /// the negotiated link speed when one is known.
    NetworkDynamicScaling(bool),
    /// Show or hide one Performance-page device family (persisted, GPUI
    /// parity). Hiding a device removes it from the device rail.
    ShowDevice(DeviceVisibilityKey, bool),
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
