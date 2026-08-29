//! Renderer-local resource, focus, and process-layout selectors.

use crate::TuiApp;

/// Frontend-local selector for the Performance page's resource detail model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerfDevice {
    Cpu,
    Memory,
    Disk,
    Network,
    Gpu,
    Battery,
    Fan,
}

impl PerfDevice {
    pub const ALL: [PerfDevice; 7] = [
        PerfDevice::Cpu,
        PerfDevice::Memory,
        PerfDevice::Disk,
        PerfDevice::Network,
        PerfDevice::Gpu,
        PerfDevice::Battery,
        PerfDevice::Fan,
    ];

    #[must_use]
    pub const fn label_key(self) -> &'static str {
        match self {
            PerfDevice::Cpu => "common.cpu",
            PerfDevice::Memory => "common.memory",
            PerfDevice::Disk => "common.disk",
            PerfDevice::Network => "sidebar.network",
            PerfDevice::Gpu => "common.gpu",
            PerfDevice::Battery => "common.battery",
            PerfDevice::Fan => "common.fan",
        }
    }

    #[must_use]
    pub fn from_digit(character: char) -> Option<Self> {
        let one_based = character.to_digit(10)? as usize;
        Self::ALL.get(one_based.checked_sub(1)?).copied()
    }
}

/// The keyboard focus target on the Applications page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FocusPanel {
    #[default]
    Table,
    Details,
}

impl FocusPanel {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Table => Self::Details,
            Self::Details => Self::Table,
        }
    }
}

impl TuiApp {
    /// Select one Performance resource and reset resource-local viewport
    /// intent so a scroll position from a dense CPU topology cannot leak into
    /// the next resource visit.
    pub(crate) fn select_perf_device(&mut self, device: PerfDevice) {
        self.perf_device = device;
        self.cpu_core_scroll = 0;
        self.gpu_engine_scroll = 0;
    }

    /// Move the CPU per-core viewport by logical grid rows. Paint clamps this
    /// intent against the current topology and terminal geometry.
    pub(crate) fn scroll_cpu_cores(&mut self, delta: isize) {
        if delta >= 0 {
            self.cpu_core_scroll = self.cpu_core_scroll.saturating_add(delta as usize);
        } else {
            self.cpu_core_scroll = self.cpu_core_scroll.saturating_sub(delta.unsigned_abs());
        }
    }

    /// Move the standard GPU engine viewport. Compact paint omits that region,
    /// so this intent can never displace the primary chart or fact strip.
    pub(crate) fn scroll_gpu_engines(&mut self, delta: isize) {
        if delta >= 0 {
            self.gpu_engine_scroll = self.gpu_engine_scroll.saturating_add(delta as usize);
        } else {
            self.gpu_engine_scroll = self.gpu_engine_scroll.saturating_sub(delta.unsigned_abs());
        }
    }

    /// Move the System section viewport. This is page navigation, not
    /// selection state: all typed facts remain in one fixed order.
    pub(crate) fn scroll_system(&mut self, delta: isize) {
        if delta >= 0 {
            self.system_scroll = self.system_scroll.saturating_add(delta as usize);
        } else {
            self.system_scroll = self.system_scroll.saturating_sub(delta.unsigned_abs());
        }
    }

    /// Performance resources allowed by preferences and currently available facts.
    #[must_use]
    pub fn visible_perf_devices(&self) -> Vec<PerfDevice> {
        let show = &self.prefs.show;
        let snapshot = self.shell.projection().snapshot.as_ref();
        let mut devices = Vec::new();
        if show[0] {
            devices.push(PerfDevice::Cpu);
        }
        if show[1] {
            devices.push(PerfDevice::Memory);
        }
        if show[2] && snapshot.is_some_and(|snapshot| !snapshot.disks.is_empty()) {
            devices.push(PerfDevice::Disk);
        }
        if show[3] && snapshot.is_some_and(|snapshot| !snapshot.networks.is_empty()) {
            devices.push(PerfDevice::Network);
        }
        if show[9] && snapshot.is_some_and(|snapshot| !snapshot.gpu.is_empty()) {
            devices.push(PerfDevice::Gpu);
        }
        if self
            .shell
            .projection()
            .power_supplies
            .as_ref()
            .is_some_and(|power| !power.batteries.is_empty())
        {
            devices.push(PerfDevice::Battery);
        }
        if self
            .shell
            .projection()
            .sensors
            .as_ref()
            .is_some_and(|sensors| {
                sensors.readings.iter().any(|reading| {
                    reading.quantity() == &taskmanager_core::core::sensors::SensorQuantity::FanSpeed
                })
            })
        {
            devices.push(PerfDevice::Fan);
        }
        devices
    }

    #[must_use]
    pub fn select_perf_device_digit(&self, character: char) -> Option<PerfDevice> {
        let one_based = character.to_digit(10)? as usize;
        self.visible_perf_devices()
            .get(one_based.checked_sub(1)?)
            .copied()
    }
}
