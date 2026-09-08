//! Inventory scene composition and table cell adapters.

use super::*;

pub(super) fn sessions_toolbar_scene(has_selection: bool, palette: &UiPalette) -> Box<dyn Scene> {
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
                on(on_session_disconnect_button_activated)
                SessionDisconnectButton
                Children [
                    (
                        Text({ t("users.disconnect").to_owned() })
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
                on(on_session_lock_button_activated)
                SessionLockButton
                Children [
                    (
                        Text({ t("users.lock").to_owned() })
                        TextRole(Role::Caption)
                        template_value(no_wrap_text())
                        Pickable::IGNORE
                    )
                ]
            ),
        ]
    })
}

pub(super) fn sessions_body_scene(
    shell: &ShellApp,
    palette: &UiPalette,
    selection: &SessionSelection,
) -> impl Scene + use<> {
    let rows = session_rows(shell);
    let selected = selected_row(&rows, selection);
    let sources = shell.projection().sessions_source.as_deref();
    let notice = source_notice_text(sources);
    let empty = empty_state_text(sources);
    let feedback = shell
        .projection()
        .session_control_feedback
        .as_ref()
        .map(feedback_line_text);
    let children = body_children(&rows, selected, notice, feedback, empty, palette);
    let header = header_scene(shell.sessions_sort, palette);
    let toolbar = sessions_toolbar_scene(selection.target.is_some(), palette);
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
    rows: &[SessionRowModel],
    selected: Option<usize>,
    notice: Option<String>,
    feedback: Option<String>,
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
            children.push(session_row_scene(
                row,
                index,
                selected == Some(index),
                palette,
            ));
        }
    }
    if let Some(text) = feedback {
        children.push(Box::new(caption_line_scene(text)) as Box<dyn Scene>);
    }
    children
}

/// Header row: one caption cell per column; every cell carries the
/// [`SessionsSortHeader`] identity for the pointer adapter.
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
                SessionsSortHeader(sort_target)
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

pub(super) fn session_row_scene(
    row: &SessionRowModel,
    index: usize,
    selected: bool,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let widths = columns();
    let session = row.session.clone();
    let user = row.user.clone();
    let seat = row.seat.clone();
    let tty = row.tty.clone();
    let kind = row.kind.to_owned();
    let since = row.since.clone();
    let fill = if selected {
        palette.nav_active_bg
    } else {
        Color::NONE
    };
    let target = row.target.clone();
    let height = palette.control_height_px;
    let radius = palette.control_radius_px;
    let session_width = widths[0].width_px;
    let user_width = widths[1].width_px;
    let seat_width = widths[2].width_px;
    let tty_width = widths[3].width_px;
    let kind_width = widths[4].width_px;
    let since_width = widths[5].width_px;
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
        SessionsRowMarker(index, target)
        Button
        on(on_sessions_row_activated)
        Children [
            ( text_cell_scene(session, session_width, Role::Body) ),
            ( text_cell_scene(user, user_width, Role::Body) ),
            ( text_cell_scene(seat, seat_width, Role::Body) ),
            ( text_cell_scene(tty, tty_width, Role::Body) ),
            ( text_cell_scene(kind, kind_width, Role::Body) ),
            ( text_cell_scene(since, since_width, Role::Body) ),
        ]
    })
}

pub(super) fn text_cell_scene(text: String, width: f32, role: Role) -> impl Scene + use<> {
    bsn! {
        Node { width: px(width), align_items: AlignItems::FlexStart }
        Children [
            ( Text(text) TextRole(role) ),
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
