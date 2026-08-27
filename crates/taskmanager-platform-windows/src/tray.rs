//! Windows tray adapter: dedicated host thread + `Shell_NotifyIcon` via
//! `tray-icon`/`muda`.
//!
//! The tray lives on its own background thread with its own bounded Win32
//! message pump (the pump is provided by the audited
//! `taskmanager-windows-api` boundary). This keeps every mutation thread-safe
//! — the [`TrayController`] is `Send + Sync` — and decouples the tray from
//! any particular frontend event loop. The neutral menu is rendered to muda
//! by the shared `taskmanager-tray-muda` bridge.
//!
//! Only the message pump touches raw Win32; `tray-icon`/`muda` own all other
//! native surface behind their safe public APIs. This crate remains
//! `#![forbid(unsafe_code)]`.

#![forbid(unsafe_code)]

#[cfg(windows)]
use std::sync::Mutex;
use std::sync::mpsc::Sender;
#[cfg(windows)]
use std::sync::mpsc::{self, RecvTimeoutError};
#[cfg(windows)]
use std::thread::{self, JoinHandle};
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
use taskmanager_core::tray::TrayActionId;
use taskmanager_core::tray::{TrayEvent, TraySpec};
use taskmanager_platform_contract::{TrayController, TrayFailure};

/// How long the host thread waits for the next command before pumping window
/// messages again. 20 ms bounds command latency without busy-spinning.
// Only the Windows host loop polls; the constant (and its Duration import)
// are gated so non-Windows hosts see no dead code (mirrors the macOS tray).
#[cfg(windows)]
const HOST_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Commands routed from the (possibly foreign-thread) controller to the host
/// thread that owns the native tray.
#[cfg(windows)]
enum Command {
    SetVisible(bool),
    SetTooltip(Option<String>),
    SetItemChecked { id: TrayActionId, checked: bool },
    Shutdown,
}

/// Spawn the Windows tray on a dedicated host thread.
pub fn spawn_tray(
    spec: TraySpec,
    events: Sender<TrayEvent>,
) -> Result<Box<dyn TrayController>, TrayFailure> {
    #[cfg(windows)]
    {
        spawn_tray_windows(spec, events)
    }
    #[cfg(not(windows))]
    {
        let _ = (spec, events);
        Err(TrayFailure::Unsupported)
    }
}

#[cfg(windows)]
fn spawn_tray_windows(
    spec: TraySpec,
    events: Sender<TrayEvent>,
) -> Result<Box<dyn TrayController>, TrayFailure> {
    let (command_tx, command_rx) = mpsc::channel::<Command>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), TrayFailure>>();
    let handle = thread::spawn(move || host_main(spec, events, command_rx, ready_tx));
    match ready_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(failure)) => return Err(failure),
        Err(_) => return Err(TrayFailure::Rejected),
    }
    Ok(Box::new(WindowsTrayController {
        commands: command_tx,
        join: Mutex::new(Some(handle)),
    }))
}

#[cfg(windows)]
fn host_main(
    spec: TraySpec,
    events: Sender<TrayEvent>,
    command_rx: mpsc::Receiver<Command>,
    ready_tx: Sender<Result<(), TrayFailure>>,
) {
    let native = match NativeTray::new(&spec) {
        Ok(native) => native,
        Err(failure) => {
            let _ = ready_tx.send(Err(failure));
            return;
        }
    };
    let _ = ready_tx.send(Ok(()));
    loop {
        // Deliver Shell_NotifyIcon callbacks to the hidden tray window.
        let _ = taskmanager_windows_api::pump_pending_messages();
        forward_tray_events(&events);
        match command_rx.recv_timeout(HOST_POLL_INTERVAL) {
            Ok(Command::SetVisible(visible)) => {
                let _ = native.set_visible(visible);
            }
            Ok(Command::SetTooltip(tooltip)) => {
                let _ = native.set_tooltip(tooltip);
            }
            Ok(Command::SetItemChecked { id, checked }) => {
                native.radio.set_checked(id, checked);
            }
            Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
    // `native` is dropped here, removing the icon (NIM_DELETE) and joining
    // the hidden window.
}

#[cfg(windows)]
fn forward_tray_events(events: &Sender<TrayEvent>) {
    use tray_icon::{MouseButton, TrayIconEvent};

    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } => {
                let _ = events.send(TrayEvent::IconActivated);
            }
            TrayIconEvent::DoubleClick { .. } => {
                let _ = events.send(TrayEvent::IconDoubleClicked);
            }
            _ => {}
        }
    }
    while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
        if let Some(id) = taskmanager_tray_muda::decode_menu_id(event.id.as_ref()) {
            let _ = events.send(TrayEvent::MenuActivated { id });
        }
    }
}

/// The native tray object, owned by the host thread.
#[cfg(windows)]
struct NativeTray {
    icon: tray_icon::TrayIcon,
    radio: taskmanager_tray_muda::RadioState,
}

#[cfg(windows)]
impl NativeTray {
    fn new(spec: &TraySpec) -> Result<Self, TrayFailure> {
        let icon = tray_icon::Icon::from_rgba(
            spec.icon().pixels().to_vec(),
            spec.icon().width(),
            spec.icon().height(),
        )
        .map_err(|_| TrayFailure::Rejected)?;

        let built = taskmanager_tray_muda::build_menu(spec.menu())?;

        let mut builder = tray_icon::TrayIconBuilder::new()
            .with_icon(icon)
            .with_menu(Box::new(built.menu))
            .with_menu_on_left_click(spec.show_menu_on_left_click())
            .with_menu_on_right_click(true);
        if let Some(tooltip) = spec.tooltip() {
            builder = builder.with_tooltip(tooltip);
        }
        let icon = builder.build().map_err(classify_build_error)?;

        Ok(Self {
            icon,
            radio: built.radio,
        })
    }

    fn set_visible(&self, visible: bool) -> Result<(), TrayFailure> {
        self.icon
            .set_visible(visible)
            .map_err(|_| TrayFailure::TemporarilyUnavailable)
    }

    fn set_tooltip(&self, tooltip: Option<String>) -> Result<(), TrayFailure> {
        self.icon
            .set_tooltip(tooltip)
            .map_err(|_| TrayFailure::TemporarilyUnavailable)
    }
}

#[cfg(windows)]
fn classify_build_error(error: tray_icon::Error) -> TrayFailure {
    match error {
        tray_icon::Error::OsError(_) => TrayFailure::TemporarilyUnavailable,
        _ => TrayFailure::Rejected,
    }
}

/// The [`TrayController`] facade; commands are forwarded to the host thread.
/// Windows-only: the host thread (and its command channel) exist only where
/// the native tray can be spawned; other hosts get a typed `Unsupported`
/// from [`spawn_tray`] and never see this type.
#[cfg(windows)]
pub struct WindowsTrayController {
    commands: mpsc::Sender<Command>,
    join: Mutex<Option<JoinHandle<()>>>,
}

#[cfg(windows)]
impl TrayController for WindowsTrayController {
    fn set_visible(&self, visible: bool) -> Result<(), TrayFailure> {
        self.commands
            .send(Command::SetVisible(visible))
            .map_err(|_| TrayFailure::Rejected)
    }

    fn set_tooltip(&self, tooltip: Option<String>) -> Result<(), TrayFailure> {
        self.commands
            .send(Command::SetTooltip(tooltip))
            .map_err(|_| TrayFailure::Rejected)
    }

    fn set_title(&self, _title: Option<String>) -> Result<(), TrayFailure> {
        Err(TrayFailure::Unsupported)
    }

    fn set_item_checked(&self, id: TrayActionId, checked: bool) -> Result<(), TrayFailure> {
        self.commands
            .send(Command::SetItemChecked { id, checked })
            .map_err(|_| TrayFailure::Rejected)
    }
}

#[cfg(windows)]
impl Drop for WindowsTrayController {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(handle) = self.join.lock().ok().and_then(|mut guard| guard.take()) {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/platform_windows_tray.rs"]
mod tests;
