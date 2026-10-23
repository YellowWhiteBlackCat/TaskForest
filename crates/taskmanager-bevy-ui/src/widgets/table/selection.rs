//! Cell selection: the selected value and the clipboard export.
//!
//! A table value is text the user may want to take away. The table therefore
//! offers two pointer paths, exactly like the reference surfaces:
//!
//! - a DOUBLE CLICK selects one cell: the selection owns the cell's complete
//!   value (never a painted fragment) and the cell shows the selection
//!   surface, so the selection is visible;
//! - a SECONDARY CLICK on a row copies that row's summary through the
//!   shared clipboard port, which writes the system clipboard and reports the
//!   result.

use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::observer::On;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::picking::events::PointerClick;
use bevy::ui::prelude::BackgroundColor;

use crate::text_selection::TextSelectionState;
use crate::window::WindowPalette;

/// Marker on a table cell: the cell's identity and its complete value. The
/// selection owns this value, so a copied selection is never the painted
/// fragment of a truncated cell.
#[derive(Component, Clone, Debug, Default)]
pub(crate) struct CellSelection {
    pub(crate) id: String,
    pub(crate) content: String,
}

/// The double click that selects a cell.
///
/// Bevy's picking counts consecutive clicks, so the second click of a double
/// click carries the count — the first click of a pair is an ordinary
/// selection gesture and must not start a text selection.
pub(crate) fn on_cell_double_clicked(
    click: On<PointerClick>,
    cells: Query<&CellSelection>,
    parents: Query<&ChildOf>,
    mut selection: ResMut<TextSelectionState>,
) {
    if click.count < 2 {
        return;
    }
    let cell_entity = if cells.contains(click.entity) {
        click.entity
    } else {
        parents
            .get(click.entity)
            .map_or(click.entity, |parent| parent.parent())
    };
    let Ok(cell) = cells.get(cell_entity) else {
        return;
    };
    selection.select_all(cell_entity, cell.id.clone(), cell.content.clone());
}

/// Paint the selection surface on the selected cell.
///
/// The selection is visible: the selected cell carries the theme's selection
/// tint, every other cell stays unpainted.
pub(crate) fn paint_cell_selection(
    state: Option<Res<TextSelectionState>>,
    palette: Res<WindowPalette>,
    mut cells: Query<(Entity, &CellSelection, &mut BackgroundColor)>,
) {
    let target = state
        .as_deref()
        .and_then(|state| state.session.as_ref())
        .map(|session| session.target);
    for (entity, _cell, mut background) in &mut cells {
        let selected = target == Some(entity);
        let wanted = if selected {
            palette.inner.selection_bg
        } else {
            Color::NONE
        };
        if background.0 != wanted {
            background.0 = wanted;
        }
    }
}
