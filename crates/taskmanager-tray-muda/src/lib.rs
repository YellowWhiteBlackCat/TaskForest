//! Shared muda/tray-icon bridge for the Windows and macOS tray adapters.
//!
//! The neutral TrayMenuSpec (from `taskmanager-core`) is rendered to a
//! native `tray_icon::menu::Menu` exactly once here, then attached to the
//! tray icon by whichever OS adapter hosts it. Menu id encoding and radio
//! exclusivity also live here so the two adapters cannot drift.
//!
//! This crate is `#![forbid(unsafe_code)]`; `tray-icon`/`muda` own all native
//! surface behind their safe public APIs. On targets without a tray
//! (`#[cfg(not(any(windows, macos)))]`) the native bridge is dormant and
//! `build_menu` reports a typed `TrayFailure::Unsupported`.

#![forbid(unsafe_code)]

#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::collections::HashMap;

use taskmanager_core::tray::TrayActionId;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use taskmanager_core::tray::{TrayMenuItem, TrayMenuSpec};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use taskmanager_platform_contract::TrayFailure;

/// Prefix of the native menu ids so inbound `tray_icon::menu::MenuEvent`s
/// can be attributed to our tray.
const MENU_ID_PREFIX: &str = "taskmanager:";

/// Encode a neutral action id into the native menu id.
#[must_use]
pub fn menu_id_for(id: TrayActionId) -> String {
    format!("{MENU_ID_PREFIX}{id}")
}

/// Decode a native menu id back to the neutral action id, if it is ours.
#[must_use]
pub fn decode_menu_id(value: &str) -> Option<TrayActionId> {
    value.strip_prefix(MENU_ID_PREFIX)?.parse().ok()
}

/// Check-state map for every checkmark and radio item plus radio-group
/// membership. `set_checked` enforces one-selected-per-group.
#[cfg(any(target_os = "windows", target_os = "macos"))]
#[derive(Default)]
pub struct RadioState {
    check_items: HashMap<TrayActionId, tray_icon::menu::CheckMenuItem>,
    radio_group_of: HashMap<TrayActionId, u32>,
    radio_groups: HashMap<u32, Vec<TrayActionId>>,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl RadioState {
    /// Apply a neutral state change to the native check items.
    pub fn set_checked(&self, id: TrayActionId, checked: bool) {
        if let Some(&group) = self.radio_group_of.get(&id) {
            if checked {
                if let Some(ids) = self.radio_groups.get(&group) {
                    for other in ids {
                        if let Some(item) = self.check_items.get(other) {
                            item.set_checked(*other == id);
                        }
                    }
                }
            } else if let Some(item) = self.check_items.get(&id) {
                item.set_checked(false);
            }
        } else if let Some(item) = self.check_items.get(&id) {
            item.set_checked(checked);
        }
    }

    /// Read the native checked state of a known checkmark or radio item.
    pub fn is_checked(&self, id: TrayActionId) -> bool {
        self.check_items
            .get(&id)
            .map(tray_icon::menu::CheckMenuItem::is_checked)
            .unwrap_or(false)
    }
}

/// A native menu plus the state needed to update its check items later.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub struct MudaMenu {
    pub menu: tray_icon::menu::Menu,
    pub radio: RadioState,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub use native::build_menu;

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod native {
    use super::*;
    use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

    /// Build the native menu tree for a validated neutral menu spec.
    pub fn build_menu(menu: &TrayMenuSpec) -> Result<MudaMenu, TrayFailure> {
        let native_menu = Menu::new();
        let mut radio = RadioState::default();
        append_items(&native_menu, menu.items(), &mut radio)?;
        Ok(MudaMenu {
            menu: native_menu,
            radio,
        })
    }

    /// Trait so the same builder code fills a root `Menu` or a nested
    /// `Submenu`.
    trait MenuHost {
        fn append_item(&self, item: &dyn tray_icon::menu::IsMenuItem) -> Result<(), TrayFailure>;
    }

    impl MenuHost for Menu {
        fn append_item(&self, item: &dyn tray_icon::menu::IsMenuItem) -> Result<(), TrayFailure> {
            self.append(item).map_err(|_| TrayFailure::Rejected)
        }
    }

    impl MenuHost for Submenu {
        fn append_item(&self, item: &dyn tray_icon::menu::IsMenuItem) -> Result<(), TrayFailure> {
            self.append(item).map_err(|_| TrayFailure::Rejected)
        }
    }

    fn append_items<H: MenuHost>(
        host: &H,
        items: &[TrayMenuItem],
        radio: &mut RadioState,
    ) -> Result<(), TrayFailure> {
        for item in items {
            match item {
                TrayMenuItem::Separator => {
                    host.append_item(&PredefinedMenuItem::separator())?;
                }
                TrayMenuItem::Action { id, label, enabled } => {
                    let native = MenuItem::with_id(menu_id_for(*id), label.clone(), *enabled, None);
                    host.append_item(&native)?;
                }
                TrayMenuItem::Checkmark {
                    id,
                    label,
                    checked,
                    enabled,
                } => {
                    let native = CheckMenuItem::with_id(
                        menu_id_for(*id),
                        label.clone(),
                        *enabled,
                        *checked,
                        None,
                    );
                    host.append_item(&native)?;
                    radio.check_items.insert(*id, native);
                }
                TrayMenuItem::Radio {
                    id,
                    label,
                    checked,
                    enabled,
                    radio_group,
                } => {
                    // muda has no radio item; both native menu systems draw
                    // every check as a check mark, so a radio maps to a check
                    // item and the host enforces exclusivity via RadioState.
                    let group = radio_group.unwrap_or(*id);
                    let native = CheckMenuItem::with_id(
                        menu_id_for(*id),
                        label.clone(),
                        *enabled,
                        *checked,
                        None,
                    );
                    host.append_item(&native)?;
                    radio.check_items.insert(*id, native);
                    radio.radio_group_of.insert(*id, group);
                    radio.radio_groups.entry(group).or_default().push(*id);
                }
                TrayMenuItem::Submenu {
                    label,
                    items,
                    enabled,
                } => {
                    let submenu = Submenu::new(label.clone(), *enabled);
                    append_items(&submenu, items, radio)?;
                    host.append_item(&submenu)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/headless/tray_menu.rs"]
mod tests;
