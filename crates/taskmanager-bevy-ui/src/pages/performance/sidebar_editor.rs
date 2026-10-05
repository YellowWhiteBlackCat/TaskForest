//! Device preferences use immutable config publications and declared access sets.
use super::{PerformanceDeviceTarget, metrics};
use crate::app::{FrontendTrack, RouteChanged, SharedRuntimeHandle};
use crate::palette::{UiPalette, space_8};
use crate::window::{DemoMode, Role, TextRole};
use crate::window_surface::{
    WindowSurfaceChanged, WindowSurfaceCommand, WindowSurfaceKind, modal_scene,
};
use bevy::app::{App, PreUpdate};
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res, ResMut};
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::ui::widget::Text;
use bevy::ui::{BackgroundColor, FlexDirection, FlexWrap, Node, UiRect, percent, px};
use bevy::ui_widgets::{Activate, Button};
use std::sync::Arc;
use taskmanager_application::{ConfigSubmissionStatus, ConfigSubmitError};
use taskmanager_core::core::config::Config;
use taskmanager_core::core::config::sidebar::{
    move_sidebar_order, ordered_indices, set_sidebar_override, visible_with_override,
};
use taskmanager_core::core::metrics::NetworkAdapterType;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

#[derive(Resource, Default)]
pub(crate) struct SidebarState(pub(crate) Arc<Config>);

impl PerformanceDeviceTarget {
    pub(crate) fn sidebar_key(&self) -> String {
        match self {
            Self::Cpu => "cpu".into(),
            Self::Memory => "memory".into(),
            Self::Disk(id) => format!("disk:{id}"),
            Self::Network(id) => format!("network:{id}"),
            Self::Gpu(id) => format!("gpu:{id}"),
            Self::Battery(id) => format!("battery:{id}"),
        }
    }
}

impl SidebarState {
    pub(crate) fn visible(&self, target: &PerformanceDeviceTarget, shell: &ShellApp) -> bool {
        let category = match target {
            PerformanceDeviceTarget::Cpu => self.0.show_cpu,
            PerformanceDeviceTarget::Memory => self.0.show_memory,
            PerformanceDeviceTarget::Disk(_) => self.0.show_disks,
            PerformanceDeviceTarget::Network(id) => {
                self.0.show_network
                    && metrics::network_devices(shell)
                        .and_then(|devices| {
                            devices
                                .iter()
                                .find(|device| device.device_id.as_ref() == id)
                        })
                        .is_some_and(|device| match device.adapter_type() {
                            NetworkAdapterType::Ethernet => self.0.show_network_wired,
                            NetworkAdapterType::WiFi => self.0.show_network_wireless,
                            NetworkAdapterType::Vpn => self.0.show_network_vpn,
                            NetworkAdapterType::Virtual => self.0.show_network_virtual,
                            NetworkAdapterType::Other
                            | NetworkAdapterType::Unknown
                            | NetworkAdapterType::Loopback => self.0.show_network_other,
                        })
            }
            PerformanceDeviceTarget::Gpu(_) => self.0.show_gpus,
            PerformanceDeviceTarget::Battery(_) => true,
        };
        visible_with_override(
            &target.sidebar_key(),
            category,
            &self.0.sidebar_device_overrides,
        )
    }
    pub(crate) fn order(&self, targets: &[PerformanceDeviceTarget]) -> Vec<usize> {
        ordered_indices(
            &targets
                .iter()
                .map(PerformanceDeviceTarget::sidebar_key)
                .collect::<Vec<_>>(),
            &self.0.sidebar_order,
        )
    }
}

pub(crate) fn entries(shell: &ShellApp) -> Vec<(PerformanceDeviceTarget, String)> {
    let mut entries = vec![
        (PerformanceDeviceTarget::Cpu, "CPU".into()),
        (PerformanceDeviceTarget::Memory, "Memory".into()),
    ];
    if let Some(disks) = metrics::disks(shell) {
        entries.extend(disks.iter().map(|disk| {
            (
                PerformanceDeviceTarget::Disk(disk.device_id.clone()),
                if disk.model.is_empty() {
                    disk.name.clone()
                } else {
                    disk.model.clone()
                },
            )
        }));
    }
    if let Some(networks) = metrics::network_devices(shell) {
        entries.extend(networks.iter().map(|nic| {
            (
                PerformanceDeviceTarget::Network(nic.device_id.to_string()),
                nic.interface_name.to_string(),
            )
        }));
    }
    if let Some(gpus) = metrics::gpu_devices(shell) {
        entries.extend(gpus.iter().map(|gpu| {
            (
                PerformanceDeviceTarget::Gpu(gpu.device_id.clone()),
                gpu.device_id.clone(),
            )
        }));
    }
    if let Some(batteries) = metrics::batteries(shell) {
        entries.extend(batteries.iter().map(|battery| {
            (
                PerformanceDeviceTarget::Battery(battery.id.clone()),
                battery.display_name.clone(),
            )
        }));
    }
    entries
}

#[derive(Component, Clone, Default)]
pub(crate) struct SidebarControl(pub(crate) SidebarAction);
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum SidebarAction {
    #[default]
    Open,
    Close,
    Visibility(PerformanceDeviceTarget, bool),
    Move(PerformanceDeviceTarget, isize),
}
pub(crate) fn button(
    label: String,
    action: SidebarAction,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! { Node { min_height: px(palette.control_height_px), padding: UiRect::all(px(space_8())) }
    BackgroundColor({palette.content_bg}) Button SidebarControl({action}) on(activate)
    Children [ Text(label) TextRole(Role::Caption) Pickable::IGNORE ] }
}

pub(crate) fn scene(
    shell: &ShellApp,
    state: &SidebarState,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let entries = entries(shell);
    let targets: Vec<_> = entries.iter().map(|(target, _)| target.clone()).collect();
    let rows: Vec<Box<dyn Scene>> = state.order(&targets).into_iter().map(|index| {
        let (target, label) = &entries[index];
        let visible = state.visible(target, shell);
        let toggle = button(if visible {"Hide"} else {"Show"}.into(), SidebarAction::Visibility(target.clone(), !visible), palette);
        let up = button("Move up".into(), SidebarAction::Move(target.clone(), -1), palette);
        let down = button("Move down".into(), SidebarAction::Move(target.clone(), 1), palette);
        Box::new(bsn! { Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: px(space_8()), padding: UiRect::bottom(px(space_8())) }
            Children [ Text({label.clone()}) TextRole(Role::Body) --
                Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(space_8()) }
                Children [ @{toggle} -- @{up} -- @{down} ] ] }) as Box<dyn Scene>
    }).collect();
    let body = Box::new(
        bsn! { Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(space_8()) } Children [ {rows} ] },
    );
    let close = Box::new(button("Done".into(), SidebarAction::Close, palette));
    modal_scene(
        WindowSurfaceKind::SidebarDevices,
        "Edit devices",
        body,
        vec![close],
        palette,
    )
}

fn apply_edit(next: &mut Config, keys: &[String], action: &SidebarAction) -> bool {
    match action {
        SidebarAction::Visibility(target, visible) if keys.contains(&target.sidebar_key()) => {
            set_sidebar_override(
                &mut next.sidebar_device_overrides,
                &target.sidebar_key(),
                *visible,
            )
        }
        SidebarAction::Move(target, delta) => {
            let Some(order) =
                move_sidebar_order(keys, &next.sidebar_order, &target.sidebar_key(), *delta)
            else {
                return false;
            };
            next.sidebar_order = order;
        }
        _ => return false,
    }
    true
}

fn activate(
    event: On<Activate>,
    controls: Query<&SidebarControl>,
    runtime: Option<Res<SharedRuntimeHandle>>,
    demo: Option<Res<DemoMode>>,
    mut state: ResMut<SidebarState>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    let Ok(control) = controls.get(event.entity) else {
        return;
    };
    match &control.0 {
        SidebarAction::Open => {
            commands.trigger(WindowSurfaceCommand::SidebarDevices);
            return;
        }
        SidebarAction::Close => {
            commands.trigger(WindowSurfaceCommand::Close(
                WindowSurfaceKind::SidebarDevices,
            ));
            return;
        }
        _ => {}
    }
    let live = entries(&track.shell);
    let keys: Vec<_> = live
        .iter()
        .map(|(target, _)| target.sidebar_key())
        .collect();
    let mut next = (*state.0).clone();
    if !apply_edit(&mut next, &keys, &control.0) {
        return;
    }
    if demo.is_some() {
        state.0 = Arc::new(next);
        commands.trigger(RouteChanged);
        commands.trigger(WindowSurfaceChanged);
        return;
    }
    let submitted = runtime
        .as_deref()
        .and_then(|runtime| {
            let guard = runtime.shared.lock_config();
            guard.as_ref().and_then(|client| {
                let mut current = (**client.snapshot()?).clone();
                apply_edit(&mut current, &keys, &control.0).then(|| client.try_submit(current))
            })
        })
        .unwrap_or(Err(ConfigSubmitError::NotReady));
    if let Err(error) = submitted {
        track.shell.report_notice(
            FeedbackSource::Settings,
            FeedbackSeverity::Error,
            FeedbackLifecycle::TIMED_LONG,
            format!("Device preferences: {error}"),
        );
    } else if submitted == Ok(ConfigSubmissionStatus::NoChange) {
        commands.trigger(WindowSurfaceChanged);
    }
}

fn sync_config(
    runtime: Option<Res<SharedRuntimeHandle>>,
    mut state: ResMut<SidebarState>,
    mut track: Option<NonSendMut<FrontendTrack>>,
    mut commands: Commands,
) {
    let Some(runtime) = runtime else {
        return;
    };
    let mut guard = runtime.shared.lock_config();
    let Some(client) = guard.as_mut() else {
        return;
    };
    let drain = client.drain();
    if let Some(publication) = drain
        .latest()
        .filter(|publication| publication.outcome().is_failure())
        && let Some(track) = track.as_mut()
    {
        track.shell.report_notice(
            FeedbackSource::Settings,
            FeedbackSeverity::Error,
            FeedbackLifecycle::UntilReplaced,
            format!("Configuration: {:?}", publication.outcome()),
        );
    }
    let Some(snapshot) = client.snapshot().cloned() else {
        return;
    };
    if state.0 != snapshot {
        state.0 = snapshot;
        commands.trigger(RouteChanged);
        commands.trigger(WindowSurfaceChanged);
    }
}
pub(crate) fn register(app: &mut App) {
    app.init_resource::<SidebarState>();
    app.add_systems(PreUpdate, sync_config);
}

#[cfg(test)]
#[path = "../../../tests/headless/pages/sidebar_editor.rs"]
mod tests;
