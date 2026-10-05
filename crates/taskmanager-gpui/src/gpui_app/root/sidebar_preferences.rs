//! Root-owned projection of persisted sidebar ordering and visibility choices.

use gpui::Context;

use crate::gpui_app::sidebar::NetworkVisibility;
use taskmanager_core::core::config::sidebar::{
    normalize_sidebar_preferences, reordered_sidebar_order, set_sidebar_override,
};

use super::RootView;

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

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_sidebar_preferences_tests.rs"]
mod tests;
