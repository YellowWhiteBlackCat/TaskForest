//! Bevy UI system-tray host seam (ADR-032).
//!
//! This module provides parity with the GPUI and Iced tray hosts
//! using Bevy UI product identity (`TaskForestB`). The native platform
//! adapter and singleton stay behind `taskmanager-app-host`/`taskmanager-platform-native`;
//! this seam builds the neutral spec, owns typed channels, and resolves
//! tray interactions into typed intents.
//!
//! Linux tray communication runs over DBus StatusNotifierItem (Wayland-native,
//! zero X11 dependencies).

#![allow(dead_code)]

use std::fmt;
use std::sync::mpsc::{Receiver, channel};

use taskmanager_assets::{PRODUCT_TRAY_ICON_SIZE, product, product_tray_icon_rgba};
use taskmanager_core::core::tray::{
    TrayActionId, TrayEvent, TrayIconData, TrayIconError, TrayMenuItem, TrayMenuSpec, TraySpec,
    TraySpecError,
};
use taskmanager_platform_contract::TrayController;

/// Stable tray action identifiers shared with other frontends.
pub const TRAY_ACTION_SHOW: TrayActionId = 1;
pub const TRAY_ACTION_PAUSE: TrayActionId = 2;
pub const TRAY_ACTION_QUIT: TrayActionId = 3;

/// Frontend intent behind a tray activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayIntent {
    /// Restore and foreground the main Bevy window.
    Show,
    /// Toggle the telemetry refresh pause state.
    TogglePause,
    /// Close the process and release resources.
    Quit,
}

impl TrayIntent {
    /// Alias matching the GPUI/Iced intent naming convention.
    #[allow(non_upper_case_globals)]
    pub const ShowWindow: Self = Self::Show;
    pub const SHOW_WINDOW: Self = Self::Show;
}

/// Pure mapping from a native action id to a Bevy UI tray intent.
#[must_use]
pub fn resolve_tray_action(id: TrayActionId) -> Option<TrayIntent> {
    match id {
        TRAY_ACTION_SHOW => Some(TrayIntent::Show),
        TRAY_ACTION_PAUSE => Some(TrayIntent::TogglePause),
        TRAY_ACTION_QUIT => Some(TrayIntent::Quit),
        _ => None,
    }
}

/// Deterministic 22x22 RGBA tray icon. The native adapters all consume the
/// same validated bytes, so frontends do not drift visually.
pub fn tray_icon_pixels() -> Result<TrayIconData, TrayIconError> {
    TrayIconData::from_rgba(
        product_tray_icon_rgba().to_vec(),
        PRODUCT_TRAY_ICON_SIZE,
        PRODUCT_TRAY_ICON_SIZE,
    )
}

/// Build the localized Bevy tray spec. `paused` seeds the refresh checkmark.
pub fn build_tray_spec(paused: bool) -> Result<TraySpec, TraySpecError> {
    let icon = tray_icon_pixels().map_err(TraySpecError::Icon)?;
    let menu = TrayMenuSpec::from_items(vec![
        TrayMenuItem::Action {
            id: TRAY_ACTION_SHOW,
            label: taskmanager_application::i18n::t("tray.show_window").to_owned(),
            enabled: true,
        },
        TrayMenuItem::Checkmark {
            id: TRAY_ACTION_PAUSE,
            label: taskmanager_application::i18n::t("tray.pause_refresh").to_owned(),
            checked: paused,
            enabled: true,
        },
        TrayMenuItem::Separator,
        TrayMenuItem::Action {
            id: TRAY_ACTION_QUIT,
            label: taskmanager_application::i18n::t("tray.quit").to_owned(),
            enabled: true,
        },
    ])
    .map_err(TraySpecError::Menu)?;
    TraySpec::new(
        icon,
        Some(taskmanager_application::i18n::t("tray.tooltip").to_owned()),
        Some(product::BEVY_NAME.to_owned()),
        menu,
        false,
    )
}

/// Target that can provide an optional reference to a [`TrayController`].
pub trait TrayControllerTarget {
    fn controller(&self) -> Option<&dyn TrayController>;
}

impl TrayControllerTarget for TrayResource {
    fn controller(&self) -> Option<&dyn TrayController> {
        self.controller.as_deref()
    }
}

impl TrayControllerTarget for Option<Box<dyn TrayController>> {
    fn controller(&self) -> Option<&dyn TrayController> {
        self.as_deref()
    }
}

impl TrayControllerTarget for Option<&dyn TrayController> {
    fn controller(&self) -> Option<&dyn TrayController> {
        *self
    }
}

impl TrayControllerTarget for Box<dyn TrayController> {
    fn controller(&self) -> Option<&dyn TrayController> {
        Some(&**self)
    }
}

impl TrayControllerTarget for dyn TrayController {
    fn controller(&self) -> Option<&dyn TrayController> {
        Some(self)
    }
}

/// Update the pause checkmark state on the tray controller.
pub fn sync_tray_pause_checkmark<T: TrayControllerTarget + ?Sized>(target: &T, paused: bool) {
    if let Some(controller) = target.controller() {
        let _ = controller.set_item_checked(TRAY_ACTION_PAUSE, paused);
    }
}

/// Bevy system tray resource wrapper.
///
/// Owns the active native tray controller handle and the receiver for
/// incoming user interactions (menu clicks, icon activations).
#[derive(Default)]
pub struct TrayResource {
    pub controller: Option<Box<dyn TrayController>>,
    pub events_rx: Option<Receiver<TrayEvent>>,
}

impl fmt::Debug for TrayResource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrayResource")
            .field("has_controller", &self.controller.is_some())
            .field("has_events_rx", &self.events_rx.is_some())
            .finish()
    }
}

impl TrayResource {
    pub fn new(
        controller: Option<Box<dyn TrayController>>,
        events_rx: Option<Receiver<TrayEvent>>,
    ) -> Self {
        Self {
            controller,
            events_rx,
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.controller.is_some()
    }

    pub fn sync_pause_checkmark(&self, paused: bool) {
        sync_tray_pause_checkmark(self, paused);
    }

    pub fn drain_events(&mut self) -> Vec<TrayEvent> {
        drain_tray_events(self)
    }
}

/// Spawn the Bevy system tray host. Gracefully degrades to `(None, None)`
/// if the tray service is unavailable on the host.
pub fn spawn_tray_host(
    paused: bool,
) -> (Option<Box<dyn TrayController>>, Option<Receiver<TrayEvent>>) {
    let spec = match build_tray_spec(paused) {
        Ok(spec) => spec,
        Err(error) => {
            eprintln!("taskforest-b: cannot build system tray spec: {error:?}");
            return (None, None);
        }
    };
    let (events_tx, events_rx) = channel::<TrayEvent>();
    match taskmanager_app_host::spawn_tray(spec, events_tx) {
        Ok(controller) => (Some(controller), Some(events_rx)),
        Err(failure) => {
            eprintln!("taskforest-b: system tray unavailable: {failure:?}");
            (None, None)
        }
    }
}

/// Drain pending tray events from the receiver without blocking.
pub fn drain_tray_events(tray: &mut TrayResource) -> Vec<TrayEvent> {
    let mut events = Vec::new();
    if let Some(rx) = tray.events_rx.as_ref() {
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
    }
    events
}

#[cfg(test)]
#[path = "../tests/headless/tray.rs"]
mod tests;
