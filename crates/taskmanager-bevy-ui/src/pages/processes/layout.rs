//! Measured Applications slots own table capacity; optional chrome yields as whole groups.
use super::centered_scroll_top;
use super::{
    ProcessOptionalChrome, ProcessPageRoot, ProcessRowsRoot, ProcessScrollState, TableSurface,
    rebuild_table,
};
use crate::app::FrontendTrack;
use crate::palette::{space_4, space_8};
use crate::widgets::table::rows_in_viewport;
use crate::window::WindowPalette;
use bevy::app::{App, PostUpdate};
use bevy::ecs::query::{With, Without};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, NonSend, Query, Res};
use bevy::ui::{ComputedNode, Display, Node, UiSystems, px};
type ChromeNodes<'w, 's> =
    Query<'w, 's, &'static mut Node, (With<ProcessOptionalChrome>, Without<ProcessPageRoot>)>;
fn sync_chrome(
    mut pages: Query<(&ComputedNode, &mut Node), With<ProcessPageRoot>>,
    mut chrome: ChromeNodes,
    palette: Res<WindowPalette>,
) {
    let Ok((computed, mut page)) = pages.single_mut() else {
        return;
    };
    let compact =
        computed.size().y * computed.inverse_scale_factor() < palette.inner.control_height_px * 8.0;
    let gap = px(if compact { space_4() } else { space_8() });
    if page.row_gap != gap {
        page.row_gap = gap;
    }
    let display = if compact {
        Display::None
    } else {
        Display::Flex
    };
    for mut node in &mut chrome {
        if node.display != display {
            node.display = display;
        }
    }
}
fn sync_rows(
    track: Option<NonSend<FrontendTrack>>,
    geometry: Query<&ComputedNode, With<ProcessRowsRoot>>,
    mut surface: TableSurface,
    mut commands: Commands,
) {
    let Some(track) = track else {
        return;
    };
    let Ok(node) = geometry.single() else {
        return;
    };
    let capacity = rows_in_viewport(
        (node.size().y * node.inverse_scale_factor() - space_8()).max(0.0),
        surface.palette.inner.control_height_px,
    );
    if capacity == surface.scroll.viewport_rows {
        return;
    }
    let Ok(root) = surface.roots.single() else {
        return;
    };
    surface.scroll.viewport_rows = capacity;
    surface.scroll.top = centered_scroll_top(
        track.shell.visible_processes().len(),
        capacity,
        track.shell.selected,
    );
    rebuild_table(&mut commands, root, &track.shell, &mut surface);
}
pub(crate) fn register(app: &mut App) {
    app.init_resource::<ProcessScrollState>().add_systems(
        PostUpdate,
        (sync_chrome, sync_rows).after(UiSystems::Layout),
    );
}
