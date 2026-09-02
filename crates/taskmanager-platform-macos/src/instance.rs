//! macOS single-instance adapter (borrowed core from
//! `tauri-plugin-single-instance`): a per-user Unix domain socket in the
//! process's temp directory.
//!
//! Only one process can bind the socket path, which gives atomic exclusivity.
//! A second launch connects to it, writes an `activate` request (best-effort),
//! and reports `Secondary`; the primary's accept loop forwards
//! [`InstanceEvent::Activate`] to the frontend. A stale socket from a crashed
//! primary is detected (connect fails with `ConnectionRefused`) and replaced.
//! Pure `std::os::unix::net`; `#![forbid(unsafe_code)]`.

#![forbid(unsafe_code)]

// The macOS implementation is Unix-socket based; the imports it needs are
// gated so the module (and its typed `Unsupported` fallback) still compiles
// — and its contract test still runs — on every other host.
#[cfg(target_os = "macos")]
use std::io::{ErrorKind, Read, Write};
#[cfg(target_os = "macos")]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(any(target_os = "macos", test))]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "macos")]
use std::thread::JoinHandle;

#[cfg(target_os = "macos")]
use taskmanager_platform_contract::InstanceGuard;
use taskmanager_platform_contract::{InstanceEvent, InstanceFailure, InstanceRole};

/// Wire payload a secondary writes to the primary's socket.
#[cfg(target_os = "macos")]
const ACTIVATE_PAYLOAD: &[u8] = b"activate";

/// Maximum payload a primary accepts from a connection (bounds work).
#[cfg(target_os = "macos")]
const MAX_PAYLOAD_BYTES: usize = 64;

/// An accepted local client is not trusted to finish its write. The primary
/// must still be able to release the singleton within a bounded interval.
#[cfg(target_os = "macos")]
const CLIENT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

/// Socket path in the process's (per-user on macOS) temp directory.
#[cfg(any(target_os = "macos", test))]
fn socket_path(instance_name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("taskmanager.{instance_name}.sock"))
}

/// Acquire the single-instance ownership.
///
/// `Primary(guard)`: hold `guard` for the process lifetime; the accept loop
/// forwards [`InstanceEvent::Activate`] to `events`. `Secondary`: another
/// instance exists and has been asked to show its window (best-effort).
pub fn acquire_single_instance(
    instance_name: &str,
    events: Sender<InstanceEvent>,
) -> Result<InstanceRole, InstanceFailure> {
    #[cfg(target_os = "macos")]
    {
        acquire_macos(instance_name, events)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (instance_name, events);
        Err(InstanceFailure::Unsupported)
    }
}

#[cfg(target_os = "macos")]
fn acquire_macos(
    instance_name: &str,
    events: Sender<InstanceEvent>,
) -> Result<InstanceRole, InstanceFailure> {
    let path = socket_path(instance_name);
    match bind_or_activate(&path) {
        BindOutcome::Primary(listener) => {
            let stop = Arc::new(AtomicBool::new(false));
            let join = spawn_accept_loop(listener, events, stop.clone())?;
            Ok(InstanceRole::Primary(Box::new(MacosInstanceGuard {
                path,
                stop,
                join: Mutex::new(Some(join)),
            })))
        }
        BindOutcome::Secondary => Ok(InstanceRole::Secondary),
        BindOutcome::Rejected => Err(InstanceFailure::Rejected),
    }
}

#[cfg(target_os = "macos")]
enum BindOutcome {
    Primary(UnixListener),
    Secondary,
    Rejected,
}

#[cfg(target_os = "macos")]
fn bind_or_activate(path: &Path) -> BindOutcome {
    match UnixListener::bind(path) {
        Ok(listener) => BindOutcome::Primary(listener),
        Err(_) => {
            // A socket exists: it is either a live primary or a stale one.
            if notify_existing(path) {
                BindOutcome::Secondary
            } else {
                // Stale socket from a crashed primary: remove and rebind once.
                let _ = std::fs::remove_file(path);
                match UnixListener::bind(path) {
                    Ok(listener) => BindOutcome::Primary(listener),
                    Err(_) => BindOutcome::Rejected,
                }
            }
        }
    }
}

/// Try to connect and write the activation payload. `true` if a live primary
/// accepted the request.
#[cfg(target_os = "macos")]
fn notify_existing(path: &Path) -> bool {
    let Ok(mut stream) = UnixStream::connect(path) else {
        return false;
    };
    if stream.write_all(ACTIVATE_PAYLOAD).is_err() {
        return false;
    }
    let _ = stream.flush();
    true
}

#[cfg(target_os = "macos")]
fn spawn_accept_loop(
    listener: UnixListener,
    events: Sender<InstanceEvent>,
    stop: Arc<AtomicBool>,
) -> Result<JoinHandle<()>, InstanceFailure> {
    listener
        .set_nonblocking(true)
        .map_err(|_| InstanceFailure::Rejected)?;
    std::thread::Builder::new()
        .name("taskmanager:single-instance".into())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _addr)) => {
                        if stream.set_nonblocking(false).is_err() {
                            continue;
                        }
                        let _ = stream.set_read_timeout(Some(CLIENT_READ_TIMEOUT));
                        let mut buffer = [0u8; MAX_PAYLOAD_BYTES];
                        if stream.read(&mut buffer).is_ok() && buffer.starts_with(ACTIVATE_PAYLOAD)
                        {
                            let _ = events.send(InstanceEvent::Activate);
                        }
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
        })
        .map_err(|_| InstanceFailure::Rejected)
}

/// Holds the primary's socket file + accept loop alive.
#[cfg(target_os = "macos")]
struct MacosInstanceGuard {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    join: Mutex<Option<JoinHandle<()>>>,
}

#[cfg(target_os = "macos")]
impl InstanceGuard for MacosInstanceGuard {}

#[cfg(target_os = "macos")]
impl Drop for MacosInstanceGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.join.lock().ok().and_then(|mut guard| guard.take()) {
            let _ = handle.join();
        }
        // Remove the socket file so the next launch can bind the path.
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
#[path = "../tests/headless/macos_instance.rs"]
mod tests;
