//! Persisted Performance review over the shared history projection and commands.

use crate::pages::history::control::{
    HistoryCommand, PerformanceHistoryChanged, PerformanceHistoryProjectionResource,
    PerformancePresentation,
};
use crate::palette::{UiPalette, space_8};
use crate::widgets::chart::{MAX_CHART_POINTS, line_segments, polyline_scene};
use crate::widgets::history_controls::{action_scene, toolbar_scene};
use crate::widgets::scene_paint::ScenePaint;
use crate::window::{Role, TextRole, WindowPalette};
use bevy::app::{App, PostUpdate};
use bevy::ecs::{
    component::Component,
    entity::Entity,
    hierarchy::{ChildOf, Children},
    lifecycle::Add,
    observer::On,
    query::With,
    resource::Resource,
    schedule::IntoScheduleConfigs,
    system::{Commands, Query, Res, ResMut},
};
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::widget::Text;
use bevy::ui::{
    UiSystems,
    prelude::{
        BackgroundColor, ComputedNode, Display, FlexDirection, Node, Overflow, UiRect, percent, px,
    },
};
use bevy::ui_widgets::ScrollArea;
use std::sync::Arc;
use taskmanager_application::{
    ApplicationHistoryStatus, PerformanceHistoryProjection,
    history_decimation::gap_preserving_envelope, i18n::t,
};
use taskmanager_shell::presentation::{history_replay::row_heading, missing_value};

#[derive(Component, Clone, Default)]
pub(crate) struct PerformanceLiveBody;
#[derive(Component, Clone, Default)]
pub(crate) struct PerformanceReplayRoot;
#[derive(Component, Clone, Default)]
pub(crate) struct PerformanceHistoryEntry;
#[derive(Component, Clone, Default)]
pub(crate) struct ReplayChart(Arc<[f32]>);
#[derive(Component, Clone, Default)]
pub(crate) struct ChartSize(Option<(f32, f32)>);
#[derive(Resource, Default)]
struct PaintDirty(bool);
#[derive(Component, Clone, Default)]
pub(crate) struct PerformanceReplayBody;

pub(crate) fn register(app: &mut App) {
    app.init_resource::<PaintDirty>()
        .add_observer(changed)
        .add_observer(mark_mounted)
        .add_systems(
            PostUpdate,
            paint.in_set(ScenePaint).before(UiSystems::Prepare),
        )
        .add_systems(
            PostUpdate,
            (
                visibility.in_set(ScenePaint).before(UiSystems::Prepare),
                paint_charts.after(UiSystems::Layout),
            ),
        );
}
fn mark_mounted(_event: On<Add<PerformanceReplayRoot>>, mut dirty: ResMut<PaintDirty>) {
    dirty.0 = true;
}

fn changed(_event: On<PerformanceHistoryChanged>, mut dirty: ResMut<PaintDirty>) {
    dirty.0 = true;
}
pub(crate) fn entry_scene(palette: &UiPalette) -> impl Scene + use<> {
    let action = action_scene(
        t("perf.replay.toggle").to_owned(),
        HistoryCommand::OpenPerformance,
        palette,
    );
    bsn! { Node { display: Display::None, flex_shrink: 0.0 } PerformanceHistoryEntry Children [ @{ action } ] }
}
fn review_scene(model: &PerformanceHistoryProjection, palette: &UiPalette) -> impl Scene + use<> {
    let controls = toolbar_scene(model.selected_window, true, palette);
    let mut rows: Vec<Box<dyn Scene>> = Vec::new();
    if model.refreshing {
        rows.push(Box::new(
            bsn! { Text(t("history.application.refreshing")) TextRole(Role::Caption) },
        ));
    }
    if let Some(failure) = &model.failure {
        rows.push(Box::new(
            bsn! { Text({ failure.kind().stable_code() }) TextRole(Role::Caption) },
        ));
    }
    if model.rows.is_empty() {
        rows.push(Box::new(
            bsn! { Text(t("perf.replay.empty")) TextRole(Role::Body) },
        ));
    }
    for row in model.rows.iter() {
        let peak = row
            .peak_value
            .filter(|value| value.is_finite())
            .map_or_else(missing_value, |value| format!("{value:.1}"));
        let heading = format!(
            "{} · {} {}",
            row_heading(&row.key),
            t("perf.replay.peak"),
            peak
        );
        let summary = format!(
            "{} {} · {} {} · {} {}",
            t("perf.replay.observed"),
            row.observed,
            t("perf.replay.gaps"),
            row.gaps,
            t("perf.replay.clock_jumps"),
            row.clock_jumps
        );
        let samples: Arc<[f32]> =
            gap_preserving_envelope(&row.gap_aware_samples(), MAX_CHART_POINTS).into();
        rows.push(Box::new(bsn! { Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: px(space_8()), padding: UiRect::all(px(space_8())) } BackgroundColor({ palette.panel_fill }) Children [ Text(heading) TextRole(Role::Body) TextLayout { linebreak: LineBreak::AnyCharacter } -- Text(summary) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::AnyCharacter } -- Node { width: percent(100), height: px(96.0), flex_shrink: 0.0, overflow: Overflow::clip() } ReplayChart(samples) ChartSize(None) ] }));
    }
    bsn! { Node { width: percent(100), height: percent(100), min_height: px(0.0), flex_direction: FlexDirection::Column, row_gap: px(space_8()), padding: UiRect::all(px(space_8())) } Children [ Text(t("perf.replay.title")) TextRole(Role::Heading) -- @{ controls } -- Node { width: percent(100), min_height: px(0.0), flex_grow: 1.0, flex_basis: px(0.0), flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), row_gap: px(space_8()) } ScrollArea PerformanceReplayBody Children [ { rows } ] ] }
}
type ReplayVisibilityQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Node,
        Option<&'static PerformanceLiveBody>,
        Option<&'static PerformanceReplayRoot>,
        Option<&'static PerformanceHistoryEntry>,
    ),
>;
fn visibility(
    presentation: Res<PerformancePresentation>,
    model: Res<PerformanceHistoryProjectionResource>,
    mut nodes: ReplayVisibilityQuery,
) {
    let replay = *presentation == PerformancePresentation::Replay;
    let available = matches!(
        model.0.status,
        ApplicationHistoryStatus::Ready | ApplicationHistoryStatus::Collecting
    );
    for (mut node, live, review, entry) in &mut nodes {
        if live.is_some() {
            node.display = if replay { Display::None } else { Display::Flex };
        } else if review.is_some() {
            node.display = if replay { Display::Flex } else { Display::None };
        } else if entry.is_some() {
            node.display = if available && !replay {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
}
fn paint(
    mut dirty: ResMut<PaintDirty>,
    model: Res<PerformanceHistoryProjectionResource>,
    palette: Res<WindowPalette>,
    roots: Query<(Entity, Option<&Children>), With<PerformanceReplayRoot>>,
    mut commands: Commands,
) {
    if !dirty.0 || roots.is_empty() {
        return;
    }
    dirty.0 = false;
    for (root, children) in &roots {
        if let Some(children) = children {
            for child in children.iter() {
                commands.entity(*child).despawn();
            }
        }
        let entity = commands
            .spawn_scene(review_scene(&model.0, &palette.inner))
            .id();
        commands.entity(root).add_one_related::<ChildOf>(entity);
    }
}
pub(crate) fn paint_charts(
    mut charts: Query<(
        Entity,
        &ComputedNode,
        &ReplayChart,
        &mut ChartSize,
        Option<&Children>,
    )>,
    palette: Res<WindowPalette>,
    mut commands: Commands,
) {
    for (entity, node, chart, mut last, children) in &mut charts {
        let size = node.size() * node.inverse_scale_factor();
        if size.x <= 0.0 || size.y <= 0.0 || last.0 == Some((size.x, size.y)) {
            continue;
        }
        if let Some(children) = children {
            for child in children.iter() {
                commands.entity(*child).despawn();
            }
        }
        let segments = line_segments(&chart.0, size.x, size.y, MAX_CHART_POINTS);
        let child = commands
            .spawn_scene(polyline_scene(&segments, palette.inner.accent))
            .id();
        commands.entity(entity).add_one_related::<ChildOf>(child);
        last.0 = Some((size.x, size.y));
    }
}

pub(crate) fn charts_presented(
    charts: &Query<(&ReplayChart, &ChartSize, Option<&Children>)>,
) -> bool {
    let mut count = 0;
    for (_, size, children) in charts.iter() {
        count += 1;
        if size.0.is_none() || children.is_none_or(|children| children.is_empty()) {
            return false;
        }
    }
    count > 0
}
