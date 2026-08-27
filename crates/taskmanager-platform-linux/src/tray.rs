//! Linux StatusNotifierItem tray adapter.
//!
//! Pure safe Rust through `ksni` (blocking API over `zbus`): no GTK, no
//! libappindicator, no additional executor thread. The tray service runs on
//! ksni's own background thread; all mutations are routed through the
//! blocking `Handle`, so the [`TrayController`] is `Send + Sync`.
//!
//! When no StatusNotifierWatcher exists (GNOME without the AppIndicator
//! extension, headless sessions), spawn fails with a typed
//! [`TrayFailure`]; the product degrades gracefully to no tray.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::mpsc::Sender;

use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Error, Icon, MenuItem, Status, ToolTip};
use taskmanager_core::tray::{
    TrayActionId, TrayEvent, TrayIconData, TrayMenuItem, TrayMenuSpec, TraySpec,
};
use taskmanager_platform_contract::{TrayController, TrayFailure};

/// Stable StatusNotifierItem id; consistent across sessions.
const TRAY_ID: &str = "TaskForest";

/// Mutable menu state that survives between menu() rebuilds.
#[derive(Clone, Debug, Default)]
pub(super) struct MenuState {
    checkmarks: HashMap<TrayActionId, bool>,
    radio_group_of: HashMap<TrayActionId, u32>,
    radio_groups: HashMap<u32, Vec<TrayActionId>>,
    checked_radios: HashMap<TrayActionId, bool>,
}

impl MenuState {
    fn from_spec(menu: &TrayMenuSpec) -> Self {
        let mut state = Self::default();
        Self::collect(menu.items(), &mut state);
        state
    }

    fn collect(items: &[TrayMenuItem], state: &mut Self) {
        for item in items {
            match item {
                TrayMenuItem::Checkmark { id, checked, .. } => {
                    state.checkmarks.insert(*id, *checked);
                }
                TrayMenuItem::Radio {
                    id,
                    checked,
                    radio_group,
                    ..
                } => {
                    let group = radio_group.unwrap_or(*id);
                    state.radio_group_of.insert(*id, group);
                    state.radio_groups.entry(group).or_default().push(*id);
                    state.checked_radios.insert(*id, *checked);
                }
                TrayMenuItem::Submenu { items, .. } => Self::collect(items, state),
                TrayMenuItem::Action { .. } | TrayMenuItem::Separator => {}
            }
        }
    }

    fn checkmark(&self, id: TrayActionId, initial: bool) -> bool {
        self.checkmarks.get(&id).copied().unwrap_or(initial)
    }

    fn radio_checked(&self, id: TrayActionId, initial: bool) -> bool {
        self.checked_radios.get(&id).copied().unwrap_or(initial)
    }

    fn set_checked(&mut self, id: TrayActionId, checked: bool) {
        if let Some(&group) = self.radio_group_of.get(&id) {
            if checked {
                if let Some(ids) = self.radio_groups.get(&group) {
                    for other in ids {
                        if *other != id {
                            self.checked_radios.insert(*other, false);
                        }
                    }
                }
                self.checked_radios.insert(id, true);
            } else {
                self.checked_radios.insert(id, false);
            }
        } else if self.checkmarks.contains_key(&id) {
            self.checkmarks.insert(id, checked);
        }
    }

    fn radio_group_ids(&self, group: u32) -> Option<&[TrayActionId]> {
        self.radio_groups.get(&group).map(Vec::as_slice)
    }
}

/// The ksni tray service; `menu()` re-derives the menu from spec + state.
pub(super) struct KsniTray {
    spec: TraySpec,
    tooltip: Option<String>,
    title: Option<String>,
    state: MenuState,
    events: Sender<TrayEvent>,
}

impl KsniTray {
    fn send(&self, event: TrayEvent) {
        let _ = self.events.send(event);
    }
}

impl ksni::Tray for KsniTray {
    fn id(&self) -> String {
        TRAY_ID.into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![to_ksni_icon(self.spec.icon())]
    }

    fn title(&self) -> String {
        self.title.clone().unwrap_or_default()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            icon_pixmap: vec![to_ksni_icon(self.spec.icon())],
            title: self.title.clone().unwrap_or_default(),
            description: self.tooltip.clone().unwrap_or_default(),
            ..Default::default()
        }
    }

    fn status(&self) -> Status {
        Status::Active
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayEvent::IconActivated);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        map_items(self.spec.menu().items(), &self.state, &self.events)
    }
}

/// Map the neutral menu tree to ksni menu items (pure; used by tests).
pub(super) fn map_items(
    items: &[TrayMenuItem],
    state: &MenuState,
    events: &Sender<TrayEvent>,
) -> Vec<MenuItem<KsniTray>> {
    let mut mapped = Vec::new();
    let mut pending_radio_group: Option<(u32, Vec<RadioItem>, Vec<TrayActionId>)> = None;

    for item in items {
        match item {
            TrayMenuItem::Separator => {
                flush_radio_group(&mut pending_radio_group, state, events, &mut mapped);
                mapped.push(MenuItem::Separator);
            }
            TrayMenuItem::Action { id, label, enabled } => {
                flush_radio_group(&mut pending_radio_group, state, events, &mut mapped);
                let events = events.clone();
                let id = *id;
                mapped.push(
                    StandardItem {
                        label: label.clone(),
                        enabled: *enabled,
                        activate: Box::new(move |_tray| {
                            let _ = events.send(TrayEvent::MenuActivated { id });
                        }),
                        ..Default::default()
                    }
                    .into(),
                );
            }
            TrayMenuItem::Checkmark {
                id,
                label,
                checked,
                enabled,
            } => {
                flush_radio_group(&mut pending_radio_group, state, events, &mut mapped);
                let events = events.clone();
                let id = *id;
                let initial = *checked;
                mapped.push(
                    CheckmarkItem {
                        label: label.clone(),
                        checked: state.checkmark(id, initial),
                        enabled: *enabled,
                        activate: Box::new(move |tray: &mut KsniTray| {
                            let was = tray.state.checkmark(id, initial);
                            tray.state.set_checked(id, !was);
                            let _ = events.send(TrayEvent::MenuActivated { id });
                        }),
                        ..Default::default()
                    }
                    .into(),
                );
            }
            TrayMenuItem::Radio {
                id,
                label,
                enabled,
                radio_group,
                ..
            } => {
                let group = radio_group.unwrap_or(*id);
                match &mut pending_radio_group {
                    Some((current, options, ids)) if *current == group => {
                        options.push(RadioItem {
                            label: label.clone(),
                            enabled: *enabled,
                            ..Default::default()
                        });
                        ids.push(*id);
                    }
                    _ => {
                        flush_radio_group(&mut pending_radio_group, state, events, &mut mapped);
                        pending_radio_group = Some((
                            group,
                            vec![RadioItem {
                                label: label.clone(),
                                enabled: *enabled,
                                ..Default::default()
                            }],
                            vec![*id],
                        ));
                    }
                }
            }
            TrayMenuItem::Submenu {
                label,
                items,
                enabled,
            } => {
                flush_radio_group(&mut pending_radio_group, state, events, &mut mapped);
                mapped.push(
                    SubMenu {
                        label: label.clone(),
                        enabled: *enabled,
                        submenu: map_items(items, state, events),
                        ..Default::default()
                    }
                    .into(),
                );
            }
        }
    }
    flush_radio_group(&mut pending_radio_group, state, events, &mut mapped);
    mapped
}

fn flush_radio_group(
    pending: &mut Option<(u32, Vec<RadioItem>, Vec<TrayActionId>)>,
    state: &MenuState,
    events: &Sender<TrayEvent>,
    mapped: &mut Vec<MenuItem<KsniTray>>,
) {
    let Some((group, options, ids)) = pending.take() else {
        return;
    };
    let selected = state
        .radio_group_ids(group)
        .and_then(|group_ids| {
            group_ids
                .iter()
                .position(|group_id| state.radio_checked(*group_id, false))
        })
        .unwrap_or(0);
    let events = events.clone();
    let ids = ids.clone();
    mapped.push(
        RadioGroup {
            selected,
            select: Box::new(move |tray: &mut KsniTray, index: usize| {
                if let Some(id) = ids.get(index) {
                    tray.state.set_checked(*id, true);
                    let _ = events.send(TrayEvent::MenuActivated { id: *id });
                }
            }),
            options,
        }
        .into(),
    );
}

/// RGBA (non-premultiplied) to ksni ARGB32 network byte order.
fn to_ksni_icon(icon: &TrayIconData) -> Icon {
    let mut data = icon.pixels().to_vec();
    for pixel in data.as_chunks_mut::<4>().0 {
        pixel.rotate_right(1);
    }
    Icon {
        width: icon.width() as i32,
        height: icon.height() as i32,
        data,
    }
}

/// The [`TrayController`] for Linux; every mutation routes through the ksni
/// blocking handle to the service thread.
pub struct LinuxTrayController {
    handle: ksni::blocking::Handle<KsniTray>,
}

impl TrayController for LinuxTrayController {
    fn set_visible(&self, _visible: bool) -> Result<(), TrayFailure> {
        Err(TrayFailure::Unsupported)
    }

    fn set_tooltip(&self, tooltip: Option<String>) -> Result<(), TrayFailure> {
        self.handle
            .update(|tray| tray.tooltip = tooltip)
            .map(|_| ())
            .ok_or(TrayFailure::Rejected)
    }

    fn set_title(&self, title: Option<String>) -> Result<(), TrayFailure> {
        self.handle
            .update(|tray| tray.title = title)
            .map(|_| ())
            .ok_or(TrayFailure::Rejected)
    }

    fn set_item_checked(&self, id: TrayActionId, checked: bool) -> Result<(), TrayFailure> {
        self.handle
            .update(|tray| tray.state.set_checked(id, checked))
            .map(|_| ())
            .ok_or(TrayFailure::Rejected)
    }
}

/// Spawn the Linux tray. Blocks briefly on D-Bus setup (fast-fail when no
/// session bus or no StatusNotifierWatcher exists).
pub fn spawn_tray(
    spec: TraySpec,
    events: Sender<TrayEvent>,
) -> Result<Box<dyn TrayController>, TrayFailure> {
    use ksni::blocking::TrayMethods;

    let tray = KsniTray {
        tooltip: spec.tooltip().map(str::to_owned),
        title: spec.title().map(str::to_owned),
        state: MenuState::from_spec(spec.menu()),
        events,
        spec,
    };
    let handle = tray.spawn().map_err(classify_spawn_error)?;
    Ok(Box::new(LinuxTrayController { handle }))
}

fn classify_spawn_error(error: Error) -> TrayFailure {
    match error {
        Error::Dbus(_) | Error::Watcher(_) => TrayFailure::MissingDependency,
        Error::WontShow => TrayFailure::TemporarilyUnavailable,
        _ => TrayFailure::Rejected,
    }
}

#[cfg(test)]
#[path = "../tests/headless/linux_tray_tests.rs"]
mod tests;
