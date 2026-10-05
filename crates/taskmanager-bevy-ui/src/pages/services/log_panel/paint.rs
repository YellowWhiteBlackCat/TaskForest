//! Coalesced panel publication using the shell projection and owned slots.
use super::*;
use crate::widgets::scene_paint::ScenePaint;
use bevy::app::{App, PostUpdate};
use bevy::ecs::hierarchy::Children;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{NonSend, SystemParam};
use bevy::ui::UiSystems;
#[derive(Resource, Default)]
pub(crate) struct PaintState(pub(super) bool);
pub(super) fn register(app: &mut App) {
    app.init_resource::<PaintState>().add_systems(
        PostUpdate,
        paint.in_set(ScenePaint).before(UiSystems::Prepare),
    );
}
#[derive(SystemParam)]
struct PaintAccess<'w, 's> {
    track: NonSend<'w, FrontendTrack>,
    palette: Res<'w, WindowPalette>,
    rendered: ResMut<'w, ServicesLogRenderState>,
    dirty: ResMut<'w, PaintState>,
    slots: Query<'w, 's, (Entity, Option<&'static Children>), With<ServicesLogPanelSlot>>,
    commands: Commands<'w, 's>,
}
fn paint(mut access: PaintAccess) {
    if !access.dirty.0 {
        return;
    }
    let Some((slot, children)) = access.slots.iter().next() else {
        return;
    };
    access.dirty.0 = false;
    let shell = &access.track.shell;
    let fingerprint = shell
        .service_log
        .as_ref()
        .map(|open| log_fingerprint(Some(open)));
    if let Some(children) = children {
        for child in children.iter() {
            access.commands.entity(*child).despawn();
        }
    }
    if shell.service_log.is_some() {
        let fresh = access
            .commands
            .spawn_scene(service_log_panel_scene(shell, &access.palette.inner))
            .id();
        access
            .commands
            .entity(slot)
            .add_one_related::<ChildOf>(fresh);
    }
    access.rendered.rendered = fingerprint;
}
