//! Bevy scene construction and observer painting for application history.

use bevy::scene::WorldSceneExt;

use super::*;
use crate::window::WindowPalette;

// ---- Bevy 0.19 scene adapter ----

#[derive(Resource)]
struct HistoryPageBound;

/// Build the route-ready page scene from one immutable application
/// projection. Mainline route registration supplies the projection resource;
/// this function never reaches into app-host or the process projection.
/// The History page shell: title, status line, and the EMPTY body container.
/// The body's only author is [`paint_history`] (bound by the root's
/// on-insert hook) — a static initial body here would race the paint pass
/// into a doubled surface, so the container starts empty by contract.
pub(crate) fn content(
    projection: &ApplicationHistoryProjection,
    _palette: &UiPalette,
) -> impl Scene + use<> {
    let model = HistoryPageModel::from_projection(projection);
    let title = format!(
        "{} · {}",
        t("history.application.title"),
        window_label(model.selected_window)
    );
    let line = summary_text(&model);
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_8())),
        }
        HistoryPageRoot
        Children [
            (
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                }
                Children [
                    ( Text(title) TextRole(Role::Heading) ),
                    ( Node { flex_grow: 1.0 } ),
                    ( Text({ window_label(model.selected_window).to_owned() }) TextRole(Role::Caption) ),
                ]
            ),
            ( Text(line) HistoryStatusLine TextRole(Role::Caption) ),
            (
                Node {
                    width: percent(100),
                    height: Val::Auto,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                }
                HistoryBody
                Children []
            ),
        ]
    }
}

fn history_body_scene(model: &HistoryPageModel, palette: &UiPalette) -> impl Scene + use<> {
    let mut children: Vec<Box<dyn Scene>> = Vec::new();
    if model.has_visible_rows() {
        if model.notice.stale || model.notice.error_code.is_some() {
            children.push(Box::new(history_notice_scene(model, palette)));
        }
        children.push(Box::new(history_header_scene()));
        children.extend(model.rows.iter().map(|row| history_row_scene(row, palette)));
    } else {
        children.push(Box::new(history_empty_scene(model)));
    }
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            { children },
        ]
    }
}

fn history_notice_scene(model: &HistoryPageModel, palette: &UiPalette) -> Box<dyn Scene> {
    let mut detail = if model.notice.stale {
        t("history.application.refreshing").to_owned()
    } else {
        String::new()
    };
    if let Some(code) = model.notice.error_code {
        if !detail.is_empty() {
            detail.push_str(" · ");
        }
        detail.push_str("error=");
        detail.push_str(code);
    }
    Box::new(bsn! {
        Node {
            width: percent(100),
            padding: UiRect::all(Val::Px(space_8())),
        }
        BackgroundColor({ palette.nav_active_bg })
        Children [
            ( Text(detail) TextRole(Role::Caption) ),
        ]
    })
}

fn history_empty_scene(model: &HistoryPageModel) -> Box<dyn Scene> {
    let (heading, detail) = status_copy(model.status);
    let mut detail = detail.to_owned();
    if let Some(code) = model.notice.error_code {
        detail.push_str(" (error=");
        detail.push_str(code);
        detail.push(')');
    }
    if let Some(code) = model.notice.unavailable_code {
        detail.push_str(" (unavailable=");
        detail.push_str(code);
        detail.push(')');
    }
    Box::new(bsn! {
        Node {
            width: percent(100),
            padding: UiRect::all(Val::Px(space_24())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            ( Text({ heading.to_owned() }) TextRole(Role::Body) ),
            ( Text(detail) TextRole(Role::Caption) ),
        ]
    })
}

fn history_header_scene() -> Box<dyn Scene> {
    let labels = [
        t("common.name"),
        t("history.application.peak_cpu"),
        t("history.application.peak_memory"),
        t("history.application.peak_processes"),
        t("proc.trend"),
    ];
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(space_8()),
        }
        Children [
            ( Text({ labels[0].to_owned() }) TextRole(Role::Caption) ),
            ( Node { width: px(90.0) } Children [( Text({ labels[1].to_owned() }) TextRole(Role::Caption) )] ),
            ( Node { width: px(110.0) } Children [( Text({ labels[2].to_owned() }) TextRole(Role::Caption) )] ),
            ( Node { width: px(110.0) } Children [( Text({ labels[3].to_owned() }) TextRole(Role::Caption) )] ),
            ( Node { width: px(150.0) } Children [( Text({ labels[4].to_owned() }) TextRole(Role::Caption) )] ),
        ]
    })
}

fn history_row_scene(row: &HistoryRowModel, palette: &UiPalette) -> Box<dyn Scene> {
    let name = format!("{} · {}", row.display_name, row_annotation(row));
    let chart = row.cpu.as_ref().map_or_else(
        || empty_trend_scene(),
        |metric| trend_scene(metric, palette),
    );
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: px(palette.control_height_px),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
            ( Text(name) TextRole(Role::Body) ),
            ( Node { width: px(90.0) } Children [( Text({ scalar_text(row.cpu_peak(), "%") }) TextRole(Role::Body) )] ),
            ( Node { width: px(110.0) } Children [( Text({ memory_text(row.memory_peak()) }) TextRole(Role::Body) )] ),
            ( Node { width: px(110.0) } Children [( Text({ process_count_text(row.process_count_peak()) }) TextRole(Role::Body) )] ),
            ( Node { width: px(150.0) } Children [( { chart } )] ),
        ]
    })
}

fn empty_trend_scene() -> Box<dyn Scene> {
    Box::new(bsn! {
        Node { width: percent(100), height: px(20.0) }
        Children [( Text({ missing_value() }) TextRole(Role::Caption) )]
    })
}

/// Render finite samples as bars and leave gap samples as layout slots with
/// no fill. The missing slot is deliberate: a downtime gap never becomes a
/// zero-height measurement or a connected false trend.
fn trend_scene(metric: &HistoryMetricView, palette: &UiPalette) -> Box<dyn Scene> {
    let finite = metric
        .samples
        .iter()
        .copied()
        .filter(|sample| sample.is_finite())
        .collect::<Vec<_>>();
    if metric.finite_sample_count() < 2 {
        return empty_trend_scene();
    }
    let min = finite.iter().copied().fold(f32::INFINITY, f32::min);
    let max = finite.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let range = max - min;
    let bars: Vec<Box<dyn Scene>> = metric
        .samples
        .iter()
        .copied()
        .map(|sample| {
            let height = if !sample.is_finite() {
                1.0
            } else if range > 0.0 {
                (((sample - min) / range).clamp(0.0, 1.0) * 20.0).max(1.0)
            } else {
                10.0
            };
            if sample.is_finite() {
                Box::new(bsn! {
                    Node { width: px(space_2()), height: px(height) }
                    BackgroundColor({ palette.accent })
                }) as Box<dyn Scene>
            } else {
                Box::new(bsn! {
                    Node { width: px(space_2()), height: px(1.0) }
                }) as Box<dyn Scene>
            }
        })
        .collect();
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: px(20.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexEnd,
            column_gap: Val::Px(space_2()),
        }
        Children [{ bars }]
    })
}

// ---- observer lifecycle ----

pub(super) fn bind_history_page(mut world: DeferredWorld<'_>, _context: HookContext) {
    if world.get_resource_mut::<HistoryPageBound>().is_some() {
        return;
    }
    let mut commands = world.commands();
    commands.insert_resource(HistoryPageBound);
    commands.add_observer(on_history_body_added);
    commands.add_observer(on_history_changed);
}

fn on_history_body_added(_added: On<Add, HistoryBody>, mut commands: Commands) {
    commands.queue(paint_history);
}

fn on_history_changed(_changed: On<ApplicationHistoryChanged>, mut commands: Commands) {
    commands.queue(paint_history);
}

fn paint_history(world: &mut World) {
    let projection = world.resource::<HistoryProjectionResource>().0.clone();
    let palette = world.resource::<WindowPalette>().inner.clone();
    let model = HistoryPageModel::from_projection(&projection);
    let scene = history_body_scene(&model, &palette);
    let mut body_query = world.query_filtered::<(Entity, Option<&Children>), With<HistoryBody>>();
    let Some((body, children)) = body_query.iter(world).next() else {
        return;
    };
    let stale: Vec<Entity> = children
        .map(|children| children.iter().copied().collect())
        .unwrap_or_default();
    // Synchronous World mutation, not queued commands: a same-frame second
    // paint (bind hook + Add can both fire in one flush) must observe the
    // previous paint's result, or each stale pass spawns a duplicate body.
    for entity in stale {
        let _ = world.despawn(entity);
    }
    let fresh = world
        .spawn_scene(scene)
        .expect("the repainted history body resolves without assets")
        .id();
    world.entity_mut(body).add_one_related::<ChildOf>(fresh);
    let mut lines = world.query_filtered::<&mut Text, With<HistoryStatusLine>>();
    if let Ok(mut line) = lines.single_mut(world) {
        line.0 = summary_text(&model);
    }
}
