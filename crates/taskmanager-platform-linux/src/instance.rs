//! Linux single-instance adapter (borrowed core from
//! `tauri-plugin-single-instance`): a D-Bus session-bus well-known name.
//!
//! Ownership of the well-known name is atomic: only one process can own
//! `org.taskforest.<name>`. The primary serves an `Activate` method on it
//! (the blocking connection drives its own background executor thread); a
//! secondary launch fails to take the name, calls `Activate` on the primary
//! (best-effort), and reports `Secondary`. Reuses the adapter's existing
//! `zbus` dependency; `#![forbid(unsafe_code)]`.

#![forbid(unsafe_code)]

use std::{sync::mpsc::Sender, time::Duration};

use taskmanager_platform_contract::{InstanceEvent, InstanceFailure, InstanceGuard, InstanceRole};

const DBUS_INTERFACE: &str = "org.taskforest.Instance";
const DBUS_PATH: &str = "/org/taskforest/Instance";
const DBUS_METHOD_TIMEOUT: Duration = Duration::from_millis(500);

/// The D-Bus method served by the primary; a secondary call activates the
/// existing instance.
struct Activator {
    events: Sender<InstanceEvent>,
}

#[zbus::interface(name = "org.taskforest.Instance")]
impl Activator {
    fn activate(&mut self) {
        let _ = self.events.send(InstanceEvent::Activate);
    }
}

/// The well-known bus name derives from the stable instance name.
fn bus_name(instance_name: &str) -> String {
    format!("org.taskforest.{instance_name}")
}

/// Acquire the single-instance ownership.
///
/// `Primary(guard)`: hold `guard` for the process lifetime (it keeps the bus
/// name + served interface alive); incoming `Activate` calls are forwarded to
/// `events`. `Secondary`: another instance owns the name and has been asked
/// to show its window (best-effort).
pub fn acquire_single_instance(
    instance_name: &str,
    events: Sender<InstanceEvent>,
) -> Result<InstanceRole, InstanceFailure> {
    #[cfg(target_os = "linux")]
    {
        acquire_linux(instance_name, events)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (instance_name, events);
        Err(InstanceFailure::Unsupported)
    }
}

#[cfg(target_os = "linux")]
fn acquire_linux(
    instance_name: &str,
    events: Sender<InstanceEvent>,
) -> Result<InstanceRole, InstanceFailure> {
    use zbus::blocking::connection::Builder;

    let name = bus_name(instance_name);
    let activator = Activator { events };
    match Builder::session()
        .map_err(|_| InstanceFailure::MissingDependency)?
        .name(name.as_str())
        .map_err(|_| InstanceFailure::Rejected)?
        .replace_existing_names(false)
        .allow_name_replacements(false)
        .method_timeout(DBUS_METHOD_TIMEOUT)
        .serve_at(DBUS_PATH, activator)
        .map_err(|_| InstanceFailure::Rejected)?
        .build()
    {
        Ok(connection) => Ok(InstanceRole::Primary(Box::new(LinuxInstanceGuard {
            _connection: connection,
        }))),
        Err(zbus::Error::NameTaken) => {
            // Secondary: ask the existing instance to show its window.
            notify_primary(&name);
            Ok(InstanceRole::Secondary)
        }
        Err(_) => Err(InstanceFailure::Rejected),
    }
}

#[cfg(target_os = "linux")]
fn notify_primary(bus_name: &str) {
    use zbus::blocking::connection::Builder;

    // A secondary must never remain alive just because a stale or partially
    // initialized primary owns the name but does not answer Activate yet.
    // The name collision already established the single-instance decision;
    // activation is best-effort and bounded.
    if let Ok(connection) =
        Builder::session().and_then(|builder| builder.method_timeout(DBUS_METHOD_TIMEOUT).build())
    {
        let _ = connection.call_method(
            Some(bus_name),
            DBUS_PATH,
            Some(DBUS_INTERFACE),
            "Activate",
            &(),
        );
    }
}

/// Holds the primary's bus-name ownership alive. The connection is never
/// read directly; its lifetime is the point (dropping it releases the name).
#[cfg(target_os = "linux")]
struct LinuxInstanceGuard {
    _connection: zbus::blocking::Connection,
}

#[cfg(target_os = "linux")]
impl InstanceGuard for LinuxInstanceGuard {}

#[cfg(test)]
#[path = "../tests/headless/linux_instance_tests.rs"]
mod tests;
