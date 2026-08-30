//! Processes-page input bridges: pointer selection, wheel scroll, and the
//! re-render path for shell-side mutations.
//!
//! The keyboard arrives through [`crate::input`] (the shell routers own the
//! semantics). This module closes the two remaining physical channels —
//! pointer picking and wheel scrolling — by converting them into the page's
//! existing typed seams, and renders the shell mutations the keyboard made
//! (selection moves, search typing) through one
//! [`ShellInteractionApplied`] observer. No page system polls input.

use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::observer::{Observer, On};
use bevy::ecs::query::{Or, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::picking::hover::PickingInteraction;
use bevy::ui::widget::Text;
use bevy::ui_widgets::Activate;

use super::{
    ProcessCountLine, ProcessRowLink, ProcessRowsRoot, ProcessScrollIntent, ProcessSearchInput,
    ProcessSelectRow, ProcessSelectionChanged, ProcessTableArtifact, TableSurface, rebuild_table,
    selected_identity,
};
use crate::app::{Page, Route, ShellTrack};
use crate::input::ShellInteractionApplied;
use crate::window::WindowPalette;
use taskmanager_core::core::process::ProcessLiveKey;

/// Last published selection identity, so keyboard-driven shell mutations
/// publish the details seam only on a real identity change.
#[derive(Default, Resource)]
pub(crate) struct ProcessSelectionMemory(pub(crate) Option<ProcessLiveKey>);

/// Row pointer activation: the visible-set index rides the row wrapper, and
/// the shell's `select_row` reducer owns bounds (a stale index is rejected,
/// never clamped onto a neighbor). Entity-scoped to the row wrapper by its
/// scene's `on(...)`.
pub(crate) fn on_row_activated(
    activate: On<Activate>,
    links: Query<&ProcessRowLink>,
    roots: Query<Entity, With<ProcessRowsRoot>>,
    mut commands: Commands,
) {
    let Ok(root) = roots.single() else {
        return;
    };
    let Ok(link) = links.get(activate.event().entity) else {
        return;
    };
    commands.trigger(ProcessSelectRow {
        entity: root,
        row: link.0,
    });
}

/// Wheel rows for one scroll event: lines map one-to-one, pixels divide by
/// the row height. Pure; headless-tested.
#[must_use]
pub(crate) fn wheel_rows(delta_y: f32, unit: MouseScrollUnit, row_height_px: f32) -> isize {
    let lines = match unit {
        MouseScrollUnit::Line => delta_y,
        MouseScrollUnit::Pixel => {
            if row_height_px <= 0.0 {
                return 0;
            }
            delta_y / row_height_px
        }
    };
    // Truncate toward zero: a wheel notch is a row, never a rounding surprise.
    lines as isize
}

/// Hover state of any table surface node (rows root or a rendered artifact).
type HoveredTableSurfaces<'w, 's> = Query<
    'w,
    's,
    &'static PickingInteraction,
    Or<(With<ProcessTableArtifact>, With<ProcessRowsRoot>)>,
>;

/// Wheel scroll into the rows root while the pointer hovers the table. A
/// polling `Update` system (the resource is per-frame input state, not an
/// event); every other page-scope guard makes it a no-op elsewhere. The sign
/// is inverted on purpose: wheel up moves the window toward earlier rows,
/// matching every scroll surface in the product.
pub(crate) fn scroll_intent_system(
    scroll_input: Res<AccumulatedMouseScroll>,
    hover_interactions: HoveredTableSurfaces<'_, '_>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<ProcessRowsRoot>>,
    mut commands: Commands,
) {
    if scroll_input.delta.y == 0.0 {
        return;
    }
    let any_hovered = hover_interactions
        .iter()
        .any(|interaction| *interaction == PickingInteraction::Hovered);
    if !any_hovered {
        return;
    }
    let Some(palette) = palette else {
        return;
    };
    let Some(root) = roots.iter().next() else {
        return;
    };
    let rows = -wheel_rows(
        scroll_input.delta.y,
        scroll_input.unit,
        palette.inner.control_height_px,
    );
    if rows == 0 {
        return;
    }
    commands.trigger(ProcessScrollIntent { entity: root, rows });
}

/// Render-side sync for keyboard-driven shell mutations: the shell already
/// moved (cursor, query, page state); rebuild the window from it, keep the
/// search box honest, and publish the selection seam on a real change.
fn sync_after_shell_interaction(
    _applied: On<ShellInteractionApplied>,
    track: ShellTrack,
    route: Res<Route>,
    mut surface: TableSurface,
    mut memory: ResMut<ProcessSelectionMemory>,
    // Disjoint from the count line's Text write in `TableSurface` — a node
    // is never both the search box and the count line.
    mut search: Query<&mut Text, (With<ProcessSearchInput>, Without<ProcessCountLine>)>,
    mut commands: Commands,
) {
    if route.page != Page::Processes {
        return;
    }
    let Ok(root) = surface.roots.single() else {
        return;
    };
    let shell = track.shell();
    if let Ok(mut text) = search.single_mut() {
        text.0 = shell.query.clone();
    }
    rebuild_table(&mut commands, root, shell, &mut surface);
    let identity = selected_identity(shell);
    if memory.0 != identity {
        memory.0 = identity;
        commands.trigger(ProcessSelectionChanged(identity));
    }
}

/// Mount the input bridges: selection memory plus the re-render observer,
/// lifetime-bound to the rows root like the fold observer. The scroll system
/// is a plain `Update` system registered by the app shell plugin. Called by
/// the page bootstrap.
/// Mount the input bridges: selection memory plus the re-render observer,
/// lifetime-bound to the rows root like the fold observer. The scroll system
/// is a plain `Update` system registered by the app shell plugin. Called by
/// the page bootstrap with the rows-root entity.
pub(crate) fn bootstrap(root: Entity, commands: &mut Commands) {
    commands.insert_resource(ProcessSelectionMemory::default());
    let sync_observer = commands
        .spawn(Observer::new(sync_after_shell_interaction))
        .id();
    commands
        .entity(root)
        .add_one_related::<ChildOf>(sync_observer);
}
