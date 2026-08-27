//! Immutable Iced presentation projection derived from one canonical
//! configuration snapshot. It is replaced only together with that snapshot;
//! views cannot mutate it independently or turn it into a second settings
//! authority.

use super::DeviceKind;

/// Renderer-ready values resolved from one persisted `Config` snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationPreferences {
    /// Cached installed-font catalog for the Settings family pickers.
    pub(crate) font_availability: taskmanager_theme::FontAvailability,
    pub skin: String,
    pub mode: String,
    pub hc: bool,
    pub ui_font: String,
    pub mono_font: String,
    pub density: String,
    pub ui_size: String,
    pub memory_use_bytes: bool,
    /// Base-2 (MiB/GiB) vs base-10 (MB/GB) memory ladder.
    pub memory_use_base2: bool,
    /// Drive sizes/rates in bytes vs bits.
    pub drive_use_bytes: bool,
    /// Base-2 vs base-10 drive ladder.
    pub drive_use_base2: bool,
    /// Network sizes/rates in bytes vs bits (GPUI default: bits).
    pub network_use_bytes: bool,
    /// Base-2 vs base-10 network ladder.
    pub network_use_base2: bool,
    /// Automatic telemetry refresh interval (ms).
    pub refresh_ms: u64,
    /// Performance graph window length.
    pub graph_data_points: usize,
    /// Catmull-Rom smoothed graph polylines.
    /// Network graphs scale to the observed peak vs the link speed.
    pub network_dynamic_scaling: bool,
    /// Per-family Performance-page device visibility (GPUI Settings devices).
    pub show_cpu: bool,
    pub show_memory: bool,
    pub show_disks: bool,
    pub show_network: bool,
    pub show_network_wired: bool,
    pub show_network_wireless: bool,
    pub show_network_vpn: bool,
    pub show_network_virtual: bool,
    pub show_network_other: bool,
    pub show_gpus: bool,
    /// Resolved text-rendering token; Iced currently supports platform default
    /// only, so legacy subpixel/grayscale values normalize to empty.
    pub text_rendering: String,
    /// Resolved motion-preference token (normal / reduced / none); the
    /// process-wide policy is installed from the same snapshot.
    pub motion: String,
    /// Resolved startup-page token (remember last / performance / apps).
    pub startup_page: String,
    /// Mirror of the dim-zero-values Apps preference.
    pub gray_zero_values: bool,
    /// Continuous collector and durable replay preference.
    pub history_persistence: bool,
    /// Desktop notification opt-in (BN-07); `false` = never notify.
    pub notify_enabled: bool,
    /// Quiet-hours start hour (0..=23); equal start/end = no quiet hours.
    pub quiet_start: u8,
    /// Quiet-hours end hour (0..=23); equal start/end = no quiet hours.
    pub quiet_end: u8,
}

impl Default for PresentationPreferences {
    fn default() -> Self {
        Self {
            font_availability: crate::font_catalog::bundled_only(),
            skin: String::new(),
            mode: String::new(),
            hc: false,
            ui_font: String::new(),
            mono_font: String::new(),
            density: String::new(),
            ui_size: String::new(),
            memory_use_bytes: true,
            memory_use_base2: true,
            drive_use_bytes: true,
            drive_use_base2: true,
            network_use_bytes: false,
            network_use_base2: false,
            refresh_ms: 1000,
            graph_data_points: 60,
            network_dynamic_scaling: true,
            show_cpu: true,
            show_memory: true,
            show_disks: true,
            show_network: true,
            show_network_wired: true,
            show_network_wireless: true,
            show_network_vpn: true,
            show_network_virtual: true,
            show_network_other: true,
            show_gpus: true,
            text_rendering: String::new(),
            motion: "normal".to_string(),
            startup_page: String::new(),
            gray_zero_values: false,
            history_persistence: false,
            notify_enabled: false,
            quiet_start: 0,
            quiet_end: 0,
        }
    }
}

impl PresentationPreferences {
    /// Construct the initial projection with the one startup font snapshot.
    pub(crate) fn with_font_availability(
        font_availability: taskmanager_theme::FontAvailability,
    ) -> Self {
        Self {
            font_availability,
            ..Self::default()
        }
    }
}

impl PresentationPreferences {
    /// The current visibility of one device family. Iterating
    /// [`DeviceKind::ALL`] with this
    /// accessor covers every persisted field).
    #[must_use]
    pub fn device_visible(&self, kind: DeviceKind) -> bool {
        match kind {
            DeviceKind::Cpu => self.show_cpu,
            DeviceKind::Memory => self.show_memory,
            DeviceKind::Disks => self.show_disks,
            DeviceKind::Network => self.show_network,
            DeviceKind::NetworkWired => self.show_network_wired,
            DeviceKind::NetworkWireless => self.show_network_wireless,
            DeviceKind::NetworkVpn => self.show_network_vpn,
            DeviceKind::NetworkVirtual => self.show_network_virtual,
            DeviceKind::NetworkOther => self.show_network_other,
            DeviceKind::Gpus => self.show_gpus,
        }
    }
}
