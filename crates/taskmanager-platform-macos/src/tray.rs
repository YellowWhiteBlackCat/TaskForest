//! macOS tray adapter: `NSStatusItem` hosted on the application main thread.
//!
//! `tray-icon` requires the tray icon and its muda menu to be created and
//! mutated on the application's main thread. This adapter therefore keeps the
//! native objects in a main-thread slot ([`thread_local`]) and returns a
//! lightweight [`TrayController`] that refuses mutations from any other
//! thread with a typed [`TrayFailure::WrongThread`]. The frontend must call
//! [`spawn_tray`] from its main thread after the event loop is running.
//!
//! A tiny background thread drains the global `tray-icon` event channels and
//! forwards interactions to the seam's `events` sender, so the frontend only
//! ever polls its own channel. The native menu is rendered to muda by the
//! shared `taskmanager-tray-muda` bridge.
//!
//! When the controller is dropped on the creating (main) thread, the native
//! objects are removed immediately; when dropped elsewhere, cleanup is
//! deferred to process exit (documented, one-time, harmless).

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
#[cfg(target_os = "macos")]
use std::thread::ThreadId;
#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
use taskmanager_core::tray::TrayActionId;
use taskmanager_core::tray::{TrayEvent, TraySpec};
use taskmanager_platform_contract::{TrayController, TrayFailure};

// Only the macOS forwarding loop polls; the constant (and its Duration
// import) is gated so non-macOS hosts see no dead code.
#[cfg(target_os = "macos")]
const FORWARD_POLL_INTERVAL: Duration = Duration::from_millis(20);

// The native tray lives in a thread-local slot on the thread that created it
// (the application main thread on macOS); access from any other thread is
// refused. `thread_local!` expansions do not preserve `///` doc comments.
#[cfg(target_os = "macos")]
thread_local! {
    static MAC_TRAY: std::cell::RefCell<Option<NativeTray>> =
        const { std::cell::RefCell::new(None) };
}

/// Spawn the macOS tray. **Must be called on the application main thread**
/// after the event loop is running; the returned controller must also be
/// used on that thread.
pub fn spawn_tray(
    spec: TraySpec,
    events: Sender<TrayEvent>,
) -> Result<Box<dyn TrayController>, TrayFailure> {
    #[cfg(target_os = "macos")]
    {
        spawn_tray_macos(spec, events)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (spec, events);
        Err(TrayFailure::Unsupported)
    }
}

#[cfg(target_os = "macos")]
fn spawn_tray_macos(
    spec: TraySpec,
    events: Sender<TrayEvent>,
) -> Result<Box<dyn TrayController>, TrayFailure> {
    let native = NativeTray::new(&spec)?;
    MAC_TRAY.with(|slot| {
        *slot.borrow_mut() = Some(native);
    });
    let stop = Arc::new(AtomicBool::new(false));
    spawn_event_forwarder(events, stop.clone());
    Ok(Box::new(MacTrayController {
        thread: std::thread::current().id(),
        stop,
    }))
}

/// Drains the global tray-icon event channels on a background thread and
/// forwards interactions to the seam's channel.
#[cfg(target_os = "macos")]
fn spawn_event_forwarder(events: Sender<TrayEvent>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            forward_tray_events(&events);
            std::thread::sleep(FORWARD_POLL_INTERVAL);
        }
    });
}

#[cfg(target_os = "macos")]
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

/// The native tray object, held in the main-thread slot.
#[cfg(target_os = "macos")]
struct NativeTray {
    icon: tray_icon::TrayIcon,
    radio: taskmanager_tray_muda::RadioState,
}

#[cfg(target_os = "macos")]
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

    fn set_title(&self, title: Option<String>) -> Result<(), TrayFailure> {
        self.icon.set_title(title);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn classify_build_error(error: tray_icon::Error) -> TrayFailure {
    match error {
        tray_icon::Error::OsError(_) => TrayFailure::TemporarilyUnavailable,
        _ => TrayFailure::Rejected,
    }
}

/// The [`TrayController`] for macOS. Only usable on the thread that spawned
/// the tray; the native objects live in that thread's `thread_local` slot.
pub struct MacTrayController {
    /// Creating-thread guard; only the macOS implementation reads it (the
    /// native objects live in this thread's `thread_local` slot).
    #[cfg(target_os = "macos")]
    thread: ThreadId,
    stop: Arc<AtomicBool>,
}

#[cfg(target_os = "macos")]
impl MacTrayController {
    /// Access the native tray on the creating thread; a `WrongThread` failure
    /// is typed and never touches the native objects.
    fn with_native<T>(&self, action: impl FnOnce(&NativeTray) -> T) -> Result<T, TrayFailure> {
        if std::thread::current().id() != self.thread {
            return Err(TrayFailure::WrongThread);
        }
        MAC_TRAY
            .try_with(|slot| {
                let guard = slot.borrow();
                let native = guard.as_ref().ok_or(TrayFailure::Rejected)?;
                Ok(action(native))
            })
            .map_err(|_| TrayFailure::Rejected)?
    }
}

#[cfg(target_os = "macos")]
impl TrayController for MacTrayController {
    fn set_visible(&self, visible: bool) -> Result<(), TrayFailure> {
        self.with_native(|native| native.set_visible(visible))
            .and_then(std::convert::identity)
    }

    fn set_tooltip(&self, tooltip: Option<String>) -> Result<(), TrayFailure> {
        self.with_native(|native| native.set_tooltip(tooltip))
            .and_then(std::convert::identity)
    }

    fn set_title(&self, title: Option<String>) -> Result<(), TrayFailure> {
        self.with_native(|native| native.set_title(title))
            .and_then(std::convert::identity)
    }

    fn set_item_checked(&self, id: TrayActionId, checked: bool) -> Result<(), TrayFailure> {
        self.with_native(|native| native.radio.set_checked(id, checked))
            .map(|_| ())
    }
}

impl Drop for MacTrayController {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        #[cfg(target_os = "macos")]
        if std::thread::current().id() == self.thread {
            // Clean removal when dropped on the main thread.
            let _ = MAC_TRAY.try_with(|slot| {
                *slot.borrow_mut() = None;
            });
        }
        // Dropped elsewhere: the native slot is inaccessible from this
        // thread, so teardown is deferred to process exit (documented).
    }
}

#[cfg(test)]
#[path = "../tests/headless/macos_tray.rs"]
mod tests;
