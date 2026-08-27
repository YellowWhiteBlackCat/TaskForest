//! Bounded read-only Wayland compositor session facts.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum, event_created_child};

mod current_kde_protocol {
    use wayland_client;

    pub mod __interfaces {
        wayland_scanner::generate_interfaces!("protocols/kde-output-device-v2.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/kde-output-device-v2.xml");
}

use current_kde_protocol::{
    kde_output_device_mode_v2, kde_output_device_registry_v2, kde_output_device_v2,
};

const WAYLAND_CORE_SOURCE: &str = "wl_output";
const WAYLAND_SOURCE: &str = "kde_output_device_v2";
/// Session facts obtained from the compositor without sending any output
/// configuration request. The protocol objects are dropped after the bounded
/// snapshot, so no compositor state is retained by the application.
#[derive(Debug, Clone, Default)]
pub(in super::super) struct WaylandSessionFacts {
    pub displays: Vec<WaylandDisplayFacts>,
    pub compositor: Option<String>,
    pub compositor_backend: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(in super::super) struct WaylandDisplayFacts {
    pub name: Option<String>,
    pub width_mm: Option<u32>,
    pub height_mm: Option<u32>,
    pub current_width_px: Option<u32>,
    pub current_height_px: Option<u32>,
    pub current_refresh_hz: Option<f32>,
    pub hdr_supported: Option<bool>,
    pub hdr_enabled: Option<bool>,
    pub vrr_supported: Option<bool>,
    pub vrr_policy: Option<String>,
    pub enabled: Option<bool>,
    pub current_mode_source: Option<String>,
    pub hdr_source: Option<String>,
}

#[derive(Debug, Default)]
struct WaylandState {
    outputs: Vec<WaylandOutputState>,
    kde_outputs: Vec<KdeOutputState>,
    mode_owners: HashMap<u32, usize>,
    modes: HashMap<u32, KdeModeState>,
}

#[derive(Debug, Default)]
struct WaylandOutputState {
    object_id: u32,
    name: Option<String>,
    width_mm: Option<u32>,
    height_mm: Option<u32>,
    current_width_px: Option<u32>,
    current_height_px: Option<u32>,
    current_refresh_hz: Option<f32>,
}

#[derive(Debug, Default)]
struct KdeOutputState {
    object_id: u32,
    name: Option<String>,
    width_mm: Option<u32>,
    height_mm: Option<u32>,
    current_mode_id: Option<u32>,
    current_width_px: Option<u32>,
    current_height_px: Option<u32>,
    current_refresh_hz: Option<f32>,
    hdr_supported: Option<bool>,
    hdr_enabled: Option<bool>,
    vrr_supported: Option<bool>,
    vrr_policy: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Default)]
struct KdeModeState {
    width_px: Option<u32>,
    height_px: Option<u32>,
    refresh_hz: Option<f32>,
}

/// Probe the active Wayland session with a read-only, bounded client. KDE's
/// output-device protocol is preferred when advertised because it exposes the
/// compositor's current mode and HDR state; core `wl_output` remains a useful
/// fallback for current mode information on other compositors.
pub(in super::super) fn probe_wayland() -> Option<WaylandSessionFacts> {
    std::env::var_os("WAYLAND_DISPLAY")?;
    let connection = Connection::connect_to_env().ok()?;
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());

    let mut state = WaylandState::default();
    let deadline = Instant::now() + Duration::from_millis(300);
    drain_wayland(&mut queue, &mut state, deadline).ok()?;
    let mut facts = session_facts(&state);
    if facts.displays.is_empty() && facts.compositor.is_none() {
        None
    } else {
        facts
            .displays
            .sort_by(|left, right| left.name.cmp(&right.name));
        Some(std::mem::take(&mut facts))
    }
}

fn session_facts(state: &WaylandState) -> WaylandSessionFacts {
    let mut by_name = BTreeMap::<String, WaylandDisplayFacts>::new();
    for output in &state.outputs {
        let Some(name) = output.name.clone().filter(|name| !name.is_empty()) else {
            continue;
        };
        let entry = by_name.entry(name.clone()).or_default();
        entry.name = Some(name);
        entry.width_mm = output.width_mm;
        entry.height_mm = output.height_mm;
        entry.current_width_px = output.current_width_px;
        entry.current_height_px = output.current_height_px;
        entry.current_refresh_hz = output.current_refresh_hz;
        if output.current_width_px.is_some() || output.current_refresh_hz.is_some() {
            entry.current_mode_source = Some(WAYLAND_CORE_SOURCE.to_owned());
        }
    }
    for output in &state.kde_outputs {
        let Some(name) = output.name.clone().filter(|name| !name.is_empty()) else {
            continue;
        };
        let entry = by_name.entry(name).or_default();
        entry.width_mm = output.width_mm.or(entry.width_mm);
        entry.height_mm = output.height_mm.or(entry.height_mm);
        entry.current_width_px = output.current_width_px.or(entry.current_width_px);
        entry.current_height_px = output.current_height_px.or(entry.current_height_px);
        entry.current_refresh_hz = output.current_refresh_hz.or(entry.current_refresh_hz);
        if output.current_width_px.is_some() || output.current_refresh_hz.is_some() {
            entry.current_mode_source = Some(WAYLAND_SOURCE.to_owned());
        }
        entry.hdr_supported = output.hdr_supported;
        entry.hdr_enabled = output.hdr_enabled;
        if output.hdr_supported.is_some() || output.hdr_enabled.is_some() {
            entry.hdr_source = Some(WAYLAND_SOURCE.to_owned());
        }
        entry.vrr_supported = output.vrr_supported;
        entry.vrr_policy.clone_from(&output.vrr_policy);
        entry.enabled = output.enabled;
    }
    WaylandSessionFacts {
        displays: by_name.into_values().collect(),
        compositor: (!state.kde_outputs.is_empty()).then(|| "KWin".to_owned()),
        compositor_backend: (!state.kde_outputs.is_empty()).then(|| "Wayland".to_owned()),
    }
}

fn drain_wayland(
    queue: &mut wayland_client::EventQueue<WaylandState>,
    state: &mut WaylandState,
    deadline: Instant,
) -> Result<(), ()> {
    let mut saw_event = false;
    loop {
        queue.dispatch_pending(state).map_err(|_| ())?;
        if Instant::now() >= deadline {
            return saw_event.then_some(()).ok_or(());
        }
        queue.flush().map_err(|_| ())?;
        let Some(read_guard) = queue.prepare_read() else {
            continue;
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        let wait = if saw_event {
            remaining.min(Duration::from_millis(40))
        } else {
            remaining
        };
        let timeout = PollTimeout::try_from(wait).unwrap_or(PollTimeout::MAX);
        let mut fds = [PollFd::new(read_guard.connection_fd(), PollFlags::POLLIN)];
        let ready = poll(&mut fds, timeout).map_err(|_| ())?;
        if ready == 0 {
            drop(read_guard);
            return saw_event.then_some(()).ok_or(());
        }
        read_guard.read().map_err(|_| ())?;
        saw_event = true;
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for WaylandState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_output" => {
                let index = state.outputs.len();
                state.outputs.push(WaylandOutputState::default());
                let output =
                    registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, ());
                state.outputs[index].object_id = output.id().protocol_id();
            }
            "kde_output_device_registry_v2" => {
                registry.bind::<kde_output_device_registry_v2::KdeOutputDeviceRegistryV2, _, _>(
                    name,
                    version.min(21),
                    qh,
                    (),
                );
            }
            _ => {}
        }
    }
}

impl Dispatch<kde_output_device_registry_v2::KdeOutputDeviceRegistryV2, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &kde_output_device_registry_v2::KdeOutputDeviceRegistryV2,
        event: kde_output_device_registry_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let kde_output_device_registry_v2::Event::Output { output } = event {
            state.kde_outputs.push(KdeOutputState {
                object_id: output.id().protocol_id(),
                ..KdeOutputState::default()
            });
        }
    }

    event_created_child!(
        WaylandState,
        kde_output_device_registry_v2::KdeOutputDeviceRegistryV2,
        [kde_output_device_registry_v2::EVT_OUTPUT_OPCODE =>
            (kde_output_device_v2::KdeOutputDeviceV2, ())]
    );
}

impl Dispatch<wl_output::WlOutput, ()> for WaylandState {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(index) = state
            .outputs
            .iter()
            .position(|item| item.object_id == output.id().protocol_id())
        else {
            return;
        };
        let Some(target) = state.outputs.get_mut(index) else {
            return;
        };
        match event {
            wl_output::Event::Geometry {
                physical_width,
                physical_height,
                ..
            } => {
                target.width_mm = u32::try_from(physical_width)
                    .ok()
                    .filter(|value| *value > 0);
                target.height_mm = u32::try_from(physical_height)
                    .ok()
                    .filter(|value| *value > 0);
            }
            wl_output::Event::Mode {
                flags,
                width,
                height,
                refresh,
            } => {
                if matches!(flags, WEnum::Value(value) if value.contains(wl_output::Mode::Current))
                {
                    target.current_width_px = u32::try_from(width).ok().filter(|value| *value > 0);
                    target.current_height_px =
                        u32::try_from(height).ok().filter(|value| *value > 0);
                    target.current_refresh_hz = refresh_hz(refresh);
                }
            }
            wl_output::Event::Name { name } => target.name = Some(name),
            wl_output::Event::Description { .. }
            | wl_output::Event::Done
            | wl_output::Event::Scale { .. } => {}
            _ => {}
        }
        let _ = output;
    }
}

impl Dispatch<kde_output_device_v2::KdeOutputDeviceV2, ()> for WaylandState {
    fn event(
        state: &mut Self,
        output: &kde_output_device_v2::KdeOutputDeviceV2,
        event: kde_output_device_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(index) = state
            .kde_outputs
            .iter()
            .position(|item| item.object_id == output.id().protocol_id())
        else {
            return;
        };
        match event {
            kde_output_device_v2::Event::Geometry {
                physical_width,
                physical_height,
                make,
                model,
                ..
            } => {
                let Some(target) = state.kde_outputs.get_mut(index) else {
                    return;
                };
                target.width_mm = u32::try_from(physical_width)
                    .ok()
                    .filter(|value| *value > 0);
                target.height_mm = u32::try_from(physical_height)
                    .ok()
                    .filter(|value| *value > 0);
                if target.name.is_none() && !model.trim().is_empty() {
                    let _ = make;
                }
            }
            kde_output_device_v2::Event::CurrentMode { mode } => {
                let mode_id = mode.id().protocol_id();
                let Some(target) = state.kde_outputs.get_mut(index) else {
                    return;
                };
                target.current_mode_id = Some(mode_id);
                apply_current_kde_mode(state, index);
            }
            kde_output_device_v2::Event::Mode { mode } => {
                state.mode_owners.insert(mode.id().protocol_id(), index);
            }
            kde_output_device_v2::Event::Name { name } => {
                if let Some(target) = state.kde_outputs.get_mut(index) {
                    target.name = Some(name);
                }
            }
            kde_output_device_v2::Event::Enabled { enabled } => {
                if let Some(target) = state.kde_outputs.get_mut(index) {
                    target.enabled = Some(enabled != 0);
                }
            }
            kde_output_device_v2::Event::Capabilities { flags } => {
                // A compositor may advertise capability bits introduced after
                // the checked-in protocol revision. Preserve the raw bitfield
                // instead of dropping the whole event as an unknown enum.
                let flags = match flags {
                    WEnum::Value(flags) => flags.bits(),
                    WEnum::Unknown(flags) => flags,
                };
                if let Some(target) = state.kde_outputs.get_mut(index) {
                    target.hdr_supported = Some(flags & 0x8 != 0);
                    target.vrr_supported = Some(flags & 0x2 != 0);
                }
            }
            kde_output_device_v2::Event::HighDynamicRange { hdr_enabled } => {
                if let Some(target) = state.kde_outputs.get_mut(index) {
                    target.hdr_enabled = Some(hdr_enabled != 0);
                }
            }
            kde_output_device_v2::Event::VrrPolicy {
                vrr_policy: WEnum::Value(policy),
            } => {
                if let Some(target) = state.kde_outputs.get_mut(index) {
                    target.vrr_policy = Some(
                        match policy {
                            kde_output_device_v2::VrrPolicy::Never => "never",
                            kde_output_device_v2::VrrPolicy::Always => "always",
                            kde_output_device_v2::VrrPolicy::Automatic => "automatic",
                        }
                        .to_owned(),
                    );
                }
            }
            _ => {}
        }
    }

    event_created_child!(WaylandState, kde_output_device_v2::KdeOutputDeviceV2, [
        kde_output_device_v2::EVT_MODE_OPCODE =>
            (kde_output_device_mode_v2::KdeOutputDeviceModeV2, ())
    ]);
}

impl Dispatch<kde_output_device_mode_v2::KdeOutputDeviceModeV2, ()> for WaylandState {
    fn event(
        state: &mut Self,
        mode: &kde_output_device_mode_v2::KdeOutputDeviceModeV2,
        event: kde_output_device_mode_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mode_id = mode.id().protocol_id();
        let values = state.modes.entry(mode_id).or_default();
        match event {
            kde_output_device_mode_v2::Event::Size { width, height } => {
                values.width_px = u32::try_from(width).ok().filter(|value| *value > 0);
                values.height_px = u32::try_from(height).ok().filter(|value| *value > 0);
            }
            kde_output_device_mode_v2::Event::Refresh { refresh } => {
                values.refresh_hz = refresh_hz(refresh);
            }
            kde_output_device_mode_v2::Event::Preferred
            | kde_output_device_mode_v2::Event::Removed => {}
            _ => {}
        }
        if let Some(index) = state.mode_owners.get(&mode_id).copied() {
            apply_current_kde_mode(state, index);
        }
    }
}

fn apply_current_kde_mode(state: &mut WaylandState, index: usize) {
    let Some(mode_id) = state
        .kde_outputs
        .get(index)
        .and_then(|output| output.current_mode_id)
    else {
        return;
    };
    let Some(mode) = state.modes.get(&mode_id) else {
        return;
    };
    let Some(output) = state.kde_outputs.get_mut(index) else {
        return;
    };
    output.current_width_px = mode.width_px;
    output.current_height_px = mode.height_px;
    output.current_refresh_hz = mode.refresh_hz;
}

fn refresh_hz(refresh_millihz: i32) -> Option<f32> {
    let refresh_millihz = u32::try_from(refresh_millihz).ok()?;
    (refresh_millihz > 0 && refresh_millihz < 1_000_000).then(|| refresh_millihz as f32 / 1_000.0)
}
