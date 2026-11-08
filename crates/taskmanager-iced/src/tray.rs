//! Iced's process-lifetime system-tray host (ADR-032).
//!
//! This module is deliberately parallel to the GPUI tray adapter while using
//! the Iced product identity. The native object and singleton stay in the
//! shared app-host/platform seam; Iced only builds the neutral spec, owns the
//! typed channels, and reduces tray actions into existing shell state.

use std::sync::mpsc::channel;

use taskforest_contract::{TrayController, TrayControllerTarget, sync_tray_pause_checkmark};
use taskforest_core::core::tray::{
    TRAY_ACTION_PAUSE, TRAY_ACTION_QUIT, TRAY_ACTION_SHOW, TrayEvent, TrayIntent, TrayMenuItem,
    TrayMenuSpec, TraySpec, TraySpecError, resolve_tray_action,
};
use taskmanager_application::AppAction;
use taskmanager_assets::{product, product_tray_icon_data};
use taskmanager_shell::QuitReason;

use crate::IcedApp;
use taskmanager_app_host::spawn_tray;
use taskmanager_application::i18n::t;

/// Build the localized Iced tray spec. The pause checkmark is seeded from the
/// same shell policy that the keyboard path mutates.
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
        Some(product::ICED_NAME.to_owned()),
        menu,
        false,
    )
}

/// Spawn the Iced tray on the application event-loop thread. A typed native
/// failure degrades to a normal window-only app; it never fabricates tray
/// availability or prevents the frontend from starting.
pub(crate) fn spawn_tray_host(app: &mut IcedApp) -> bool {
    let spec = match build_tray_spec(app.shell.paused()) {
        Ok(spec) => spec,
        Err(error) => {
            eprintln!("taskforest-i: cannot build system tray spec: {error:?}");
            return false;
        }
    };
    let (events_tx, events_rx) = channel::<TrayEvent>();
    let controller = match spawn_tray(spec, events_tx) {
        Ok(controller) => controller,
        Err(failure) => {
            eprintln!("taskforest-i: system tray unavailable: {failure:?}");
            return false;
        }
    };
    app.runtime.install_tray(Some(controller), Some(events_rx));
    true
}

/// Drain tray messages without blocking. Returns whether the window should be
/// restored/foregrounded; quit and pause are reduced immediately through the
/// existing shell state machine.
pub(crate) fn drain_tray_events(app: &mut IcedApp) -> bool {
    let mut activate = false;
    for event in app.runtime.drain_tray_events() {
        match event {
            TrayEvent::IconActivated | TrayEvent::IconDoubleClicked => activate = true,
            TrayEvent::MenuActivated { id } => match resolve_tray_action(id) {
                Some(TrayIntent::ShowWindow) => activate = true,
                Some(TrayIntent::TogglePause) => {
                    let _ = app.shell.apply_action(AppAction::TogglePause);
                    sync_tray_pause_checkmark(app, app.shell.paused());
                }
                Some(TrayIntent::Quit) => {
                    app.shell.request_quit(QuitReason::Tray);
                }
                None => {}
            },
        }
    }
    activate
}

impl TrayControllerTarget for IcedApp {
    fn controller(&self) -> Option<&dyn TrayController> {
        self.runtime.controller()
    }
}

#[cfg(test)]
#[path = "../tests/gui/tray_tests.rs"]
mod tests;
