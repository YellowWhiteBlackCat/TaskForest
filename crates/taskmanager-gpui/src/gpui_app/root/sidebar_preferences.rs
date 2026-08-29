//! Root-owned projection of persisted sidebar ordering and visibility choices.

use gpui::Context;
use std::collections::HashSet;

use crate::gpui_app::sidebar::NetworkVisibility;
use taskmanager_core::core::config::SidebarDeviceOverrideConfig;

use super::RootView;

/// Keep persisted sidebar choices bounded and deterministic before they enter
/// the per-window render state. Unknown keys are deliberately retained: a
/// device can disappear temporarily and return later with the same stable key.
/// Empty keys and duplicate entries, however, are config corruption and must
/// not create duplicate rows or ambiguous visibility decisions.
const MAX_PERSISTED_SIDEBAR_KEYS: usize = 128;

pub(super) fn normalize_sidebar_preferences(
    order: &[String],
    overrides: &[SidebarDeviceOverrideConfig],
) -> (Vec<String>, Vec<SidebarDeviceOverrideConfig>) {
    let mut normalized_order = Vec::with_capacity(order.len().min(MAX_PERSISTED_SIDEBAR_KEYS));
    let mut seen_order = HashSet::with_capacity(order.len().min(MAX_PERSISTED_SIDEBAR_KEYS));
    for key in order {
        let key = key.trim();
        if key.is_empty() || !seen_order.insert(key) {
            continue;
        }
        normalized_order.push(key.to_owned());
        if normalized_order.len() == MAX_PERSISTED_SIDEBAR_KEYS {
            break;
        }
    }

    let mut normalized_overrides =
        Vec::with_capacity(overrides.len().min(MAX_PERSISTED_SIDEBAR_KEYS));
    for entry in overrides {
        let device = entry.device.trim();
        if device.is_empty() {
            continue;
        }
        // The live edit path uses the same last-write-wins rule. Normalizing
        // here makes a hand-edited config behave exactly like a UI edit.
        if let Some(previous) = normalized_overrides
            .iter()
            .position(|candidate: &SidebarDeviceOverrideConfig| candidate.device == device)
        {
            normalized_overrides.remove(previous);
        }
        normalized_overrides.push(SidebarDeviceOverrideConfig {
            device: device.to_owned(),
            visible: entry.visible,
        });
        if normalized_overrides.len() > MAX_PERSISTED_SIDEBAR_KEYS {
            normalized_overrides.remove(0);
        }
    }

    (normalized_order, normalized_overrides)
}

impl RootView {
    /// Project category flags once for both wide sidebar and compact strip.
    pub(crate) const fn network_visibility(&self) -> NetworkVisibility {
        let devices = self.presentation.devices();
        NetworkVisibility {
            all: devices.network,
            wired: devices.network_wired,
            wireless: devices.network_wireless,
            vpn: devices.network_vpn,
            virtual_devices: devices.network_virtual,
            other: devices.network_other,
        }
    }

    /// Apply a concrete show/hide decision from the sidebar edit affordance.
    /// The explicit decision is retained even when a category is disabled, so
    /// re-enabling the category restores the user's per-device choice.
    pub fn set_sidebar_device_override(
        &mut self,
        device: &str,
        visible: bool,
        cx: &mut Context<Self>,
    ) {
        let mut sidebar = self.presentation.sidebar().clone();
        set_sidebar_override(&mut sidebar.device_overrides, device, visible);
        self.presentation.set_sidebar(sidebar);
        cx.notify();
    }

    /// Replace persisted concrete order through the same normalization used
    /// by config publications; transient edit/drag state is untouched.
    pub fn set_sidebar_order(&mut self, order: Vec<String>, cx: &mut Context<Self>) {
        let mut sidebar = self.presentation.sidebar().clone();
        sidebar.order = normalize_sidebar_preferences(&order, &[]).0;
        self.presentation.set_sidebar(sidebar);
        cx.notify();
    }

    /// Move one concrete sidebar key before another using the order captured
    /// by the render projection. Stale persisted keys are preserved after the
    /// live set so hiding/showing a device never silently erases its choice.
    pub(crate) fn move_sidebar_device(
        &mut self,
        dragged: &str,
        target: &str,
        live_order: &[String],
        cx: &mut Context<Self>,
    ) {
        let mut sidebar = self.presentation.sidebar().clone();
        let Some(order) = reordered_sidebar_order(live_order, &sidebar.order, dragged, target)
        else {
            return;
        };
        sidebar.order = order;
        self.presentation.set_sidebar(sidebar);
        cx.notify();
    }
}

fn set_sidebar_override(
    overrides: &mut Vec<SidebarDeviceOverrideConfig>,
    device: &str,
    visible: bool,
) {
    overrides.retain(|entry| entry.device != device);
    overrides.push(SidebarDeviceOverrideConfig {
        device: device.to_string(),
        visible,
    });
}

fn reordered_sidebar_order(
    live_order: &[String],
    persisted_order: &[String],
    dragged: &str,
    target: &str,
) -> Option<Vec<String>> {
    if dragged == target {
        return None;
    }
    let mut order = live_order.to_vec();
    for stale in persisted_order {
        if !order.iter().any(|key| key == stale) {
            order.push(stale.clone());
        }
    }
    let from = order.iter().position(|key| key == dragged)?;
    let item = order.remove(from);
    let to = order.iter().position(|key| key == target)?;
    order.insert(to, item);
    Some(order)
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_sidebar_preferences_tests.rs"]
mod tests;
