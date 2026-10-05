//! One coalesced System paint through declared resources and entity queries.
use super::*;
use crate::widgets::scene_paint::ScenePaint;
use bevy::app::{App, PostUpdate};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, NonSend, Query, Res, ResMut, SystemParam};
use bevy::scene::CommandsSceneExt;
use bevy::ui::UiSystems;

#[derive(Resource, Default)]
pub(crate) struct SystemPaint {
    pub(crate) dirty: bool,
}
pub(crate) fn register(app: &mut App) {
    app.init_resource::<SystemPaint>()
        .add_observer(on_body_added)
        .add_observer(on_projection_folded)
        .add_systems(
            PostUpdate,
            paint_system.in_set(ScenePaint).before(UiSystems::Prepare),
        );
}
fn on_body_added(_event: On<Add<SystemBody>>, mut paint: ResMut<SystemPaint>) {
    paint.dirty = true;
}
fn on_projection_folded(_event: On<ShellProjectionFolded>, mut paint: ResMut<SystemPaint>) {
    paint.dirty = true;
}
#[derive(SystemParam)]
struct SystemRender<'w, 's> {
    track: NonSend<'w, FrontendTrack>,
    palette: Res<'w, WindowPalette>,
    dashboard: Option<Res<'w, SystemDashboardState>>,
    paint: ResMut<'w, SystemPaint>,
    bodies:
        Query<'w, 's, (Entity, &'static ComputedNode, Option<&'static Children>), With<SystemBody>>,
    toolbars: Query<'w, 's, (Entity, Option<&'static Children>), With<SystemDashboardToolbar>>,
    status: Query<'w, 's, &'static mut Text, With<SystemStatusLine>>,
    commands: Commands<'w, 's>,
}
fn paint_system(mut render: SystemRender) {
    if !render.paint.dirty {
        return;
    }
    let Some((body, node, children)) = render.bodies.iter().next() else {
        return;
    };
    render.paint.dirty = false;
    let palette = &render.palette.inner;
    let shell = &render.track.shell;
    let projection = shell.projection();
    let smbios = match shell.smbios_memory_state() {
        SmbiosMemoryState::Ready(ready) => Some(&ready.snapshot),
        _ => None,
    };
    let hardware = projection.hardware.as_ref();
    let npu = projection.npu_inventory.as_ref();
    let sensors = projection.sensors.as_ref();
    let summary = system_summary_model(projection);
    let mut status = status_line_text(hardware, smbios, npu, sensors);
    let dashboard = render
        .dashboard
        .as_ref()
        .filter(|state| state.section == SystemPageSection::Dashboard);
    let scene: Box<dyn Scene> = if let Some(state) = dashboard {
        let count = projection
            .processes
            .as_ref()
            .map_or_else(missing_value, |rows| rows.len().to_string());
        status = format!(
            "{} {} · {} {} · {}",
            t("dashboard.processes"),
            count,
            t("dashboard.active_alerts"),
            projection.alert_active.len(),
            state.window.label()
        );
        let size = node.size() * node.inverse_scale_factor();
        let series = shell.system_timeline_series(state.window);
        Box::new(dashboard::body(
            &series,
            state,
            SystemDashboardBudget::resolve(size.x, size.y),
            palette,
        ))
    } else if render
        .dashboard
        .as_ref()
        .is_some_and(|state| state.section == SystemPageSection::Health)
    {
        Box::new(health::body(
            shell,
            render
                .dashboard
                .as_ref()
                .map(|state| state.health_section)
                .unwrap_or_default(),
            palette,
        ))
    } else {
        Box::new(system_body_scene(
            hardware, smbios, npu, sensors, &summary, palette,
        ))
    };
    if let Some(state) = render.dashboard.as_ref()
        && let Some((host, children)) = render.toolbars.iter().next()
    {
        if let Some(children) = children {
            for child in children.iter() {
                render.commands.entity(*child).despawn();
            }
        }
        let child = render
            .commands
            .spawn_scene(dashboard::toolbar(state, palette))
            .id();
        render
            .commands
            .entity(host)
            .add_one_related::<ChildOf>(child);
    }
    if let Some(children) = children {
        for child in children.iter() {
            render.commands.entity(*child).despawn();
        }
    }
    let fresh = render.commands.spawn_scene(scene).id();
    render
        .commands
        .entity(body)
        .add_one_related::<ChildOf>(fresh);
    if let Ok(mut line) = render.status.single_mut() {
        line.0 = status;
    }
}
