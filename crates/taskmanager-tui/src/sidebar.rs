//! Stable device editing and render-only preference projection.

use crate::{PerfDevice, TuiApp, TuiSurface, TuiSurfaceKind};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use taskmanager_application::i18n::t;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::config::sidebar::{
    move_sidebar_order, ordered_indices, set_sidebar_override, visible_with_override,
};
use taskmanager_core::core::metrics::{DiskMetrics, GpuMetrics, NetworkAdapterType};
use taskmanager_core::core::sensors::SensorQuantity;

#[derive(Clone, Debug)]
pub(crate) struct SidebarEntry {
    pub(crate) key: String,
    pub(crate) device: PerfDevice,
    pub(crate) index: usize,
    pub(crate) label: String,
    pub(crate) visible: bool,
}

impl TuiApp {
    pub(crate) fn sidebar_disks(&self) -> Vec<&DiskMetrics> {
        let Some(snapshot) = self.projection().snapshot.as_ref() else {
            return Vec::new();
        };
        self.order_sidebar_rows(
            snapshot
                .disks
                .iter()
                .filter_map(|disk| {
                    let key = format!("disk:{}", disk.device_id);
                    self.sidebar_render_visible(&key, self.prefs.show[2])
                        .then_some((key, disk))
                })
                .collect(),
        )
    }

    pub(crate) fn sidebar_gpus(&self) -> Vec<&GpuMetrics> {
        let Some(snapshot) = self.projection().snapshot.as_ref() else {
            return Vec::new();
        };
        self.order_sidebar_rows(
            snapshot
                .gpu
                .iter()
                .filter_map(|gpu| {
                    let key = format!("gpu:{}", gpu.device_id);
                    self.sidebar_render_visible(&key, self.prefs.show[9])
                        .then_some((key, gpu))
                })
                .collect(),
        )
    }

    pub(crate) fn sidebar_visible(&self, key: &str, category: bool) -> bool {
        visible_with_override(key, category, &self.config_draft.sidebar_device_overrides)
    }

    pub(crate) fn sidebar_render_visible(&self, key: &str, category: bool) -> bool {
        self.sidebar_visible(key, category)
            && self
                .performance_device_key
                .as_deref()
                .is_none_or(|selected| selected == key)
    }

    pub(crate) fn order_sidebar_rows<T: Clone>(&self, rows: Vec<(String, T)>) -> Vec<T> {
        let keys: Vec<_> = rows.iter().map(|(key, _)| key.clone()).collect();
        ordered_indices(&keys, &self.config_draft.sidebar_order)
            .into_iter()
            .map(|index| rows[index].1.clone())
            .collect()
    }

    pub(crate) fn sidebar_entries(&self) -> Vec<SidebarEntry> {
        let show = &self.prefs.show;
        let mut entries = Vec::new();
        let mut add = |key: String, device, index, label, category| {
            let visible = self.sidebar_visible(&key, category);
            entries.push((
                key.clone(),
                SidebarEntry {
                    key,
                    device,
                    index,
                    label,
                    visible,
                },
            ));
        };
        add(
            "cpu".into(),
            PerfDevice::Cpu,
            0,
            t("common.cpu").into(),
            show[0],
        );
        add(
            "memory".into(),
            PerfDevice::Memory,
            0,
            t("common.memory").into(),
            show[1],
        );
        let projection = self.projection();
        if let Some(snapshot) = &projection.snapshot {
            for (index, disk) in snapshot.disks.iter().enumerate() {
                add(
                    format!("disk:{}", disk.device_id),
                    PerfDevice::Disk,
                    index,
                    disk.name.clone(),
                    show[2],
                );
            }
            for (index, network) in snapshot.networks.iter().enumerate() {
                let category = show[3]
                    && match network.adapter_type() {
                        NetworkAdapterType::Ethernet => show[4],
                        NetworkAdapterType::WiFi => show[5],
                        NetworkAdapterType::Vpn => show[6],
                        NetworkAdapterType::Virtual => show[7],
                        NetworkAdapterType::Unknown
                        | NetworkAdapterType::Loopback
                        | NetworkAdapterType::Other => show[8],
                    };
                add(
                    format!("network:{}", network.device_id),
                    PerfDevice::Network,
                    index,
                    network.interface_name.to_string(),
                    category,
                );
            }
            for (index, gpu) in snapshot.gpu.iter().enumerate() {
                add(
                    format!("gpu:{}", gpu.device_id),
                    PerfDevice::Gpu,
                    index,
                    gpu.marketing_name.as_ref().unwrap_or(&gpu.brand).clone(),
                    show[9],
                );
            }
        }
        if let Some(power) = &projection.power_supplies {
            for (index, battery) in power.batteries.iter().enumerate() {
                add(
                    format!("battery:{}", battery.id),
                    PerfDevice::Battery,
                    index,
                    battery.id.clone(),
                    true,
                );
            }
        }
        if let Some(sensors) = &projection.sensors {
            for (index, fan) in sensors
                .readings
                .iter()
                .filter(|reading| reading.quantity() == &SensorQuantity::FanSpeed)
                .enumerate()
            {
                add(
                    format!("fan:{}", fan.id()),
                    PerfDevice::Fan,
                    index,
                    fan.id().to_owned(),
                    true,
                );
            }
        }
        self.order_sidebar_rows(entries)
    }

    pub(crate) fn open_sidebar_editor(&mut self) {
        // Discard an unsubmitted Settings draft using its normal cancellation path.
        if self.local_surface_kind() == Some(TuiSurfaceKind::Settings) {
            self.cancel_settings();
        }
        let selected = self
            .sidebar_entries()
            .first()
            .map(|entry| entry.key.clone());
        self.open_local_surface(TuiSurface::SidebarEditor { selected });
    }

    pub(crate) fn handle_sidebar_key(&mut self, key: KeyEvent) {
        let Some(TuiSurface::SidebarEditor { selected }) = self.local_surface() else {
            return;
        };
        let selected = selected.clone();
        let entries = self.sidebar_entries();
        let index = selected
            .as_ref()
            .and_then(|key| entries.iter().position(|entry| &entry.key == key));
        match key.code {
            KeyCode::Esc => self.close_local_overlays(),
            KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End => {
                let index = match key.code {
                    KeyCode::Up => index.unwrap_or(0).saturating_sub(1),
                    KeyCode::Down => index
                        .unwrap_or(0)
                        .saturating_add(1)
                        .min(entries.len().saturating_sub(1)),
                    KeyCode::End => entries.len().saturating_sub(1),
                    _ => 0,
                };
                if let Some(TuiSurface::SidebarEditor { selected }) = self.local_surface_mut() {
                    *selected = entries.get(index).map(|entry| entry.key.clone());
                }
            }
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => {
                let Some(entry) = index.and_then(|index| entries.get(index)) else {
                    return;
                };
                let mut config = self.config_draft.clone();
                if key.code == KeyCode::Char(' ') {
                    set_sidebar_override(
                        &mut config.sidebar_device_overrides,
                        &entry.key,
                        !entry.visible,
                    );
                } else {
                    let live: Vec<_> = entries.iter().map(|entry| entry.key.clone()).collect();
                    let Some(order) = move_sidebar_order(
                        &live,
                        &config.sidebar_order,
                        &entry.key,
                        if key.code == KeyCode::Left { -1 } else { 1 },
                    ) else {
                        return;
                    };
                    config.sidebar_order = order;
                }
                self.commit_config_draft(config);
            }
            KeyCode::Enter => {
                if let Some(entry) = index
                    .and_then(|index| entries.get(index))
                    .filter(|entry| entry.visible)
                {
                    self.close_local_overlays();
                    let _ = self.apply_action(AppAction::SelectPage(AppPage::Performance));
                    self.select_perf_device(entry.device);
                    self.performance_device_key = Some(entry.key.clone());
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "../tests/gui/sidebar_tests.rs"]
mod tests;
