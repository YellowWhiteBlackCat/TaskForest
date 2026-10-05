//! Scoped System health rendering and ordinary SMART confirmation controls.
use crate::app::FrontendTrack;
use crate::input::ShellInteractionApplied;
use crate::palette::{UiPalette, space_8};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole};
use bevy::ecs::hierarchy::Children;
use bevy::ecs::{
    component::Component,
    observer::On,
    system::{Commands, NonSendMut, Query},
};
use bevy::scene::{Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::widget::Text;
use bevy::ui::{BackgroundColor, FlexDirection, Node, UiRect, percent, px};
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_core::core::identity::DeviceGeneration;
use taskmanager_core::core::smart::SmartSelfTestKind;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::health_review::{
    HealthReviewSection, sensor_groups, storage_groups,
};
#[derive(Component, Clone)]
pub(crate) struct HealthSelfTest(
    pub(crate) String,
    pub(crate) DeviceGeneration,
    pub(crate) SmartSelfTestKind,
);
impl Default for HealthSelfTest {
    fn default() -> Self {
        Self(
            String::new(),
            DeviceGeneration::default(),
            SmartSelfTestKind::Short,
        )
    }
}
fn activate(
    event: On<Activate>,
    controls: Query<&HealthSelfTest>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    let Ok(target) = controls.get(event.entity) else {
        return;
    };
    let observed = track
        .shell
        .projection()
        .snapshot
        .as_ref()
        .is_some_and(|snapshot| {
            snapshot.disks.iter().any(|disk| {
                disk.device_id == target.0
                    && disk.device_generation == target.1
                    && target.1.is_valid()
            })
        });
    if !observed {
        return;
    }
    let kind = target.2;
    if super::super::performance::request_smart_self_test(&mut track.shell, &target.0, kind) {
        commands.trigger(ShellInteractionApplied);
    }
}
pub(super) fn body(
    shell: &ShellApp,
    section: HealthReviewSection,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let projection = shell.projection();
    let disks = projection
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.disks.as_slice())
        .unwrap_or_default();
    let (reports, _) = projection.smart_projection();
    let storage = storage_groups(
        projection
            .storage_health_projection()
            .map(|(snapshot, _)| snapshot),
        disks,
        reports.observations(),
    );
    let sensors = sensor_groups(projection.sensors.as_ref());
    let groups = storage
        .into_iter()
        .filter(|_| section != HealthReviewSection::Sensors)
        .chain(
            sensors
                .into_iter()
                .filter(|_| section != HealthReviewSection::Storage),
        );
    let mut cards: Vec<Box<dyn Scene>> = groups.map(|group| {
        let rows = group.rows.into_iter().map(|row| {
            let line = format!("{}: {}", row.label, row.value);
            Box::new(bsn! { Text(line) TextLayout { linebreak: LineBreak::WordOrCharacter } TextRole(Role::Body) }) as Box<dyn Scene>
        }).collect::<Vec<_>>();
        let title = group.title;
        Box::new(bsn! { Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column, flex_shrink: 0.0, row_gap: px(space_8()), padding: UiRect::all(px(space_8())) } BackgroundColor({palette.panel_fill}) Children [ Text(title) TextRole(Role::Body) -- { rows } ] }) as Box<dyn Scene>
    }).collect();
    for disk in disks {
        for kind in [SmartSelfTestKind::Short, SmartSelfTestKind::Extended] {
            let label = t(if kind == SmartSelfTestKind::Extended {
                "health.extended_test"
            } else {
                "health.short_test"
            });
            let id = disk.device_id.clone();
            cards.push(Box::new(bsn! {
                Node { width: percent(100), min_height: px(palette.control_height_px), flex_shrink: 0.0, padding: UiRect::all(px(space_8())) }
                Button HealthSelfTest(id, {disk.device_generation}, kind) ControlVisual(ControlTone::Surface, false) on(activate)
                Children [ Text(label) TextRole(Role::Caption) ]
            }));
        }
    }
    bsn! { Node { width: percent(100), min_width: px(0.0), flex_direction: FlexDirection::Column, row_gap: px(space_8()), flex_shrink: 0.0 } Children [{ cards }] }
}
