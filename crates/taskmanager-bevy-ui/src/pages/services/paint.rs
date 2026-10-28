//! Coalesced services painting over explicit resource and entity access.
use super::*;
use crate::scene_attach::attach_child;
use crate::widgets::scene_paint::ScenePaint;
use bevy::app::{App, PostUpdate};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{NonSend, SystemParam};
use bevy::prelude::Visibility;
use bevy::text::EditableText;
use bevy::ui::UiSystems;

use crate::input::{ServicesSearchPlaceholder, placeholder_visibility, sync_search_editor};

#[derive(Resource, Default)]
pub(super) struct PaintState {
    dirty: bool,
}
#[derive(Event)]
pub(super) struct RepaintRequested;
pub(super) fn on_repaint_requested(_event: On<RepaintRequested>, mut state: ResMut<PaintState>) {
    state.dirty = true;
}
pub(super) fn register(app: &mut App) {
    app.add_systems(
        PostUpdate,
        paint.in_set(ScenePaint).before(UiSystems::Prepare),
    );
}
#[derive(SystemParam)]
struct PaintAccess<'w, 's> {
    track: NonSend<'w, FrontendTrack>,
    palette: Res<'w, WindowPalette>,
    selection: ResMut<'w, ServiceSelection>,
    filter: Res<'w, crate::pages::services::unit_filter::ServiceUnitFilter>,
    rendered: ResMut<'w, ServicesRenderState>,
    state: ResMut<'w, PaintState>,
    bodies: Query<'w, 's, (Entity, Option<&'static Children>), With<ServicesBody>>,
    status: Query<'w, 's, &'static mut Text, With<ServicesStatusLine>>,
    search: Query<
        'w,
        's,
        &'static mut EditableText,
        (
            With<ServicesSearchInput>,
            bevy::ecs::query::Without<ServicesStatusLine>,
        ),
    >,
    placeholder: Query<'w, 's, &'static mut Visibility, With<ServicesSearchPlaceholder>>,
    commands: Commands<'w, 's>,
}
fn paint(mut access: PaintAccess) {
    if !access.state.dirty {
        return;
    }
    let Some((body, children)) = access.bodies.iter().next() else {
        return;
    };
    access.state.dirty = false;
    let shell = &access.track.shell;
    let rows = service_rows_for(shell, *access.filter);
    if let Some(target) = &access.selection.target
        && !rows.iter().any(|row| &row.target == target)
    {
        access.selection.target = None;
    }
    let scene = services_body_scene(shell, &access.palette.inner, &access.selection, &rows);
    let line = status_line_text(shell, rows.len());
    access.rendered.rendered_revision = Some(shell.projection().services_revision);
    if let Some(children) = children {
        for child in children.iter() {
            access.commands.entity(*child).despawn();
        }
    }
    let fresh = access.commands.spawn_scene(scene).id();
    attach_child(&mut access.commands, body, fresh);
    for mut text in &mut access.status {
        text.0 = line.clone();
    }
    for mut editor in &mut access.search {
        sync_search_editor(&mut editor, &shell.services_query);
    }
    let placeholder = placeholder_visibility(&shell.services_query);
    for mut visibility in &mut access.placeholder {
        if *visibility != placeholder {
            *visibility = placeholder;
        }
    }
}
