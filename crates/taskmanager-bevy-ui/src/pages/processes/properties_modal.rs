//! Selected-process properties modal overlay (Applications page).
//!
//! Ownership line: the shell owns `process_properties_target` via the neutral
//! application interaction surface; this module owns the Bevy-local modal
//! surface, the scrollable property list backed by the shared
//! `process_details_vm`, and the typed dismiss trigger.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res};
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, Overflow,
    PositionType, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button, ScrollArea};
use taskmanager_application::i18n::t;
use taskmanager_application::process_details_vm::{ProcessDetailsField, process_details_rows};
use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::MISSING_VALUE;

use crate::app::{FrontendTrack, ShellTrack};
use crate::drain::ShellProjectionFolded;
use crate::input::ShellInteractionApplied;
use crate::palette::{UiPalette, space_4, space_8, space_24};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};

/// The materialized view data for an active process properties modal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProcessPropertiesView {
    pub(crate) target: FrozenProcessIdentity,
    pub(crate) rows: Vec<(String, String)>,
}

/// Publishes properties modal transition: `Some(view)` mounts the overlay,
/// `None` despawns it.
#[derive(Event)]
pub(crate) struct ProcessPropertiesChanged(pub(crate) Option<ProcessPropertiesView>);

/// Marker on the mounted process properties modal overlay.
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessPropertiesOverlay;

/// Marker on the dismiss/close button.
#[derive(Component, Clone, Default)]
pub(crate) struct ProcessPropertiesDismissButton;

fn field_label(field: ProcessDetailsField) -> &'static str {
    match field {
        ProcessDetailsField::Name => t("common.name"),
        ProcessDetailsField::Pid => "PID",
        ProcessDetailsField::ParentPid => t("prop.parent_pid"),
        ProcessDetailsField::AncestorLineage => t("proc.ancestor_lineage"),
        ProcessDetailsField::User => t("common.user"),
        ProcessDetailsField::Status => t("common.status"),
        ProcessDetailsField::Cpu => t("common.cpu"),
        ProcessDetailsField::Memory => t("common.memory"),
        ProcessDetailsField::Pss => t("proc.pss"),
        ProcessDetailsField::Uss => t("proc.uss"),
        ProcessDetailsField::Shared => t("proc.shared"),
        ProcessDetailsField::AnonHugePages => t("proc.anon_huge_pages"),
        ProcessDetailsField::Swap => t("proc.swap"),
        ProcessDetailsField::Threads => t("common.threads"),
        ProcessDetailsField::Fds => t("proc.fds"),
        ProcessDetailsField::Nice => t("proc.nice"),
        ProcessDetailsField::SchedPolicy => t("proc.sched_policy"),
        ProcessDetailsField::OomScore => t("proc.oom_score"),
        ProcessDetailsField::PageFaults => t("proc.page_faults"),
        ProcessDetailsField::StartTime => t("prop.start_time"),
        ProcessDetailsField::CpuTime => t("proc.cpu_time"),
        ProcessDetailsField::DiskReadRate => t("proc.disk_read"),
        ProcessDetailsField::DiskWriteRate => t("proc.disk_write"),
        ProcessDetailsField::NetworkRate => t("common.network"),
        ProcessDetailsField::CancelledWriteBytes => t("proc.cancelled_write"),
        ProcessDetailsField::DiskReadTotal => t("proc.disk_read"),
        ProcessDetailsField::DiskWriteTotal => t("proc.disk_write"),
        ProcessDetailsField::Exe => t("common.executable"),
        ProcessDetailsField::Cmdline => t("prop.command_line"),
    }
}

/// Republish the properties modal state from the shell authority.
pub(crate) fn republish(shell: &ShellApp, commands: &mut Commands) {
    let view = shell.process_properties_target().map(|target| {
        let process = target
            .live_key()
            .and_then(|key| shell.visible_process_by_identity(key))
            .or_else(|| {
                shell
                    .visible_processes()
                    .iter()
                    .copied()
                    .find(|p| p.pid == target.pid)
            });
        let rows = if let Some(process) = process {
            let mut process_clone = process.clone();
            process_clone.populate_ancestor_lineage(shell.projection().processes_slice());
            let units = UnitPreferences::default();
            let vms = process_details_rows(&process_clone, &units);
            vms.into_iter()
                .map(|vm| {
                    let label = field_label(vm.field);
                    let val = vm.value.text_or(MISSING_VALUE).to_owned();
                    (label.to_owned(), val)
                })
                .collect()
        } else {
            vec![
                (t("common.name").to_owned(), target.name.clone()),
                ("PID".to_owned(), target.pid.to_string()),
                (
                    t("common.status").to_owned(),
                    t("feedback.process_gone").to_owned(),
                ),
            ]
        };
        ProcessPropertiesView {
            target: target.clone(),
            rows,
        }
    });
    commands.trigger(ProcessPropertiesChanged(view));
}

/// Construct the declarative scene for the process properties modal.
pub(crate) fn properties_modal_scene(
    view: &ProcessPropertiesView,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let title = format!("{} · {}", t("dialog.properties"), view.target.name);
    let subtitle = format!("PID: {}", view.target.pid);

    let rows: Vec<Box<dyn Scene>> = view
        .rows
        .iter()
        .map(|(label, value)| {
            let l = label.clone();
            let v = value.clone();
            Box::new(bsn! {
                Node {
                    width: percent(100),
                    min_height: px(palette.control_height_px),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(space_8()),
                    padding: UiRect::horizontal(Val::Px(space_4())),
                }
                Children [
                    (
                        Node {
                            width: px(160.0),
                            align_items: AlignItems::Center,
                            overflow: Overflow::clip_x(),
                        }
                        Children [
                            ( Text(l) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } )
                        ]
                    ),
                    (
                        Node {
                            flex_grow: 1.0,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::FlexEnd,
                            overflow: Overflow::clip_x(),
                        }
                        Children [
                            ( Text(v) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap } )
                        ]
                    ),
                ]
            }) as Box<dyn Scene>
        })
        .collect();

    let list = Box::new(bsn! {
        Node {
            width: percent(100),
            max_height: px(360.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            overflow: Overflow::scroll_y(),
        }
        ScrollArea
        Children [
            { rows }
        ]
    }) as Box<dyn Scene>;

    let panel = Box::new(bsn! {
        Node {
            width: px(520.0),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_24())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
            ( Text(title) TextRole(Role::Heading) ),
            ( Text(subtitle) TextRole(Role::Caption) ),
            ( { list } ),
            (
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::End,
                    margin: UiRect::top(Val::Px(space_8())),
                }
                Children [
                    (
                        Text({ t("common.close").to_owned() })
                        TextRole(Role::Body)
                        ControlVisual(ControlTone::Surface, true)
                        Button
                        on(on_properties_dismiss_activated)
                        ProcessPropertiesDismissButton
                    ),
                ]
            ),
        ]
    }) as Box<dyn Scene>;

    let scrim = palette.scrim;
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        BackgroundColor({ scrim })
        ProcessPropertiesOverlay
        Children [
            ( { panel } ),
        ]
    }) as Box<dyn Scene>
}

/// Observer: mount or despawn the modal overlay on state change.
pub(crate) fn on_properties_changed(
    changed: On<ProcessPropertiesChanged>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<AppShellRoot>>,
    overlays: Query<Entity, With<ProcessPropertiesOverlay>>,
    mut commands: Commands,
) {
    for entity in &overlays {
        commands.entity(entity).despawn();
    }
    let Some(view) = changed.event().0.as_ref() else {
        return;
    };
    let Some(palette) = palette else {
        return;
    };
    let Ok(root) = roots.single() else {
        return;
    };
    let overlay = commands
        .spawn_scene(properties_modal_scene(view, &palette.inner))
        .id();
    commands.entity(root).add_one_related::<ChildOf>(overlay);
}

/// Close button activation: dismiss through the shell and republish.
pub(crate) fn on_properties_dismiss_activated(
    _activate: On<Activate>,
    mut track: NonSendMut<FrontendTrack>,
    mut commands: Commands,
) {
    track.shell.dismiss_overlay();
    republish(&track.shell, &mut commands);
    commands.trigger(ShellInteractionApplied);
}

fn on_properties_fold_sync(
    _fold: On<ShellProjectionFolded>,
    track: ShellTrack,
    mut commands: Commands,
) {
    if track.shell().process_properties_target().is_some() {
        republish(track.shell(), &mut commands);
    }
}

fn on_properties_applied_sync(
    _applied: On<ShellInteractionApplied>,
    track: ShellTrack,
    mut commands: Commands,
) {
    republish(track.shell(), &mut commands);
}

/// Register the properties modal observer on the app composition.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.add_observer(on_properties_changed);
    app.add_observer(on_properties_fold_sync);
    app.add_observer(on_properties_applied_sync);
}
