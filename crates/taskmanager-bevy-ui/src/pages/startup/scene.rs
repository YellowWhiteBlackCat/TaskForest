//! Inventory scene composition and table cell adapters.

use super::*;

pub(super) fn boot_timeline_scene(
    evidence: Option<&StartupBootEvidenceSnapshot>,
    palette: &UiPalette,
) -> Option<Box<dyn Scene>> {
    let evidence = evidence?;
    if evidence.critical_chain.is_empty() {
        return None;
    }
    let max_duration = evidence
        .critical_chain
        .iter()
        .filter_map(|node| node.duration_ms)
        .max()
        .unwrap_or(1)
        .max(1) as f32;

    let bars: Vec<Box<dyn Scene>> = evidence
        .critical_chain
        .iter()
        .take(6)
        .filter_map(|node| {
            let ms = node.duration_ms?;
            let pct = (ms as f32 / max_duration * 100.0).clamp(5.0, 100.0);
            let label = node.unit.clone();
            let duration_text = format!("{ms} ms");
            let is_slow = ms > 1000;
            let bar_color = if is_slow {
                palette.nav_active_bg
            } else {
                palette.accent
            };
            Some(Box::new(bsn! {
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_8()),
                    padding: UiRect::vertical(Val::Px(space_2())),
                }
                Children [
                    (
                        Node {
                            width: px(160.0),
                            overflow: Overflow::clip_x(),
                        }
                        Children [
                            ( Text(label) TextRole(Role::Caption) template_value(no_wrap_text()) )
                        ]
                    ),
                    (
                        Node {
                            width: px(200.0),
                            height: px(8.0),
                            border_radius: BorderRadius::all(Val::Px(space_2())),
                            overflow: Overflow::clip_x(),
                        }
                        BackgroundColor({ palette.panel_fill })
                        Children [
                            (
                                Node {
                                    width: percent(pct),
                                    height: percent(100.0),
                                    border_radius: BorderRadius::all(Val::Px(space_2())),
                                }
                                BackgroundColor(bar_color)
                            )
                        ]
                    ),
                    (
                        Text(duration_text)
                        TextRole(Role::Mono)
                        template_value(no_wrap_text())
                    ),
                ]
            }) as Box<dyn Scene>)
        })
        .collect();

    if bars.is_empty() {
        return None;
    }

    Some(Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.content_bg })
        Children [
            ( Text({ t("startup.timeline").to_owned() }) TextRole(Role::Caption) ),
            { bars },
        ]
    }))
}

pub(super) fn startup_toolbar_scene(has_selection: bool, palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
            ( Node { flex_grow: 1.0 } ),
            (
                Node {
                    height: px(palette.control_height_px),
                    padding: UiRect::horizontal(Val::Px(space_12())),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                }
                BackgroundColor({
                    if has_selection { palette.nav_active_bg } else { palette.content_bg }
                })
                ControlVisual(ControlTone::Surface, has_selection)
                Button
                on(on_startup_enable_button_activated)
                StartupEnableButton
                Children [
                    (
                        Text({ t("startup.enable").to_owned() })
                        TextRole(Role::Caption)
                        template_value(no_wrap_text())
                        Pickable::IGNORE
                    )
                ]
            ),
            (
                Node {
                    height: px(palette.control_height_px),
                    padding: UiRect::horizontal(Val::Px(space_12())),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                }
                BackgroundColor({
                    if has_selection { palette.nav_active_bg } else { palette.content_bg }
                })
                ControlVisual(ControlTone::Surface, has_selection)
                Button
                on(on_startup_disable_button_activated)
                StartupDisableButton
                Children [
                    (
                        Text({ t("startup.disable").to_owned() })
                        TextRole(Role::Caption)
                        template_value(no_wrap_text())
                        Pickable::IGNORE
                    )
                ]
            ),
        ]
    })
}

pub(super) fn startup_body_scene(
    shell: &ShellApp,
    palette: &UiPalette,
    selection: &StartupSelection,
) -> impl Scene + use<> {
    let rows = startup_rows(shell);
    let selected = selected_row(&rows, selection);
    let sources = shell.projection().startup_source.as_deref();
    let notice = source_notice_text(sources);
    let empty = empty_state_text(sources);
    let evidence = evidence_line(shell);
    let mut children = body_children(&rows, selected, notice, evidence, empty, palette);
    if let Some(timeline) =
        boot_timeline_scene(shell.projection().startup_boot_evidence.as_ref(), palette)
    {
        children.insert(0, timeline);
    }
    let toolbar = startup_toolbar_scene(selection.target.is_some(), palette);
    let header = header_scene(shell.startup_sort, palette);
    bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            ( toolbar ),
            ( header ),
            { children },
        ]
    }
}

pub(super) fn body_children(
    rows: &[StartupRowModel],
    selected: Option<usize>,
    notice: Option<String>,
    evidence: Option<String>,
    empty: String,
    palette: &UiPalette,
) -> Vec<Box<dyn Scene>> {
    let mut children = Vec::new();
    if let Some(text) = notice {
        children.push(Box::new(caption_line_scene(text)) as Box<dyn Scene>);
    }
    if let Some(text) = evidence {
        children.push(Box::new(caption_line_scene(text)) as Box<dyn Scene>);
    }
    if rows.is_empty() {
        children.push(Box::new(empty_scene(empty)) as Box<dyn Scene>);
    } else {
        for (index, row) in rows.iter().enumerate() {
            children.push(startup_row_scene(
                row,
                index,
                selected == Some(index),
                palette,
            ));
        }
    }
    children
}

/// Header row: one caption cell per column; every cell carries the
/// [`StartupSortHeader`] identity for the pointer adapter.
pub(super) fn header_scene(
    sort: Option<(InfoSortCol, SortDir)>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let cells: Vec<Box<dyn Scene>> = columns()
        .into_iter()
        .map(|column| {
            let label = header_label(&column);
            let direction = sorted_direction(&column, sort);
            let indicator = sort_indicator_scene(direction, palette);
            let width = column.width_px;
            let sort_target = column.sort;
            Box::new(bsn! {
                Node {
                    width: px(width),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_4()),
                    overflow: Overflow::clip_x(),
                }
                StartupSortHeader(sort_target)
                Children [
                    ( Text(label) TextRole(Role::Caption) template_value(no_wrap_text()) ),
                    { indicator },
                ]
            }) as Box<dyn Scene>
        })
        .collect();
    bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(space_8()),
            padding: UiRect::horizontal(Val::Px(space_8())),
        }
        Children [
            { cells }
        ]
    }
}

pub(super) fn startup_row_scene(
    row: &StartupRowModel,
    index: usize,
    selected: bool,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let widths = columns();
    let name = row.name.clone();
    let state = if row.enabled {
        t("common.enabled")
    } else {
        t("common.disabled")
    }
    .to_owned();
    let impact = row.impact.clone();
    let source = row.source.clone();
    let exec = row.exec.clone();
    let fill = if selected {
        palette.nav_active_bg
    } else {
        Color::NONE
    };
    let chip = chip_fill(enabled_chip(row.enabled), palette);
    let target = row.target.clone();
    let target_toggle = row.target.clone();
    let height = palette.control_height_px;
    let radius = palette.control_radius_px;
    let state_width = widths[0].width_px;
    let name_width = widths[1].width_px;
    let impact_width = widths[2].width_px;
    let source_width = widths[3].width_px;
    let exec_width = widths[4].width_px;
    let chip_scene = chip_cell_scene(state, state_width, chip, palette);
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: px(height),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::horizontal(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor(fill)
        StartupRowMarker(index, target)
        Button
        on(on_startup_row_activated)
        Children [
            (
                Node { width: px(state_width), align_items: AlignItems::Center }
                Button
                StartupToggleButton(index, target_toggle)
                on(on_startup_toggle_button_activated)
                Children [
                    ( { chip_scene } ),
                ]
            ),
            ( text_cell_scene(name, name_width, Role::Body) ),
            ( text_cell_scene(impact, impact_width, Role::Body) ),
            ( text_cell_scene(source, source_width, Role::Body) ),
            ( text_cell_scene(exec, exec_width, Role::Body) ),
        ]
    })
}

pub(super) fn text_cell_scene(text: String, width: f32, role: Role) -> impl Scene + use<> {
    bsn! {
        Node {
            width: px(width),
            align_items: AlignItems::FlexStart,
            overflow: Overflow::clip_x(),
        }
        Children [
            ( Text(text) TextRole(role) template_value(no_wrap_text()) ),
        ]
    }
}

pub(super) fn chip_cell_scene(
    word: String,
    width: f32,
    fill: Color,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let radius = palette.control_radius_px;
    bsn! {
        Node { width: px(width), align_items: AlignItems::Center }
        Pickable::IGNORE
        Children [
            (
                Node {
                    height: Val::Auto,
                    padding: UiRect::horizontal(Val::Px(space_8())),
                    border_radius: BorderRadius::all(Val::Px(radius)),
                }
                BackgroundColor(fill)
                Pickable::IGNORE
                Children [
                    ( Text(word) TextRole(Role::Caption) template_value(no_wrap_text()) Pickable::IGNORE ),
                ]
            ),
        ]
    }
}

pub(super) fn caption_line_scene(text: String) -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100) }
        Children [
            ( Text(text) TextRole(Role::Caption) ),
        ]
    }
}

pub(super) fn empty_scene(message: String) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            flex_grow: 1.0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: UiRect::all(Val::Px(space_24())),
        }
        Children [
            ( Text(message) TextRole(Role::Body) ),
        ]
    }
}
