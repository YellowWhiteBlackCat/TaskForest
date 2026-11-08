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

use std::fmt;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, channel};

use bevy::ecs::resource::Resource;
use taskforest_contract::{TrayController, TrayControllerTarget, sync_tray_pause_checkmark};
use taskforest_core::core::tray::{
    TRAY_ACTION_PAUSE, TRAY_ACTION_QUIT, TRAY_ACTION_SHOW, TrayEvent, TrayMenuItem, TrayMenuSpec,
    TraySpec, TraySpecError,
};
use taskmanager_app_host::spawn_tray;
use taskmanager_application::i18n::t;
use taskmanager_assets::{product, product_tray_icon_data};

/// Build the localized Bevy tray spec. `paused` seeds the refresh checkmark.
pub fn build_tray_spec(paused: bool) -> Result<TraySpec, TraySpecError> {
    let icon = product_tray_icon_data().map_err(TraySpecError::Icon)?;
    let menu = TrayMenuSpec::from_items(vec![
        TrayMenuItem::Action {
            id: TRAY_ACTION_SHOW,
            label: t("tray.show_window").to_owned(),
            enabled: true,
        },
        TrayMenuItem::Checkmark {
            id: TRAY_ACTION_PAUSE,
            label: t("tray.pause_refresh").to_owned(),
            checked: paused,
            enabled: true,
        },
        TrayMenuItem::Separator,
        TrayMenuItem::Action {
            id: TRAY_ACTION_QUIT,
            label: t("tray.quit").to_owned(),
            enabled: true,
        },
    ])
    .map_err(TraySpecError::Menu)?;
    TraySpec::new(
        icon,
        Some(t("tray.tooltip").to_owned()),
        Some(product::BEVY_NAME.to_owned()),
        menu,
        false,
    )
}

impl TrayControllerTarget for TrayResource {
    fn controller(&self) -> Option<&dyn TrayController> {
        self.controller.as_deref()
    }
}

/// Bevy system tray resource wrapper.
///
/// Owns the active native tray controller handle and the receiver for
/// incoming user interactions (menu clicks, icon activations).
#[derive(Default, Resource)]
pub struct TrayResource {
    pub controller: Option<Box<dyn TrayController>>,
    pub events_rx: Option<Mutex<Receiver<TrayEvent>>>,
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
            events_rx: events_rx.map(Mutex::new),
        }
    }

    pub fn empty() -> Self {
        Self::default()
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
    match spawn_tray(spec, events_tx) {
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
    if let Some(rx) = tray.events_rx.as_ref()
        && let Ok(rx) = rx.lock()
    {
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
    }
    events
}

#[cfg(test)]
#[path = "../tests/headless/tray.rs"]
mod tests;
