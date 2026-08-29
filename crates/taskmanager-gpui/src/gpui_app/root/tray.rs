//! GPUI system-tray host (ADR-032).
//!
//! Builds the neutral [`TraySpec`] (localized labels + branded RGBA icon),
//! spawns it through the toolkit-neutral `taskmanager-platform-native` seam,
//! and pumps the seam's event channel from a GPUI foreground task so tray
//! interactions land on the main thread. Tray hosting is graceful: a typed
//! spawn failure (e.g. Linux without a StatusNotifierWatcher) logs and leaves
//! the app tray-less instead of failing startup.
//!
//! macOS note: `spawn_tray` must run on the application main thread — this
//! module is only ever called from `root::startup::init` on the main thread,
//! so the requirement holds by construction.

use std::sync::mpsc::channel;

use gpui::{AnyWindowHandle, App, Entity};
use taskmanager_app_host::spawn_tray;
use taskmanager_assets::{PRODUCT_TRAY_ICON_SIZE, product, product_tray_icon_rgba};
use taskmanager_core::core::tray::{
    TrayActionId, TrayEvent, TrayIconData, TrayIconError, TrayMenuItem, TrayMenuSpec, TraySpec,
    TraySpecError,
};
use tracing::warn;

use crate::gpui_app::root::RootView;
use crate::gpui_app::root::i18n;

/// Stable tray menu action ids. Shared with the (frontend-local) mapping;
/// never collide across frontends because the id space is per-tray.
pub const TRAY_ACTION_SHOW: TrayActionId = 1;
pub const TRAY_ACTION_PAUSE: TrayActionId = 2;
pub const TRAY_ACTION_QUIT: TrayActionId = 3;

/// Frontend intent behind a tray menu activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayIntent {
    /// Bring the main window back to the foreground.
    ShowWindow,
    /// Invert the manual refresh pause (mirrors `AppAction::TogglePause`).
    TogglePause,
    /// Quit the application.
    Quit,
}

/// Pure mapping from a tray action id to the frontend intent.
#[must_use]
pub fn resolve_tray_action(id: TrayActionId) -> Option<TrayIntent> {
    match id {
        TRAY_ACTION_SHOW => Some(TrayIntent::ShowWindow),
        TRAY_ACTION_PAUSE => Some(TrayIntent::TogglePause),
        TRAY_ACTION_QUIT => Some(TrayIntent::Quit),
        _ => None,
    }
}

/// The branded 22×22 tray asset shared with the Iced frontend and regenerated
/// from the same SVG that owns Linux, macOS and Windows application icons.
pub fn tray_icon_pixels() -> Result<TrayIconData, TrayIconError> {
    TrayIconData::from_rgba(
        product_tray_icon_rgba().to_vec(),
        PRODUCT_TRAY_ICON_SIZE,
        PRODUCT_TRAY_ICON_SIZE,
    )
}

/// Build the localized tray spec. `paused` seeds the refresh checkmark.
pub fn build_tray_spec(paused: bool) -> Result<TraySpec, TraySpecError> {
    let icon = tray_icon_pixels().map_err(TraySpecError::Icon)?;
    let menu = TrayMenuSpec::from_items(vec![
        TrayMenuItem::Action {
            id: TRAY_ACTION_SHOW,
            label: i18n::t("tray.show_window").to_owned(),
            enabled: true,
        },
        TrayMenuItem::Checkmark {
            id: TRAY_ACTION_PAUSE,
            label: i18n::t("tray.pause_refresh").to_owned(),
            checked: paused,
            enabled: true,
        },
        TrayMenuItem::Separator,
        TrayMenuItem::Action {
            id: TRAY_ACTION_QUIT,
            label: i18n::t("tray.quit").to_owned(),
            enabled: true,
        },
    ])
    .map_err(TraySpecError::Menu)?;
    TraySpec::new(
        icon,
        Some(i18n::t("tray.tooltip").to_owned()),
        Some(product::GPUI_NAME.to_owned()),
        menu,
        false,
    )
}

/// Spawn the tray and run its event loop on the GPUI main thread.
///
/// Must be called on the main thread (macOS requires it). Failures are
/// logged and the app continues without a tray.
pub fn spawn_tray_host(view: &Entity<RootView>, cx: &mut App) -> bool {
    let paused = view.read(cx).telemetry_refresh_policy.is_manually_paused();
    let spec = match build_tray_spec(paused) {
        Ok(spec) => spec,
        Err(error) => {
            warn!(
                ?error,
                "cannot build system tray spec; continuing without a tray"
            );
            return false;
        }
    };
    let (events_tx, events_rx) = channel::<TrayEvent>();
    let controller = match spawn_tray(spec, events_tx) {
        Ok(controller) => controller,
        Err(failure) => {
            warn!(
                ?failure,
                "system tray unavailable; continuing without a tray"
            );
            return false;
        }
    };
    view.update(cx, |view, _cx| {
        view.tray_controller = Some(controller);
        view.tray_events_rx = Some(events_rx);
    });
    true
}

pub(crate) fn drain_tray_events(
    view: &mut RootView,
    cx: &mut gpui::Context<RootView>,
    window_handle: AnyWindowHandle,
) {
    let mut events = Vec::new();
    if let Some(rx) = &view.tray_events_rx {
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
    }
    for event in events {
        match event {
            TrayEvent::IconActivated | TrayEvent::IconDoubleClicked => {
                cx.activate(true);
                let _ = window_handle.update(cx, |_, window, _| window.activate_window());
            }
            TrayEvent::MenuActivated { id } => match resolve_tray_action(id) {
                Some(TrayIntent::ShowWindow) => {
                    cx.activate(true);
                    let _ = window_handle.update(cx, |_, window, _| window.activate_window());
                }
                Some(TrayIntent::TogglePause) => {
                    toggle_pause_from_tray(view);
                }
                Some(TrayIntent::Quit) => {
                    cx.quit();
                    break;
                }
                None => {}
            },
        }
    }
}

/// Mirror of the keyboard `TogglePause` path so the tray and Ctrl+Space stay
/// on one state; the tray checkmark is synced here and in the keyboard path.
fn toggle_pause_from_tray(view: &mut RootView) {
    let paused = view.telemetry_refresh_policy.is_manually_paused();
    let next = !paused;
    view.telemetry_refresh_policy
        .apply(taskmanager_application::TelemetryRefreshPolicyChange::SetPaused(next));
    sync_tray_pause_checkmark(view, next);
}

/// Keep the tray checkmark in sync with the manual-pause state.
pub(crate) fn sync_tray_pause_checkmark(view: &RootView, paused: bool) {
    if let Some(tray) = view.tray_controller.as_ref() {
        let _ = tray.set_item_checked(TRAY_ACTION_PAUSE, paused);
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_tray_tests.rs"]
mod tests;
