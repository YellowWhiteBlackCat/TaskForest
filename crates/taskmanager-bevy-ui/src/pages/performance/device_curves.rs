//! Measured device charts consume identity-scoped histories through declared
//! resources.
//!
//! Every chart is a continuous canvas: the curve is one path per series, the
//! wash under it fades from the line's own ink, and a two-series chart
//! overlays both directions in ONE coordinate system (the read/write pair
//! and the receive/send pair share one scale).

use crate::app::ShellTrack;
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, space_2, space_4, space_8};
use crate::widgets::chart::canvas::{CANVAS_FILL_ALPHA, CanvasFill, CanvasFlow, CanvasGrid};
use crate::widgets::chart::{
    ChartCanvas, ChartContent, ChartPaintState, ChartPointer, ChartStyle, CurveMeasurement,
    CurvePaintAccess,
};
use crate::widgets::layout::PERFORMANCE_CHART_SLOT_HEIGHT_REM;
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
use bevy::ui::{
    AlignItems, BackgroundColor, BorderRadius, ComputedNode, Display, FlexDirection, Node,
    Overflow, PositionType, UiSystems, Val, percent, px, rem,
};
use taskmanager_application::i18n::t;
use taskmanager_core::core::identity::DeviceGeneration;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::gpu_chart_metric::{
    GpuChartMetricUnit, gpu_chart_metric_history,
};
use taskmanager_shell::presentation::{bytes, graph_summary};

pub(crate) mod storage;

/// How far the secondary series' ink is lifted toward white — the reference
/// surface's rule: a tint of the SAME color, never a new product color.
const SECONDARY_TINT_LIFT: f32 = 0.32;

/// The half-width of the travelling highlight, in px (chart geometry).
const FLOW_RADIUS_PX: f32 = 14.0;

/// The grid's division count, matching the reference surface.
const GRID_DIVISIONS: usize = 5;

/// The emphasized quarter rules under the curve.
const GRID_REFERENCE: &[f32] = &[0.25, 0.5, 0.75];

/// The chart a device block draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DeviceCurveKind {
    /// The disk's read and write throughput in one coordinate system.
    #[default]
    DiskThroughput,
    /// The disk's active time — the IOPS-side trend.
    DiskActive,
    /// The interface's receive and send throughput in one coordinate system.
    NetworkThroughput,
    BatteryCharge,
    BatteryPower,
    FanRpm,
    GpuMetric,
}

impl DeviceCurveKind {
    /// The label the chart carries.
    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::DiskThroughput => "disk.throughput",
            Self::DiskActive => "disk.active_time",
            Self::NetworkThroughput => "net.throughput",
            Self::BatteryCharge => "battery.charge_graph",
            Self::BatteryPower => "battery.power_graph",
            Self::FanRpm => "fan.rpm",
            Self::GpuMetric => "gpu.chart_metric",
        }
    }

    /// The readout this chart's values are printed with.
    pub(crate) fn readout(self, shell: &ShellApp) -> fn(f32) -> String {
        match self {
            Self::DiskThroughput | Self::NetworkThroughput => bytes_readout,
            Self::DiskActive | Self::BatteryCharge => percent_readout,
            Self::BatteryPower => watt_readout,
            Self::FanRpm => rpm_readout,
            Self::GpuMetric => match shell.gpu_chart_metric_selected().unit() {
                GpuChartMetricUnit::Percent => percent_readout,
                GpuChartMetricUnit::Watts => watt_readout,
                GpuChartMetricUnit::Celsius => celsius_readout,
                GpuChartMetricUnit::Megahertz => megahertz_readout,
            },
        }
    }

    /// The series a chart draws, in order.
    pub(crate) fn plans(&self, shell: &ShellApp, palette: &UiPalette) -> Vec<DeviceSeriesPlan> {
        let readout = self.readout(shell);
        match self {
            Self::DiskThroughput => vec![
                DeviceSeriesPlan {
                    label: t("disk.read").to_owned(),
                    color: palette.disk,
                    format: bytes_readout,
                },
                DeviceSeriesPlan {
                    label: t("disk.write").to_owned(),
                    color: secondary_ink(palette.disk),
                    format: bytes_readout,
                },
            ],
            Self::NetworkThroughput => vec![
                DeviceSeriesPlan {
                    label: t("net.receive").to_owned(),
                    color: palette.network,
                    format: bytes_readout,
                },
                DeviceSeriesPlan {
                    label: t("net.send").to_owned(),
                    color: secondary_ink(palette.network),
                    format: bytes_readout,
                },
            ],
            Self::DiskActive => vec![DeviceSeriesPlan {
                label: t("disk.active_time").to_owned(),
                color: palette.disk,
                format: readout,
            }],
            Self::BatteryCharge => vec![DeviceSeriesPlan {
                label: t("battery.charge_graph").to_owned(),
                color: palette.success_color,
                format: readout,
            }],
            Self::BatteryPower => vec![DeviceSeriesPlan {
                label: t("battery.power_graph").to_owned(),
                color: palette.warning_color,
                format: readout,
            }],
            Self::FanRpm => vec![DeviceSeriesPlan {
                label: t("fan.rpm").to_owned(),
                color: palette.accent,
                format: readout,
            }],
            Self::GpuMetric => vec![DeviceSeriesPlan {
                // The series label is the SELECTED metric's own localized
                // name (the shared owner provides the key), so the chart
                // names what it measures instead of "a GPU metric".
                label: t(shell.gpu_chart_metric_selected().label_key()).to_owned(),
                color: palette.accent,
                format: readout,
            }],
        }
    }
}

/// One series of a device chart: its label, its ink, and its readout.
pub(crate) struct DeviceSeriesPlan {
    pub(crate) label: String,
    pub(crate) color: Color,
    pub(crate) format: fn(f32) -> String,
}

/// The percent readout.
fn percent_readout(value: f32) -> String {
    format!("{value:.0}%")
}

/// The byte-rate readout: the shared byte formatter is the only authority.
fn bytes_readout(value: f32) -> String {
    format!("{}/s", bytes(value.max(0.0) as u64))
}

/// The watt readout.
fn watt_readout(value: f32) -> String {
    format!("{value:.1} W")
}

/// The fan-speed readout.
fn rpm_readout(value: f32) -> String {
    format!("{value:.0} RPM")
}

/// The degree readout.
fn celsius_readout(value: f32) -> String {
    format!("{value:.0}\u{b0}C")
}

/// The clock readout.
fn megahertz_readout(value: f32) -> String {
    format!("{value:.0} MHz")
}

/// The secondary series' ink: the family token lifted toward white.
fn secondary_ink(base: Color) -> Color {
    let srgba = base.to_srgba();
    Color::srgba(
        srgba.red + (1.0 - srgba.red) * SECONDARY_TINT_LIFT,
        srgba.green + (1.0 - srgba.green) * SECONDARY_TINT_LIFT,
        srgba.blue + (1.0 - srgba.blue) * SECONDARY_TINT_LIFT,
        srgba.alpha,
    )
}

#[derive(Component, Clone, Default)]
pub(crate) struct DeviceCurve {
    pub(crate) kind: DeviceCurveKind,
    pub(crate) id: String,
    pub(crate) generation: DeviceGeneration,
    pub(crate) color: Color,
}

#[derive(Resource, Default)]
pub(crate) struct DeviceCurveRefresh(bool);

#[derive(Component, Clone, Default)]
pub(crate) struct DeviceCurveStatus(pub(crate) DeviceCurve);

impl DeviceCurve {
    /// The series this chart draws, in order. A two-series chart shares ONE
    /// scale: both directions are projected against the same ceiling.
    pub(crate) fn series(&self, shell: &ShellApp) -> Vec<Vec<f32>> {
        let history = &shell.history;
        match self.kind {
            DeviceCurveKind::DiskThroughput => vec![
                history.disk_read_bytes_per_sec_for(&self.id, self.generation.get()),
                history.disk_write_bytes_per_sec_for(&self.id, self.generation.get()),
            ],
            DeviceCurveKind::NetworkThroughput => vec![
                history.network_rx_bytes_per_sec_for(&self.id, self.generation.get()),
                history.network_tx_bytes_per_sec_for(&self.id, self.generation.get()),
            ],
            DeviceCurveKind::DiskActive => {
                vec![history.disk_active_time_pct_for(&self.id, self.generation.get())]
            }
            DeviceCurveKind::BatteryCharge => vec![history.battery_capacity_pct_for(&self.id)],
            DeviceCurveKind::BatteryPower => vec![history.battery_power_w_for(&self.id)],
            DeviceCurveKind::FanRpm => vec![history.fan_rpm_for(&self.id)],
            DeviceCurveKind::GpuMetric => {
                let metric = shell.gpu_chart_metric_selected();
                vec![gpu_chart_metric_history(
                    history,
                    &self.id,
                    self.generation.get(),
                    metric,
                )]
            }
        }
    }

    /// The primary series' window — the chart's own headline trend.
    pub(crate) fn samples(&self, shell: &ShellApp) -> Vec<f32> {
        self.series(shell).into_iter().next().unwrap_or_default()
    }

    /// The ceiling every series of this chart projects against.
    pub(crate) fn ceiling(&self, series: &[Vec<f32>], shell: &ShellApp) -> f32 {
        match self.kind {
            DeviceCurveKind::DiskActive | DeviceCurveKind::BatteryCharge => 100.0,
            DeviceCurveKind::GpuMetric
                if matches!(
                    shell.gpu_chart_metric_selected().unit(),
                    GpuChartMetricUnit::Percent
                ) =>
            {
                100.0
            }
            _ => {
                series
                    .iter()
                    .flat_map(|values| values.iter().copied())
                    .filter(|value| value.is_finite())
                    .fold(1.0_f32, f32::max)
                    * 1.1
            }
        }
    }

    /// Whether the battery is taking on charge: the charge trend rises.
    pub(crate) fn charging(&self, series: &[Vec<f32>]) -> bool {
        if self.kind != DeviceCurveKind::BatteryCharge {
            return false;
        }
        let Some(values) = series.first() else {
            return false;
        };
        let finite: Vec<f32> = values
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .collect();
        finite.len() >= 2 && finite[finite.len() - 1] > finite[finite.len() - 2]
    }
}

/// The legend entry of one series: its ink plate and its label.
fn legend_entry_scene(plan: &DeviceSeriesPlan, palette: &UiPalette) -> Box<dyn Scene> {
    let label = plan.label.clone();
    Box::new(bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_2()),
        }
        Children [

                Node {
                    width: rem(palette.control_height_rem * 0.4),
                    height: rem(palette.control_height_rem * 0.4),
                    border_radius: BorderRadius::all(Val::Px(space_2())),
                }
                BackgroundColor({ plan.color }) --
                Text(label) TextRole(Role::Caption)
        ]
    })
}

/// One series' canvas: the curve, its wash, and (for the first series) the
/// grid the curve sits on.
fn series_canvas_scene(
    plan: &DeviceSeriesPlan,
    first: bool,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let style = ChartStyle {
        line: plan.color,
        thickness: crate::widgets::chart::geometry::CHART_LINE_THICKNESS_PX,
        fill: first.then_some(CanvasFill {
            color: plan.color,
            alpha: CANVAS_FILL_ALPHA,
        }),
        grid: first.then_some(CanvasGrid {
            color: palette.dim_color,
            alpha: 0.13,
            horizontal: GRID_DIVISIONS,
            vertical: GRID_DIVISIONS + 1,
            reference: GRID_REFERENCE,
            reference_alpha: 0.26,
        }),
        flow: None,
        format: Some(plan.format),
    };
    let content = ChartContent::Empty;
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            overflow: Overflow::clip(),
        }
        ChartCanvas { content: {content}, style: {style} }
        ChartPointer
        ChartPaintState
    })
}

/// The chart scene: the legend, the canvas stack, and the readout.
pub(super) fn scene(
    kind: DeviceCurveKind,
    id: &str,
    generation: DeviceGeneration,
    shell: &ShellApp,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let plans = kind.plans(shell, palette);
    let label = t(kind.label_key()).to_owned();
    let id = id.to_owned();
    let ink = plans
        .first()
        .map(|plan| plan.color)
        .unwrap_or(palette.accent);
    let status = DeviceCurve {
        kind,
        id: id.clone(),
        generation,
        color: ink,
    };
    // The legend names each series of a two-series chart; a single-series
    // chart already carries its own label.
    let legend_display = if plans.len() > 1 {
        Display::Flex
    } else {
        Display::None
    };
    let legend: Vec<Box<dyn Scene>> = plans
        .iter()
        .map(|plan| legend_entry_scene(plan, palette))
        .collect();
    let canvases: Vec<Box<dyn Scene>> = plans
        .iter()
        .enumerate()
        .map(|(index, plan)| series_canvas_scene(plan, index == 0, palette))
        .collect();
    bsn! {
        Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: Val::Px(space_4()) }
        Children [
            Text(label) TextRole(Role::Caption) --
            Node { width: percent(100), flex_direction: FlexDirection::Row, column_gap: Val::Px(space_8()), display: { legend_display } }
            Children [ { legend } ] --
            Node { width: percent(100), height: rem(PERFORMANCE_CHART_SLOT_HEIGHT_REM), flex_shrink: 0.0, overflow: Overflow::clip() }
            DeviceCurve { kind: {kind}, id: {id}, generation: {generation}, color: {ink} } CurveMeasurement(None)
            Children [ { canvases } ] --
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
        let series = curve.series(track.shell());
        let ceiling = curve.ceiling(&series, track.shell());
        let charging = curve.charging(&series);
        let Ok(children) = access.children.get(entity) else {
            continue;
        };
        for (index, child) in children.iter().enumerate() {
            let Some(values) = series.get(index) else {
                continue;
            };
            let Ok(mut canvas) = access.canvases.get_mut(*child) else {
                continue;
            };
            canvas.content = ChartContent::Samples {
                values: values.clone(),
                ceiling,
            };
            if curve.kind == DeviceCurveKind::BatteryCharge {
                canvas.style.flow = charging.then(|| CanvasFlow {
                    color: secondary_ink(curve.color),
                    phase: 0.0,
                    radius: FLOW_RADIUS_PX,
                });
            }
        }
        measurement.0 = Some((size.x, size.y));
        painted = true;
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
                    let format = status.0.kind.readout(track.shell());
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
    crate::widgets::chart::register(app);
    // The curve card's hover pill (the sampling instant) and the NPU card's
    // power section live in the scene layer; their seams register here so
    // the page owns one composition entry.
    super::scene::hover::register(app);
    super::scene::npu_power::register(app);
    storage::register(app);
}

#[cfg(test)]
#[path = "../../../tests/headless/pages/device_curves_parity.rs"]
mod parity_tests;
