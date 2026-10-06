//! Measured device charts consume identity-scoped histories through declared resources.
use crate::app::ShellTrack;
use crate::drain::ShellProjectionFolded;
use crate::palette::UiPalette;
use crate::widgets::chart::{CurveMeasurement, CurvePaintAccess, paint_curve_at_size};
use crate::window::{Role, TextRole};
use bevy::app::{App, PostUpdate};
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, ResMut};
use bevy::scene::{Scene, bsn};
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, FlexDirection, Node, Overflow, UiSystems, percent, px};
use taskmanager_application::i18n::t;
use taskmanager_core::core::identity::DeviceGeneration;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::{bytes, graph_summary};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DeviceCurveKind {
    #[default]
    DiskRead,
    DiskWrite,
    DiskActive,
    BatteryCharge,
    BatteryPower,
    FanRpm,
}

#[derive(Component, Clone, Default)]
pub(crate) struct DeviceCurve {
    pub(crate) kind: DeviceCurveKind,
    pub(crate) id: String,
    generation: DeviceGeneration,
    color: Color,
}
#[derive(Resource, Default)]
pub(crate) struct DeviceCurveRefresh(bool);

#[derive(Component, Clone, Default)]
pub(crate) struct DeviceCurveStatus(pub(crate) DeviceCurve);

impl DeviceCurve {
    pub(crate) fn samples(&self, shell: &ShellApp) -> Vec<f32> {
        let history = &shell.history;
        match self.kind {
            DeviceCurveKind::DiskRead => {
                history.disk_read_bytes_per_sec_for(&self.id, self.generation.get())
            }
            DeviceCurveKind::DiskWrite => {
                history.disk_write_bytes_per_sec_for(&self.id, self.generation.get())
            }
            DeviceCurveKind::DiskActive => {
                history.disk_active_time_pct_for(&self.id, self.generation.get())
            }
            DeviceCurveKind::BatteryCharge => history.battery_capacity_pct_for(&self.id),
            DeviceCurveKind::BatteryPower => history.battery_power_w_for(&self.id),
            DeviceCurveKind::FanRpm => history.fan_rpm_for(&self.id),
        }
    }
}

pub(super) fn scene(
    kind: DeviceCurveKind,
    id: &str,
    generation: DeviceGeneration,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let label = t(match kind {
        DeviceCurveKind::DiskRead => "disk.read",
        DeviceCurveKind::DiskWrite => "disk.write",
        DeviceCurveKind::DiskActive => "disk.active_time",
        DeviceCurveKind::BatteryCharge => "battery.capacity",
        DeviceCurveKind::BatteryPower => "battery.power",
        DeviceCurveKind::FanRpm => "fan.rpm",
    });
    let id = id.to_owned();
    let status = DeviceCurve {
        kind,
        id: id.clone(),
        generation,
        color: palette.accent,
    };
    bsn! {
        Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: px(4.0) }
        Children [
            Text(label) TextRole(Role::Caption) --
            Node { width: percent(100), height: px(64.0), flex_shrink: 0.0, overflow: Overflow::clip() }
            DeviceCurve { kind: {kind}, id: {id}, generation: {generation}, color: {palette.accent} } CurveMeasurement(None) Children [] --
            Text(t("perf.collecting_samples")) TextRole(Role::Caption) DeviceCurveStatus({status})
        ]
    }
}

fn request_refresh(_fold: On<ShellProjectionFolded>, mut refresh: ResMut<DeviceCurveRefresh>) {
    refresh.0 = true;
}

pub(crate) fn paint_curves(
    track: ShellTrack,
    mut refresh: ResMut<DeviceCurveRefresh>,
    mut curves: Query<(Entity, &DeviceCurve, &ComputedNode, &mut CurveMeasurement)>,
    mut access: CurvePaintAccess,
    mut statuses: Query<(&DeviceCurveStatus, &mut Text)>,
) {
    let mut painted = false;
    for (entity, curve, node, mut measurement) in &mut curves {
        let size = node.size() * node.inverse_scale_factor();
        if size.x <= 0.0 || size.y <= 0.0 || (!refresh.0 && measurement.0 == Some((size.x, size.y)))
        {
            continue;
        }
        let samples = curve.samples(track.shell());
        let ceiling = match curve.kind {
            DeviceCurveKind::DiskActive | DeviceCurveKind::BatteryCharge => 100.0,
            _ => {
                samples
                    .iter()
                    .copied()
                    .filter(|sample| sample.is_finite())
                    .fold(1.0_f32, f32::max)
                    * 1.1
            }
        };
        if paint_curve_at_size(&mut access, entity, size, &samples, ceiling, curve.color) {
            measurement.0 = Some((size.x, size.y));
            painted = true;
        }
    }
    if painted || refresh.0 {
        for (status, mut text) in &mut statuses {
            let samples = status.0.samples(track.shell());
            let summary = (samples.iter().filter(|sample| sample.is_finite()).count() >= 2)
                .then(|| graph_summary(&samples))
                .flatten();
            text.0 = summary.map_or_else(
                || t("perf.collecting_samples").to_owned(),
                |summary| {
                    let format = |value: f32| match status.0.kind {
                        DeviceCurveKind::DiskRead | DeviceCurveKind::DiskWrite => {
                            format!("{}/s", bytes(value.max(0.0) as u64))
                        }
                        DeviceCurveKind::BatteryPower => format!("{value:.1} W"),
                        DeviceCurveKind::FanRpm => format!("{value:.0} RPM"),
                        _ => format!("{value:.0}%"),
                    };
                    format!(
                        "{} {} · {} {} · {} {}",
                        t("common.latest"),
                        format(summary.latest),
                        t("common.avg"),
                        format(summary.average),
                        t("common.peak"),
                        format(summary.maximum)
                    )
                },
            );
        }
    }
    refresh.0 = false;
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<DeviceCurveRefresh>();
    app.add_observer(request_refresh);
    app.add_systems(PostUpdate, paint_curves.after(UiSystems::PostLayout));
}
