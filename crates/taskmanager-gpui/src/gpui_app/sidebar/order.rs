//! Deterministic sidebar ordering for persisted and newly discovered devices.

use std::collections::HashSet;

use crate::core::config::SidebarDeviceOverrideConfig;

/// Resolve a persisted order against the currently discovered device keys.
/// Unknown/stale keys are ignored, duplicate persisted keys are applied once,
/// and new devices keep the caller's deterministic discovery order.
pub(crate) fn ordered_indices(keys: &[String], preferred: &[String]) -> Vec<usize> {
    let mut indices = Vec::with_capacity(keys.len());
    let mut used = HashSet::with_capacity(keys.len());

    for wanted in preferred {
        let Some(index) = keys.iter().position(|key| key == wanted) else {
            continue;
        };
        if used.insert(index) {
            indices.push(index);
        }
    }

    for index in 0..keys.len() {
        if used.insert(index) {
            indices.push(index);
        }
    }
    indices
}

/// Apply Mission Center's precedence rule: a concrete device override wins
/// over its category switch; without one, the category policy remains in force.
pub(crate) fn visible_with_override(
    key: &str,
    category_visible: bool,
    overrides: &[SidebarDeviceOverrideConfig],
) -> bool {
    overrides
        .iter()
        .rev()
        .find(|override_entry| override_entry.device == key)
        .map_or(category_visible, |override_entry| override_entry.visible)
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_sidebar_order_tests.rs"]
mod tests;
