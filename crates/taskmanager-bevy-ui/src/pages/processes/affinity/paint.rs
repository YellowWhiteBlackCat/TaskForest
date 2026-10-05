//! Modal painting with an explicit access set.
use super::*;
use crate::widgets::scene_paint::ScenePaint;
use bevy::app::{App, PostUpdate};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Res, SystemParam};
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
    state: Res<'w, ProcessAffinityModalState>,
    palette: Res<'w, WindowPalette>,
    dirty: ResMut<'w, PaintState>,
    roots: Query<'w, 's, Entity, With<AppShellRoot>>,
    overlays: Query<'w, 's, Entity, With<ProcessAffinityOverlay>>,
    commands: Commands<'w, 's>,
}
fn paint(mut access: PaintAccess) {
    if !access.dirty.0 {
        return;
    }
    let Some(root) = access.roots.iter().next() else {
        return;
    };
    access.dirty.0 = false;
    for entity in &access.overlays {
        access.commands.entity(entity).despawn();
    }
    if let Some(value) = access.state.session.as_ref() {
        let fresh = access
            .commands
            .spawn_scene(affinity_modal_scene(value, &access.palette.inner))
            .id();
        access
            .commands
            .entity(root)
            .add_one_related::<ChildOf>(fresh);
    }
}
