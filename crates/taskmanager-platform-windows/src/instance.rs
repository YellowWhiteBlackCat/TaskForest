//! Windows single-instance adapter (borrowed core from
//! `tauri-plugin-single-instance`).
//!
//! Exclusivity is a named mutex (created atomically by the OS; a second
//! `CreateMutexW` reports already-exists). The "activate the existing
//! instance" handoff is a named auto-reset event: a secondary signals it and
//! exits; the primary waits on it from a helper thread and forwards
//! [`InstanceEvent::Activate`] to the frontend. All raw Win32 surface lives
//! in the audited `taskmanager-windows-api` boundary; this crate stays
//! `#![forbid(unsafe_code)]`.

#![forbid(unsafe_code)]

// The Windows implementation is named-mutex/named-event based; the imports
// it needs are gated so the module (and its typed `Unsupported` fallback)
// still compiles — and its contract test still runs — on every other host.
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
#[cfg(windows)]
use std::sync::{Arc, Mutex};
#[cfg(windows)]
use std::thread::JoinHandle;

#[cfg(windows)]
use taskmanager_platform_contract::InstanceGuard;
use taskmanager_platform_contract::{InstanceEvent, InstanceFailure, InstanceRole};
#[cfg(windows)]
use taskmanager_windows_api::{InstanceMutex, signal_named_event};

/// The kernel-object name for a single-instance mutex/event derives from the
/// stable instance name (e.g. the app identifier). Mutex and event must use
/// distinct names — two kernel objects cannot share one name.
#[cfg(windows)]
fn native_mutex_name(instance_name: &str) -> String {
    format!("single-instance.{instance_name}.mutex")
}

#[cfg(windows)]
fn native_event_name(instance_name: &str) -> String {
    format!("single-instance.{instance_name}.event")
}

/// Acquire the single-instance ownership.
///
/// `Primary(guard)`: this process owns the instance; hold `guard` for the
/// process lifetime. `Secondary`: another instance exists; the adapter has
/// already asked it to show its window (best-effort), so the caller should
/// exit.
pub fn acquire_single_instance(
    instance_name: &str,
    events: Sender<InstanceEvent>,
) -> Result<InstanceRole, InstanceFailure> {
    #[cfg(windows)]
    {
        acquire_windows(instance_name, events)
    }
    #[cfg(not(windows))]
    {
        let _ = (instance_name, events);
        Err(InstanceFailure::Unsupported)
    }
}

#[cfg(windows)]
fn acquire_windows(
    instance_name: &str,
    events: Sender<InstanceEvent>,
) -> Result<InstanceRole, InstanceFailure> {
    let mutex_name = native_mutex_name(instance_name);
    let event_name = native_event_name(instance_name);
    let (mutex, already_exists) =
        InstanceMutex::create(&mutex_name).map_err(|_| InstanceFailure::Rejected)?;
    if already_exists {
        // Secondary: wake the primary (best-effort) and report Secondary.
        let _ = signal_named_event(&event_name);
        drop(mutex);
        return Ok(InstanceRole::Secondary);
    }

    let event = Arc::new(
        taskmanager_windows_api::InstanceEvent::create(&event_name)
            .map_err(|_| InstanceFailure::Rejected)?,
    );
    let stop = Arc::new(AtomicBool::new(false));
    let join = spawn_activation_thread(event.clone(), events, stop.clone())?;

    Ok(InstanceRole::Primary(Box::new(WindowsInstanceGuard {
        _mutex: mutex,
        _event: event,
        stop,
        join: Mutex::new(Some(join)),
        event_name,
    })))
}

#[cfg(windows)]
fn spawn_activation_thread(
    event: Arc<taskmanager_windows_api::InstanceEvent>,
    events: Sender<InstanceEvent>,
    stop: Arc<AtomicBool>,
) -> Result<JoinHandle<()>, InstanceFailure> {
    std::thread::Builder::new()
        .name("taskmanager:single-instance".into())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if event.wait().is_ok() {
                    let _ = events.send(InstanceEvent::Activate);
                }
            }
        })
        .map_err(|_| InstanceFailure::Rejected)
}

/// Holds the primary's mutex + event + activation thread alive. `event` is
/// never read directly; the Arc keeps the named event handle open so the
/// activation thread can keep waiting on it.
#[cfg(windows)]
struct WindowsInstanceGuard {
    _mutex: InstanceMutex,
    _event: Arc<taskmanager_windows_api::InstanceEvent>,
    stop: Arc<AtomicBool>,
    join: Mutex<Option<JoinHandle<()>>>,
    event_name: String,
}

#[cfg(windows)]
impl InstanceGuard for WindowsInstanceGuard {}

#[cfg(windows)]
impl Drop for WindowsInstanceGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wake the blocked wait so it can observe `stop` and exit.
        let _ = signal_named_event(&self.event_name);
        if let Some(handle) = self.join.lock().ok().and_then(|mut guard| guard.take()) {
            let _ = handle.join();
        }
        // `event` and `_mutex` drop here: the last event handle closes and
        // the named mutex is destroyed, releasing the instance.
    }
}

#[cfg(test)]
#[path = "../tests/headless/platform_windows_instance.rs"]
mod tests;
