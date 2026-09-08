//! Inventory scene composition and table cell adapters.

use super::*;

pub(super) fn services_search_input_scene(palette: &UiPalette, query: &str) -> impl Scene + use<> {
    let text = if query.is_empty() {
        t("search.services").to_owned()
    } else {
        query.to_owned()
    };
    let ink = if query.is_empty() {
        palette.dim_color
    } else {
        palette.nav_active_ink
    };
    let width = space_24() * 10.0;
    let height = palette.control_height_px;
    let radius = palette.control_radius_px;
    bsn! {
        Node {
            width: px(width),
            height: px(height),
            align_items: AlignItems::Center,
            padding: Val::Px(space_8()),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({ palette.panel_fill })
        ServicesSearchInput
        Children [
            ( Text(text) TextRole(Role::Body) TextColor(ink) template_value(no_wrap_text()) ),
        ]
    }
}

/// The table body: header row plus (notice, rows or empty state). Rebuilt as
/// one scene by [`paint_services`].
pub(super) fn services_body_scene(
    shell: &ShellApp,
    palette: &UiPalette,
    selection: &ServiceSelection,
) -> impl Scene + use<> {
    let rows = service_rows(shell);
    let selected = selected_row(&rows, selection);
    let notice = source_notice_text(shell.projection().services_source.as_deref());
    let empty = empty_state_text(shell.projection().services_source.as_deref());
    let children = body_children(&rows, selected, notice, empty, palette);
    let header = header_scene(shell.services_sort, palette);
    let toolbar = services_toolbar_scene(selection.target.is_some(), palette);
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
    rows: &[ServiceRowModel],
    selected: Option<usize>,
    notice: Option<String>,
    empty: String,
    palette: &UiPalette,
) -> Vec<Box<dyn Scene>> {
    let mut children = Vec::new();
    if let Some(text) = notice {
        children.push(Box::new(caption_line_scene(text)) as Box<dyn Scene>);
    }
    if rows.is_empty() {
        children.push(Box::new(empty_scene(empty)) as Box<dyn Scene>);
    } else {
        for (index, row) in rows.iter().enumerate() {
            children.push(service_row_scene(
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
/// [`ServicesSortHeader`] identity (`Some` on sortable columns) for the
/// pointer adapter.
pub(super) fn services_toolbar_scene(has_selection: bool, palette: &UiPalette) -> Box<dyn Scene> {
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
                on(on_service_start_button_activated)
                ServiceStartButton
                Children [
                    (
                        Text({ t("svc.start").to_owned() })
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
                on(on_service_stop_button_activated)
                ServiceStopButton
                Children [
                    (
                        Text({ t("svc.stop").to_owned() })
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
                on(on_service_restart_button_activated)
                ServiceRestartButton
                Children [
                    (
                        Text({ t("svc.restart").to_owned() })
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
                on(details_modal::on_details_button_activated)
                details_modal::ServiceDetailsOpenButton
                Children [
                    (
                        Text({ t("common.details").to_owned() })
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
                on(dependencies_panel::services_dependencies_button_activated)
                dependencies_panel::ServicesDependenciesOpenButton
                Children [
                    (
                        Text({ t("svc.dependencies").to_owned() })
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
                on(log_panel::services_logs_button_activated)
                log_panel::ServicesLogsOpenButton
                Children [
                    (
                        Text({ t("svc.logs").to_owned() })
                        TextRole(Role::Caption)
                        template_value(no_wrap_text())
                        Pickable::IGNORE
                    )
                ]
            ),
        ]
    })
}

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
            if sort_target.is_some() {
                Box::new(bsn! {
                    Node {
                        width: px(width),
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(space_4()),
                        overflow: Overflow::clip_x(),
                    }
                    ServicesSortHeader(sort_target)
                    Button
                    on(on_services_sort_header_activated)
                    Children [
                        ( Text(label) TextRole(Role::Caption) template_value(no_wrap_text()) Pickable::IGNORE ),
                        { indicator },
                    ]
                }) as Box<dyn Scene>
            } else {
                Box::new(bsn! {
                    Node {
                        width: px(width),
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(space_4()),
                        overflow: Overflow::clip_x(),
                    }
                    ServicesSortHeader(sort_target)
                    Children [
                        ( Text(label) TextRole(Role::Caption) template_value(no_wrap_text()) ),
                        { indicator },
                    ]
                }) as Box<dyn Scene>
            }
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

pub(super) fn service_row_scene(
    row: &ServiceRowModel,
    index: usize,
    selected: bool,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let widths = columns();
    let name = row.name.clone();
    let description = row.description.clone();
    let status = row.status.as_str().to_owned();
    let fill = if selected {
        palette.nav_active_bg
    } else {
        palette.content_bg
    };
    let chip = chip_fill(service_chip(row.status), palette);
    let target = row.target.clone();
    let height = palette.control_height_px;
    let radius = palette.control_radius_px;
    let name_width = widths[0].width_px;
    let status_width = widths[1].width_px;
    let description_width = widths[2].width_px;
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
        ServicesRowMarker(index, target)
        ControlVisual(ControlTone::Surface, selected)
        Button
        on(on_services_row_activated)
        Children [
            ( service_name_cell_scene(name, name_width, row.cycle, palette) ),
            ( chip_cell_scene(status, status_width, chip, palette) ),
            ( text_cell_scene(description, description_width, Role::Body) ),
        ]
    })
}

pub(super) fn service_name_cell_scene(
    name: String,
    width: f32,
    cycle: bool,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let alert = if cycle {
        vec![crate::icons::icon_scene(
            IconId::Alert,
            12.0,
            palette.warning_color,
        )]
    } else {
        Vec::new()
    };
    bsn! {
        Node {
            width: px(width),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_2()),
            overflow: Overflow::clip_x(),
        }
        Pickable::IGNORE
        Children [
            { alert },
            (
                Text(name)
                TextRole(Role::Body)
                template_value(no_wrap_text())
                Pickable::IGNORE
            ),
        ]
    }
}

pub(super) fn text_cell_scene(text: String, width: f32, role: Role) -> impl Scene + use<> {
    bsn! {
        Node { width: px(width), align_items: AlignItems::FlexStart }
        Pickable::IGNORE
        Children [
            ( Text(text) TextRole(role) template_value(no_wrap_text()) Pickable::IGNORE ),
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
