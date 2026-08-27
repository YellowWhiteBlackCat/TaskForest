//! Private authority for one window's persisted presentation preferences.
//!
//! Runtime interaction state (page, focus, sidebar visibility/edit/drag,
//! scroll handles) deliberately stays on [`super::RootView`]. Config folds and
//! Settings actions replace or mutate the typed sections here, while renderers
//! consume immutable snapshots.

use gpui::{Context, Pixels, SharedString};

use crate::core::config::{
    COLOR_SCHEME_SYSTEM, STARTUP_PAGE_REMEMBER, SidebarDeviceOverrideConfig,
    TEXT_RENDERING_PLATFORM_DEFAULT,
};
use crate::gpui_app::formatting::DisplayUnits;
use crate::gpui_app::graph::DEFAULT_GRAPH_DATA_POINTS_CONFIG;
use crate::gpui_app::theme::{
    FontPreference, Skin,
    tokens::{RowDensity, UiSize},
};
use crate::i18n;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevicePreference {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitFamily {
    Memory,
    Drive,
    Network,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuantityNotation {
    Bytes,
    Bits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MagnitudeBase {
    Binary,
    Decimal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresentationFingerprint {
    pub(crate) appearance: u64,
    pub(crate) devices: u64,
    pub(crate) units: u64,
    pub(crate) graphs: u64,
    pub(crate) sidebar: u64,
    pub(crate) apps: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AppearancePreferences {
    pub(crate) font: FontPreference,
    pub(crate) density: RowDensity,
    pub(crate) ui_size: UiSize,
    pub(crate) skin: Option<Skin>,
    pub(crate) color_scheme: &'static str,
    pub(crate) text_rendering: &'static str,
    pub(crate) high_contrast: bool,
    pub(crate) language: Option<i18n::Language>,
}

impl Default for AppearancePreferences {
    fn default() -> Self {
        Self {
            font: FontPreference::default(),
            density: RowDensity::default(),
            ui_size: UiSize::default(),
            skin: None,
            color_scheme: COLOR_SCHEME_SYSTEM,
            text_rendering: TEXT_RENDERING_PLATFORM_DEFAULT,
            high_contrast: false,
            language: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DeviceVisibilityPreferences {
    pub(crate) cpu: bool,
    pub(crate) memory: bool,
    pub(crate) disks: bool,
    pub(crate) network: bool,
    pub(crate) network_wired: bool,
    pub(crate) network_wireless: bool,
    pub(crate) network_vpn: bool,
    pub(crate) network_virtual: bool,
    pub(crate) network_other: bool,
    pub(crate) gpus: bool,
}

impl Default for DeviceVisibilityPreferences {
    fn default() -> Self {
        Self {
            cpu: true,
            memory: true,
            disks: true,
            network: true,
            network_wired: true,
            network_wireless: true,
            network_vpn: true,
            network_virtual: true,
            network_other: true,
            gpus: true,
        }
    }
}

impl DeviceVisibilityPreferences {
    pub(crate) const fn visible(self, device: DevicePreference) -> bool {
        match device {
            DevicePreference::Cpu => self.cpu,
            DevicePreference::Memory => self.memory,
            DevicePreference::Disks => self.disks,
            DevicePreference::Network => self.network,
            DevicePreference::NetworkWired => self.network_wired,
            DevicePreference::NetworkWireless => self.network_wireless,
            DevicePreference::NetworkVpn => self.network_vpn,
            DevicePreference::NetworkVirtual => self.network_virtual,
            DevicePreference::NetworkOther => self.network_other,
            DevicePreference::Gpus => self.gpus,
        }
    }

    fn set_visible(&mut self, device: DevicePreference, visible: bool) {
        match device {
            DevicePreference::Cpu => self.cpu = visible,
            DevicePreference::Memory => self.memory = visible,
            DevicePreference::Disks => self.disks = visible,
            DevicePreference::Network => self.network = visible,
            DevicePreference::NetworkWired => self.network_wired = visible,
            DevicePreference::NetworkWireless => self.network_wireless = visible,
            DevicePreference::NetworkVpn => self.network_vpn = visible,
            DevicePreference::NetworkVirtual => self.network_virtual = visible,
            DevicePreference::NetworkOther => self.network_other = visible,
            DevicePreference::Gpus => self.gpus = visible,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GraphPreferences {
    pub(crate) data_points: u32,
    pub(crate) sliding: bool,
    pub(crate) network_dynamic_scaling: bool,
}

impl Default for GraphPreferences {
    fn default() -> Self {
        Self {
            data_points: DEFAULT_GRAPH_DATA_POINTS_CONFIG,
            sliding: false,
            network_dynamic_scaling: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SidebarPreferences {
    pub(crate) width: Pixels,
    pub(crate) order: Vec<String>,
    pub(crate) device_overrides: Vec<SidebarDeviceOverrideConfig>,
}

impl Default for SidebarPreferences {
    fn default() -> Self {
        Self {
            width: Pixels::from(260.0),
            order: Vec::new(),
            device_overrides: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresentationSnapshot {
    pub(crate) appearance: AppearancePreferences,
    pub(crate) devices: DeviceVisibilityPreferences,
    pub(crate) units: DisplayUnits,
    pub(crate) graphs: GraphPreferences,
    pub(crate) sidebar: SidebarPreferences,
    pub(crate) gray_zero_values: bool,
    pub(crate) startup_page: SharedString,
    pub(crate) fingerprint: PresentationFingerprint,
}

impl Default for PresentationSnapshot {
    fn default() -> Self {
        Self {
            appearance: AppearancePreferences::default(),
            devices: DeviceVisibilityPreferences::default(),
            units: DisplayUnits::default(),
            graphs: GraphPreferences::default(),
            sidebar: SidebarPreferences::default(),
            gray_zero_values: false,
            startup_page: SharedString::from(STARTUP_PAGE_REMEMBER),
            fingerprint: PresentationFingerprint::default(),
        }
    }
}

impl PresentationFingerprint {
    #[must_use]
    pub const fn appearance(self) -> u64 {
        self.appearance
    }

    #[must_use]
    pub const fn devices(self) -> u64 {
        self.devices
    }

    #[must_use]
    pub const fn units(self) -> u64 {
        self.units
    }

    #[must_use]
    pub const fn graphs(self) -> u64 {
        self.graphs
    }

    #[must_use]
    pub const fn sidebar(self) -> u64 {
        self.sidebar
    }

    #[must_use]
    pub const fn apps(self) -> u64 {
        self.apps
    }
}

impl PresentationSnapshot {
    #[must_use]
    pub const fn fingerprint(&self) -> PresentationFingerprint {
        self.fingerprint
    }

    #[must_use]
    pub const fn font_preference(&self) -> FontPreference {
        self.appearance.font
    }

    #[must_use]
    pub const fn density(&self) -> RowDensity {
        self.appearance.density
    }

    #[must_use]
    pub const fn ui_size(&self) -> UiSize {
        self.appearance.ui_size
    }

    #[must_use]
    pub const fn skin_preference(&self) -> Option<Skin> {
        self.appearance.skin
    }

    #[must_use]
    pub const fn color_scheme(&self) -> &'static str {
        self.appearance.color_scheme
    }

    #[must_use]
    pub const fn text_rendering(&self) -> &'static str {
        self.appearance.text_rendering
    }

    #[must_use]
    pub const fn high_contrast(&self) -> bool {
        self.appearance.high_contrast
    }

    #[must_use]
    pub const fn language(&self) -> Option<i18n::Language> {
        self.appearance.language
    }

    #[must_use]
    pub const fn device_visible(&self, device: DevicePreference) -> bool {
        self.devices.visible(device)
    }

    #[must_use]
    pub const fn unit_choices(&self, family: UnitFamily) -> (bool, bool) {
        match family {
            UnitFamily::Memory => (self.units.memory_use_bytes, self.units.memory_use_base2),
            UnitFamily::Drive => (self.units.drive_use_bytes, self.units.drive_use_base2),
            UnitFamily::Network => (self.units.network_use_bytes, self.units.network_use_base2),
        }
    }

    #[must_use]
    pub const fn graph_data_points(&self) -> u32 {
        self.graphs.data_points
    }

    #[must_use]
    pub const fn sliding_graphs(&self) -> bool {
        self.graphs.sliding
    }

    #[must_use]
    pub const fn network_dynamic_scaling(&self) -> bool {
        self.graphs.network_dynamic_scaling
    }

    #[must_use]
    pub fn sidebar_width(&self) -> Pixels {
        self.sidebar.width
    }

    #[must_use]
    pub fn sidebar_order(&self) -> &[String] {
        &self.sidebar.order
    }

    #[must_use]
    pub fn sidebar_device_overrides(&self) -> &[SidebarDeviceOverrideConfig] {
        &self.sidebar.device_overrides
    }

    #[must_use]
    pub const fn gray_zero_values(&self) -> bool {
        self.gray_zero_values
    }

    #[must_use]
    pub fn startup_page(&self) -> &str {
        self.startup_page.as_str()
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct PresentationPreferences {
    snapshot: PresentationSnapshot,
}

impl PresentationPreferences {
    pub(super) fn snapshot(&self) -> PresentationSnapshot {
        self.snapshot.clone()
    }

    pub(super) const fn fingerprint(&self) -> PresentationFingerprint {
        self.snapshot.fingerprint
    }

    pub(super) const fn appearance(&self) -> AppearancePreferences {
        self.snapshot.appearance
    }

    pub(super) const fn devices(&self) -> DeviceVisibilityPreferences {
        self.snapshot.devices
    }

    pub(super) const fn units(&self) -> DisplayUnits {
        self.snapshot.units
    }

    pub(super) const fn graphs(&self) -> GraphPreferences {
        self.snapshot.graphs
    }

    pub(super) fn sidebar(&self) -> &SidebarPreferences {
        &self.snapshot.sidebar
    }

    pub(super) fn replace(&mut self, mut next: PresentationSnapshot) {
        let current = &self.snapshot;
        let mut fingerprint = current.fingerprint;
        if next.appearance != current.appearance || next.startup_page != current.startup_page {
            fingerprint.appearance = bump(fingerprint.appearance);
        }
        if next.devices != current.devices {
            fingerprint.devices = bump(fingerprint.devices);
        }
        if next.units != current.units {
            fingerprint.units = bump(fingerprint.units);
        }
        if next.graphs != current.graphs {
            fingerprint.graphs = bump(fingerprint.graphs);
        }
        if next.sidebar != current.sidebar {
            fingerprint.sidebar = bump(fingerprint.sidebar);
        }
        if next.gray_zero_values != current.gray_zero_values {
            fingerprint.apps = bump(fingerprint.apps);
        }
        next.fingerprint = fingerprint;
        self.snapshot = next;
    }

    pub(super) fn set_device_visible(&mut self, device: DevicePreference, visible: bool) {
        if self.snapshot.devices.visible(device) == visible {
            return;
        }
        self.snapshot.devices.set_visible(device, visible);
        self.snapshot.fingerprint.devices = bump(self.snapshot.fingerprint.devices);
    }

    pub(super) fn set_quantity_notation(&mut self, family: UnitFamily, notation: QuantityNotation) {
        let use_bytes = notation == QuantityNotation::Bytes;
        let target = match family {
            UnitFamily::Memory => &mut self.snapshot.units.memory_use_bytes,
            UnitFamily::Drive => &mut self.snapshot.units.drive_use_bytes,
            UnitFamily::Network => &mut self.snapshot.units.network_use_bytes,
        };
        if *target != use_bytes {
            *target = use_bytes;
            self.snapshot.fingerprint.units = bump(self.snapshot.fingerprint.units);
        }
    }

    pub(super) fn set_magnitude_base(&mut self, family: UnitFamily, base: MagnitudeBase) {
        let use_base2 = base == MagnitudeBase::Binary;
        let target = match family {
            UnitFamily::Memory => &mut self.snapshot.units.memory_use_base2,
            UnitFamily::Drive => &mut self.snapshot.units.drive_use_base2,
            UnitFamily::Network => &mut self.snapshot.units.network_use_base2,
        };
        if *target != use_base2 {
            *target = use_base2;
            self.snapshot.fingerprint.units = bump(self.snapshot.fingerprint.units);
        }
    }

    pub(super) fn set_graphs(&mut self, graphs: GraphPreferences) {
        if self.snapshot.graphs != graphs {
            self.snapshot.graphs = graphs;
            self.snapshot.fingerprint.graphs = bump(self.snapshot.fingerprint.graphs);
        }
    }

    pub(super) fn set_sidebar(&mut self, sidebar: SidebarPreferences) {
        if self.snapshot.sidebar != sidebar {
            self.snapshot.sidebar = sidebar;
            self.snapshot.fingerprint.sidebar = bump(self.snapshot.fingerprint.sidebar);
        }
    }

    pub(super) fn set_appearance(&mut self, appearance: AppearancePreferences) {
        if self.snapshot.appearance != appearance {
            self.snapshot.appearance = appearance;
            self.snapshot.fingerprint.appearance = bump(self.snapshot.fingerprint.appearance);
        }
    }

    pub(super) fn set_startup_page(&mut self, startup_page: SharedString) {
        if self.snapshot.startup_page != startup_page {
            self.snapshot.startup_page = startup_page;
            self.snapshot.fingerprint.appearance = bump(self.snapshot.fingerprint.appearance);
        }
    }

    pub(super) fn set_gray_zero_values(&mut self, enabled: bool) {
        if self.snapshot.gray_zero_values != enabled {
            self.snapshot.gray_zero_values = enabled;
            self.snapshot.fingerprint.apps = bump(self.snapshot.fingerprint.apps);
        }
    }
}

const fn bump(revision: u64) -> u64 {
    revision.saturating_add(1)
}

impl super::RootView {
    pub fn presentation_snapshot(&self) -> PresentationSnapshot {
        self.presentation.snapshot()
    }

    pub(crate) const fn presentation_fingerprint(&self) -> PresentationFingerprint {
        self.presentation.fingerprint()
    }

    pub(crate) fn replace_presentation(&mut self, snapshot: PresentationSnapshot) {
        self.presentation.replace(snapshot);
    }

    pub(crate) const fn appearance_preferences(&self) -> AppearancePreferences {
        self.presentation.appearance()
    }

    pub fn set_device_visibility(
        &mut self,
        device: DevicePreference,
        visible: bool,
        cx: &mut Context<Self>,
    ) {
        self.presentation.set_device_visible(device, visible);
        cx.notify();
    }

    pub fn set_density(&mut self, density: RowDensity, cx: &mut Context<Self>) {
        let mut appearance = self.presentation.appearance();
        appearance.density = density;
        self.presentation.set_appearance(appearance);
        cx.notify();
    }

    pub fn set_ui_size(&mut self, ui_size: UiSize, cx: &mut Context<Self>) {
        let mut appearance = self.presentation.appearance();
        appearance.ui_size = ui_size;
        self.presentation.set_appearance(appearance);
        cx.notify();
    }

    pub fn set_text_rendering(&mut self, text_rendering: &'static str, cx: &mut Context<Self>) {
        let mut appearance = self.presentation.appearance();
        appearance.text_rendering = text_rendering;
        self.presentation.set_appearance(appearance);
        cx.notify();
    }

    pub fn set_startup_page_preference(
        &mut self,
        startup_page: SharedString,
        cx: &mut Context<Self>,
    ) {
        self.presentation.set_startup_page(startup_page);
        cx.notify();
    }

    pub fn set_gray_zero_values(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.presentation.set_gray_zero_values(enabled);
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_app/root/presentation_preferences_tests.rs"]
mod tests;
