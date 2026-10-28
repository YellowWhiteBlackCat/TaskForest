//! Inventory scene composition and table cell adapters.

use super::*;
use bevy::text::{LineBreak, TextLayout};
use taskmanager_shell::SearchSlot;

use crate::input::{SearchEditor, ServicesSearchPlaceholder};

pub(crate) fn on_services_search_input_activated(
    _activate: On<Activate>,
    mut track: NonSendMut<FrontendTrack>,
    mut text_state: Option<ResMut<crate::input::TextInputState>>,
    mut commands: Commands,
) {
    track.shell.open_search_for(SearchSlot::Services);
    if let Some(ref mut state) = text_state {
        state.edit.set_caret_at_end(&track.shell.services_query);
    }
    commands.trigger(crate::input::ShellInteractionApplied);
}

pub(super) fn services_search_input_scene(palette: &UiPalette, query: &str) -> impl Scene + use<> {
    let text = query.to_owned();
    let placeholder = t("search.services").to_owned();
    let ink = palette.nav_active_ink;
    let placeholder_ink = palette.dim_color;
    let placeholder_visible = query.is_empty();
    let width = space_24() * 10.0;
    let radius = palette.control_radius_px;
    bsn! {
        Node {
            width: px(width),
            height: rem(palette.control_height_rem),
            align_items: AlignItems::Center,
            padding: Val::Px(space_8()),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({ palette.panel_fill })
        ServicesSearchInput
        SearchEditor({ text })
        TextInput
        TextFont { font_size: FontSize::Rem(1.0) }
        TextColor(ink)
        TextRole(Role::Body)
        TextLayout { linebreak: LineBreak::NoWrap }
        Button
        on(on_services_search_input_activated)
        Children [
             Text(placeholder) TextRole(Role::Body) TextColor(placeholder_ink)
             Node { position_type: PositionType::Absolute, left: px(space_8()) }
             Pickable::IGNORE
             ServicesSearchPlaceholder({ placeholder_visible })
        ]
    }
}

/// The table body: header row plus (notice, rows or empty state). Rebuilt as
/// one scene by [`paint_services`].
pub(super) fn services_body_scene(
    shell: &ShellApp,
    palette: &UiPalette,
    selection: &ServiceSelection,
    rows: &[ServiceRowModel],
) -> impl Scene + use<> {
    let selected = selected_row(rows, selection);
    let notice = source_notice_text(shell.projection().services_source.as_deref());
    let empty = empty_state_text(shell.projection().services_source.as_deref());
    let children = body_children(
        rows,
        selected,
        notice,
        empty,
        palette,
        &shell.services_query,
    );
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
             @toolbar --
             @header --
            { children }
        ]
    }
}

pub(super) fn body_children(
    rows: &[ServiceRowModel],
    selected: Option<usize>,
    notice: Option<String>,
    empty: String,
    palette: &UiPalette,
    query: &str,
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
                query,
            ));
        }
    }
    children
}

/// The content of one action capsule: the semantic mark and the label, in
/// the reference surface's `[mark][label]` order.
fn capsule_content(
    label: String,
    glyph: Option<action_glyph::ServiceActionGlyph>,
    enabled: bool,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let mark: Vec<Box<dyn Scene>> = match glyph {
        Some(glyph) => vec![action_glyph::action_glyph_scene(glyph, palette, enabled)],
        None => Vec::new(),
    };
    Box::new(bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
        }
        Pickable::IGNORE
        Children [
             { mark }--
             Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE
        ]
    })
}

/// Header row: one caption cell per column; every cell carries the
/// [`ServicesSortHeader`] identity (`Some` on sortable columns) for the
/// pointer adapter.
pub(super) fn services_toolbar_scene(has_selection: bool, palette: &UiPalette) -> Box<dyn Scene> {
    let enabled = has_selection;
    let start = capsule_content(
        t("svc.start").to_owned(),
        Some(action_glyph::ServiceActionGlyph::Start),
        enabled,
        palette,
    );
    let stop = capsule_content(
        t("svc.stop").to_owned(),
        Some(action_glyph::ServiceActionGlyph::Stop),
        enabled,
        palette,
    );
    let restart = capsule_content(
        t("svc.restart").to_owned(),
        Some(action_glyph::ServiceActionGlyph::Restart),
        enabled,
        palette,
    );
    Box::new(bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
             Node { flex_grow: 1.0 } --

                Node {
                    height: rem(palette.control_height_rem),
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
                     @{ start }
                ]
            --

                Node {
                    height: rem(palette.control_height_rem),
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
                     @{ stop }
                ]
            --

                Node {
                    height: rem(palette.control_height_rem),
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
                     @{ restart }
                ]
            --

                Node {
                    height: rem(palette.control_height_rem),
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

                        Text({ t("common.details").to_owned() })
                        TextRole(Role::Caption)
                        TextLayout { linebreak: LineBreak::NoWrap }
                        Pickable::IGNORE

                ]
            --

                Node {
                    height: rem(palette.control_height_rem),
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

                        Text({ t("svc.dependencies").to_owned() })
                        TextRole(Role::Caption)
                        TextLayout { linebreak: LineBreak::NoWrap }
                        Pickable::IGNORE

                ]
            --

                Node {
                    height: rem(palette.control_height_rem),
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

                        Text({ t("svc.logs").to_owned() })
                        TextRole(Role::Caption)
                        TextLayout { linebreak: LineBreak::NoWrap }
                        Pickable::IGNORE

                ]

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
                         Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE --
                        { indicator }
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
                         Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } --
                        { indicator }
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
    query: &str,
) -> Box<dyn Scene> {
    let widths = columns();
    let name = row.name.clone();
    let description = row.description.clone();
    let fill = if selected {
        palette.nav_active_bg
    } else {
        palette.content_bg
    };
    let chip = chip_fill(service_chip(row.status), palette);
    let target = row.target.clone();
    let radius = palette.control_radius_px;
    let name_width = widths[0].width_px;
    let status_width = widths[1].width_px;
    let description_width = widths[2].width_px;
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: rem(palette.control_height_rem),
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
             @service_name_cell_scene(name, name_width, row.cycle, palette, query) --
             @chip_cell_scene(row.status, status_width, chip, palette) --
             @text_cell_scene(description, description_width, Role::Body)
        ]
    })
}

pub(super) fn service_name_cell_scene(
    name: String,
    width: f32,
    cycle: bool,
    palette: &UiPalette,
    query: &str,
) -> impl Scene + use<> {
    let alert = if cycle {
        vec![crate::icons::icon_scene(
            IconId::Alert,
            palette,
            palette.warning_color,
        )]
    } else {
        Vec::new()
    };
    let ranges = crate::widgets::table::highlight_ranges("Name", &name, query);
    let mut flow: Vec<Box<dyn Scene>> = Vec::new();
    for (segment, is_match) in crate::widgets::table::highlight_segments(&name, &ranges) {
        let background = if is_match {
            palette.selection_bg
        } else {
            Color::NONE
        };
        if is_match {
            flow.push(Box::new(bsn! {
                TextSpan({ segment })
                TextRole(Role::Body)
                TextBackgroundColor({ background })
                TextColor({ palette.highlight_fg })
                CustomTextInk
            }) as Box<dyn Scene>);
        } else {
            flow.push(Box::new(bsn! {
                TextSpan({ segment })
                TextRole(Role::Body)
                TextBackgroundColor({ background })
            }) as Box<dyn Scene>);
        }
    }
    let root_text = if flow.is_empty() { name } else { String::new() };
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
            { alert }--

                Text(root_text)
                TextRole(Role::Body)
                TextLayout { linebreak: LineBreak::NoWrap }
                Pickable::IGNORE
                Children [ { flow } ]
        ]
    }
}

pub(super) fn text_cell_scene(text: String, width: f32, role: Role) -> impl Scene + use<> {
    bsn! {
        Node { width: px(width), align_items: AlignItems::FlexStart }
        Pickable::IGNORE
        Children [
             Text(text) TextRole(role) TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE
        ]
    }
}

/// The status cell: the breathing status dot and the status word, in one
/// bounded line.
pub(super) fn chip_cell_scene(
    status: ServiceStatus,
    width: f32,
    fill: Color,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let radius = palette.control_radius_px;
    let word = status.as_str().to_owned();
    let dot = status_dot::status_dot_scene(service_chip(status), palette);
    bsn! {
        Node {
            width: px(width),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
        }
        Pickable::IGNORE
        Children [
             @{ dot }--

                Node {
                    height: Val::Auto,
                    padding: UiRect::horizontal(Val::Px(space_8())),
                    border_radius: BorderRadius::all(Val::Px(radius)),
                }
                BackgroundColor(fill)
                Pickable::IGNORE
                Children [
                     Text(word) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } Pickable::IGNORE
                ]

        ]
    }
}

pub(super) fn caption_line_scene(text: String) -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100) }
        Children [
             Text(text) TextRole(Role::Caption)
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
             Text(message) TextRole(Role::Body)
        ]
    }
}
