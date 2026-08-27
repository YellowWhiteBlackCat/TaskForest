//! Stable device-selection helpers for [`RootView`].
//!
//! `select_device` records the stable identity behind an index-based selection
//! (so the same physical device is reselected after hot-plug churn), and
//! `reconcile_device_selection` re-resolves that stored identity against the
//! current snapshot. Both are pure functions of the RootView's existing fields
//! and perform no I/O.

use super::navigation::StableDeviceKind;
use crate::gpui_app::sidebar::SelectedDevice;

use super::RootView;

impl RootView {
    pub fn select_device(&mut self, device: SelectedDevice) {
        self.selected = device;
        self.selected_device_missing = false;
        let selected = match device {
            SelectedDevice::Disk(index) => self
                .system_snapshot()
                .disks
                .get(index)
                .map(|item| (StableDeviceKind::Disk, item.device_id.clone())),
            SelectedDevice::Nic(index) => self.system_snapshot().networks.get(index).map(|item| {
                (
                    StableDeviceKind::Network,
                    item.device_id.as_ref().to_owned(),
                )
            }),
            SelectedDevice::Gpu(index) => self
                .system_snapshot()
                .gpu
                .get(index)
                .map(|item| (StableDeviceKind::Gpu, item.device_id.clone())),
            SelectedDevice::Battery(index) => self
                .power_supplies()
                .batteries
                .get(index)
                .map(|item| (StableDeviceKind::Battery, item.id.clone())),
            SelectedDevice::Fan(index) => self
                .sensors()
                .readings
                .iter()
                .filter(|reading| reading.quantity() == &crate::core::SensorQuantity::FanSpeed)
                .nth(index)
                .map(|reading| (StableDeviceKind::Fan, reading.id().to_owned())),
            SelectedDevice::Cpu | SelectedDevice::Memory => None,
        };
        if let Some((kind, id)) = selected {
            self.stable_device_kind = Some(kind);
            self.stable_device_selection.select(id);
        } else {
            self.stable_device_kind = None;
            self.stable_device_selection.clear();
        }
    }

    pub fn reconcile_device_selection(&mut self) {
        let resolved = match self.stable_device_kind {
            Some(StableDeviceKind::Disk) => self
                .stable_device_selection
                .resolve(
                    self.system_snapshot()
                        .disks
                        .iter()
                        .map(|item| item.device_id.as_str()),
                )
                .map(SelectedDevice::Disk),
            Some(StableDeviceKind::Network) => self
                .stable_device_selection
                .resolve(
                    self.system_snapshot()
                        .networks
                        .iter()
                        .map(|item| item.device_id.as_ref()),
                )
                .map(SelectedDevice::Nic),
            Some(StableDeviceKind::Gpu) => self
                .stable_device_selection
                .resolve(
                    self.system_snapshot()
                        .gpu
                        .iter()
                        .map(|item| item.device_id.as_str()),
                )
                .map(SelectedDevice::Gpu),
            Some(StableDeviceKind::Battery) => self
                .stable_device_selection
                .resolve(
                    self.power_supplies()
                        .batteries
                        .iter()
                        .map(|item| item.id.as_str()),
                )
                .map(SelectedDevice::Battery),
            Some(StableDeviceKind::Fan) => self
                .stable_device_selection
                .resolve(
                    self.sensors()
                        .readings
                        .iter()
                        .filter(|reading| {
                            reading.quantity() == &crate::core::SensorQuantity::FanSpeed
                        })
                        .map(crate::core::SensorReading::id),
                )
                .map(SelectedDevice::Fan),
            None => {
                self.select_device(self.selected);
                return;
            }
        };
        if let Some(device) = resolved {
            self.selected = device;
            self.selected_device_missing = false;
        } else {
            self.selected_device_missing = true;
        }
    }
}
