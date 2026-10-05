//! Deterministic sidebar ordering for persisted and newly discovered devices.

use std::collections::HashSet;

use crate::core::config::SidebarDeviceOverrideConfig;

/// Resolve a persisted order against the currently discovered device keys.
/// Unknown/stale keys are ignored, duplicate persisted keys are applied once,
/// and new devices keep the caller's deterministic discovery order.
pub fn ordered_indices(keys: &[String], preferred: &[String]) -> Vec<usize> {
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
pub fn visible_with_override(
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

/// Keep persisted sidebar choices bounded and deterministic before they enter
/// the per-window render state. Unknown keys are deliberately retained: a
/// device can disappear temporarily and return later with the same stable key.
/// Empty keys and duplicate entries, however, are config corruption and must
/// not create duplicate rows or ambiguous visibility decisions.
pub const MAX_PERSISTED_SIDEBAR_KEYS: usize = 128;

pub fn normalize_sidebar_preferences(
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

pub fn set_sidebar_override(
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

pub fn reordered_sidebar_order(
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

/// Move a discovered device by one or more positions without losing absent-device choices.
pub fn move_sidebar_order(
    live: &[String],
    preferred: &[String],
    device: &str,
    delta: isize,
) -> Option<Vec<String>> {
    let mut order: Vec<_> = ordered_indices(live, preferred)
        .into_iter()
        .map(|index| live[index].clone())
        .collect();
    let from = order.iter().position(|key| key == device)?;
    let to = from
        .saturating_add_signed(delta)
        .min(order.len().saturating_sub(1));
    if from == to {
        return None;
    }
    let key = order.remove(from);
    order.insert(to, key);
    for absent in preferred {
        if !order.contains(absent) {
            order.push(absent.clone());
        }
    }
    Some(order)
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_config_sidebar_tests.rs"]
mod tests;
