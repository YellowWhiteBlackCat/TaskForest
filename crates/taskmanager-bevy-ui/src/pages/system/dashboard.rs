//! System dashboard controls and native measured curves over the shared read model.

use super::SystemBody;
use super::paint::SystemPaint;
use crate::icons::icon_scene;
use crate::widgets::layout::SystemDashboardBudget;
use crate::{
    palette::{UiPalette, space_4, space_8},
    widgets::{
        chart::{CurveMeasurement, CurvePaintAccess, paint_curve_at_size},
        controls::{ControlTone, ControlVisual},
    },
    window::{Role, TextRole},
};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ui::widget::Text;
use bevy::{
    app::{App, PostUpdate},
    ecs::{
        component::Component,
        entity::Entity,
        hierarchy::Children,
        observer::On,
        query::With,
        resource::Resource,
        system::{Query, ResMut},
    },
    scene::{Scene, bsn, on},
    text::{LineBreak, TextLayout},
    ui::{
        UiSystems,
        prelude::{
            BackgroundColor, BorderRadius, ComputedNode, FlexDirection, FlexWrap, Node, Overflow,
            ScrollPosition, UiRect, percent, px,
        },
    },
    ui_widgets::{Activate, Button},
};
use std::sync::Arc;
use taskmanager_application::{
    i18n::t,
    system_timeline::{
        SystemHistoryWindow, SystemPageSection, TimelineMetric, TimelineSeries, TimelineStatistic,
    },
};
use taskmanager_shell::presentation::health_review::HealthReviewSection;
use taskmanager_shell::presentation::system_timeline::{coverage, readout};
use taskmanager_ui_contract::IconId;

#[derive(Resource)]
pub(crate) struct SystemDashboardState {
    pub(crate) section: SystemPageSection,
    pub(crate) health_section: HealthReviewSection,
    pub(crate) window: SystemHistoryWindow,
    pub(crate) first_metric: usize,
    layout: Option<(f32, f32)>,
}
impl Default for SystemDashboardState {
    fn default() -> Self {
        Self {
            section: SystemPageSection::Hardware,
            health_section: HealthReviewSection::All,
            window: SystemHistoryWindow::FifteenMinutes,
            first_metric: 0,
            layout: None,
        }
    }
}
#[derive(Component, Clone, Default)]
pub(crate) struct SystemDashboardToolbar;
#[derive(Clone, Copy)]
pub(crate) enum DashboardControl {
    Section(SystemPageSection),
    HealthSection(HealthReviewSection),
    Window(SystemHistoryWindow),
    Previous,
    Next,
}
impl Default for DashboardControl {
    fn default() -> Self {
        Self::Section(SystemPageSection::Dashboard)
    }
}
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct DashboardControlButton(pub(crate) DashboardControl);
#[derive(Component, Clone, Default)]
pub(crate) struct SystemDashboardCurve {
    pub(crate) metric: TimelineMetric,
    samples: Arc<[f32]>,
    ceiling: f32,
    color: bevy::color::Color,
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<SystemDashboardState>()
        .add_systems(PostUpdate, (resize, paint_charts).after(UiSystems::Layout));
}
fn activate(
    event: On<Activate>,
    controls: Query<&DashboardControlButton>,
    mut state: ResMut<SystemDashboardState>,
    mut bodies: Query<&mut ScrollPosition, With<SystemBody>>,
    mut paint: ResMut<SystemPaint>,
) {
    let Ok(control) = controls.get(event.entity) else {
        return;
    };
    match control.0 {
        DashboardControl::HealthSection(section) => {
            state.section = SystemPageSection::Health;
            state.health_section = section;
        }
        DashboardControl::Section(section) => {
            state.section = section;
            state.first_metric = 0;
        }
        DashboardControl::Window(window) => {
            state.window = window;
            state.first_metric = 0;
        }
        DashboardControl::Previous => state.first_metric = state.first_metric.saturating_sub(1),
        DashboardControl::Next => state.first_metric = state.first_metric.saturating_add(1).min(3),
    }
    state.layout = None;
    for mut scroll in &mut bodies {
        scroll.0.y = 0.0;
    }
    paint.dirty = true;
}
pub(crate) fn button(
    label: String,
    action: DashboardControl,
    active: bool,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let height = palette.control_height_px;
    bsn! {
        Node { min_height: px(height), padding: UiRect::horizontal(px(space_8())), flex_shrink: 0.0 }
        Button DashboardControlButton(action) ControlVisual(ControlTone::Surface, active) on(activate)
        Children [ Text(label) TextRole(Role::Caption) ]
    }
}

pub(crate) fn toolbar(state: &SystemDashboardState, palette: &UiPalette) -> impl Scene + use<> {
    let mut controls: Vec<Box<dyn Scene>> = Vec::new();
    if state.section == SystemPageSection::Dashboard {
        for window in SystemHistoryWindow::ALL {
            controls.push(Box::new(button(
                window.label().into(),
                DashboardControl::Window(window),
                window == state.window,
                palette,
            )));
        }
        for (icon, action) in [
            (IconId::NavigateUp, DashboardControl::Previous),
            (IconId::NavigateDown, DashboardControl::Next),
        ] {
            let image = icon_scene(icon, 16.0, palette.body_color);
            let height = palette.control_height_px;
            controls.push(Box::new(bsn! { Node { width: px(height), min_height: px(height), flex_shrink: 0.0, padding: UiRect::all(px(space_8())) } Button DashboardControlButton(action) ControlVisual(ControlTone::Surface, false) on(activate) Children [ @{image} ] }));
        }
        controls.push(Box::new(button(
            t("dashboard.hardware").into(),
            DashboardControl::Section(SystemPageSection::Hardware),
            false,
            palette,
        )));
    }
    if state.section == SystemPageSection::Health {
        for section in HealthReviewSection::ALL {
            let label = match section {
                HealthReviewSection::All => t("health.system_health_alerts"),
                HealthReviewSection::Storage => t("health.storage"),
                HealthReviewSection::Sensors => t("health.sensors"),
            };
            controls.push(Box::new(button(
                label.into(),
                DashboardControl::HealthSection(section),
                state.health_section == section,
                palette,
            )));
        }
    }
    bsn! { Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(space_4()), row_gap: px(space_4()) } Children [{ controls }] }
}

pub(crate) fn body(
    series: &TimelineSeries,
    state: &SystemDashboardState,
    budget: SystemDashboardBudget,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let cards = TimelineMetric::ALL.into_iter().skip(state.first_metric).take(budget.count).map(|metric| {
        let samples = series.samples(metric);
        let ceiling = match metric { TimelineMetric::Cpu | TimelineMetric::Memory => 100.0, _ => samples.iter().copied().filter(|v| v.is_finite()).fold(1.0_f32, f32::max) };
        let color = match metric { TimelineMetric::Cpu => palette.cpu, TimelineMetric::Memory => palette.memory, TimelineMetric::Disk => palette.disk, TimelineMetric::Network => palette.network };
        let stats = format!("{} {} · {} {}", t("dashboard.latest"), readout(series, metric, TimelineStatistic::Latest), t("dashboard.peak"), readout(series, metric, TimelineStatistic::Peak));
        let mut parts: Vec<Box<dyn Scene>> = vec![Box::new(bsn! { Text({t(metric.label_key()).to_owned()}) TextRole(Role::Body) })];
        parts.push(Box::new(bsn! { Text(stats) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::WordBoundary } Node { width: percent(100), min_width: px(0.0) } }));
        if budget.chart_height > 0.0 {
            parts.push(Box::new(bsn! { Node { width: percent(100), height: px(budget.chart_height), flex_shrink: 0.0, overflow: Overflow::clip() } SystemDashboardCurve { metric, samples, ceiling, color } CurveMeasurement::default() }));
        }
        parts.push(Box::new(bsn! { Text({coverage(series, metric)}) TextRole(Role::Caption) }));
        Box::new(bsn! {
            Node { flex_basis: px(260.0), flex_grow: 1.0, min_width: px(0.0), max_width: percent(100), height: px(budget.card_height), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: px(space_4()), padding: UiRect::all(px(space_8())), border_radius: BorderRadius::all(px(palette.panel_radius_px)) }
            BackgroundColor({palette.panel_fill})
            Children [{parts}]
        }) as Box<dyn Scene>
    }).collect::<Vec<_>>();
    let notice: Vec<Box<dyn Scene>> = if budget.count == 0 {
        vec![Box::new(
            bsn! { Text({t("dashboard.resize").to_owned()}) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::WordBoundary } Node { width: percent(100), min_width: px(0.0) } },
        )]
    } else {
        Vec::new()
    };
    bsn! { Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(space_8()), row_gap: px(space_8()) } Children [{cards} -- {notice}] }
}
fn resize(
    mut state: ResMut<SystemDashboardState>,
    bodies: Query<&ComputedNode, With<SystemBody>>,
    mut paint: ResMut<SystemPaint>,
) {
    if state.section != SystemPageSection::Dashboard {
        return;
    }
    let size = bodies
        .iter()
        .next()
        .map(|node| node.size() * node.inverse_scale_factor());
    let Some(size) = size.filter(|size| size.x > 0.0 && size.y > 0.0) else {
        return;
    };
    if state.layout != Some((size.x, size.y)) {
        state.layout = Some((size.x, size.y));
        paint.dirty = true;
    }
}
pub(crate) fn paint_charts(
    mut charts: Query<(
        Entity,
        &ComputedNode,
        &SystemDashboardCurve,
        &mut CurveMeasurement,
    )>,
    mut access: CurvePaintAccess,
) {
    for (entity, node, curve, mut last) in &mut charts {
        let size = node.size() * node.inverse_scale_factor();
        if size.x > 0.0
            && size.y > 0.0
            && last.0 != Some((size.x, size.y))
            && paint_curve_at_size(
                &mut access,
                entity,
                size,
                &curve.samples,
                curve.ceiling,
                curve.color,
            )
        {
            last.0 = Some((size.x, size.y));
        }
    }
}

pub(crate) fn presented(
    charts: &Query<(&SystemDashboardCurve, &CurveMeasurement, &ComputedNode)>,
) -> bool {
    let mut count = 0;
    let mut seen = Vec::new();
    for (curve, measured, node) in charts.iter() {
        count += 1;
        if seen.contains(&curve.metric) {
            return false;
        }
        seen.push(curve.metric);
        if measured.0.is_none()
            || node.size().y <= 0.0
            || curve
                .samples
                .iter()
                .filter(|value| value.is_finite())
                .count()
                < 2
        {
            return false;
        }
    }
    count > 0
}
