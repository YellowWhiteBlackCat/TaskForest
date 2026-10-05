//! Coalesced sessions painting over explicit resource and entity access.
use super::*;
use crate::widgets::scene_paint::ScenePaint;
use bevy::app::{App, PostUpdate};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{NonSend, SystemParam};
use bevy::ui::UiSystems;

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
    selection: ResMut<'w, SessionSelection>,
    rendered: ResMut<'w, SessionsRenderState>,
    state: ResMut<'w, PaintState>,
    bodies: Query<'w, 's, (Entity, Option<&'static Children>), With<SessionsBody>>,
    status: Query<'w, 's, &'static mut Text, With<SessionsStatusLine>>,
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
    let rows = session_rows(shell);
    if let Some(target) = &access.selection.target
        && !rows.iter().any(|row| &row.target == target)
    {
        access.selection.target = None;
    }
    let scene = sessions_body_scene(shell, &access.palette.inner, &access.selection);
    let line = status_line_text(shell, rows.len());
    access.rendered.rendered_revision = Some(shell.projection().sessions_revision);
    if let Some(children) = children {
        for child in children.iter() {
            access.commands.entity(*child).despawn();
        }
    }
    let fresh = access.commands.spawn_scene(scene).id();
    access
        .commands
        .entity(body)
        .add_one_related::<ChildOf>(fresh);
    for mut text in &mut access.status {
        text.0 = line.clone();
    }
}
