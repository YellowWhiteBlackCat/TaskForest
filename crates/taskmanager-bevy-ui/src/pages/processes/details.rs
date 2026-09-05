//! Selected-process details for the Bevy Applications page.
//!
//! This is the first Bevy product component built around the shared process
//! details contracts. The page owns only the bsn scene and its observer
//! adapters; the application crate folds scalar rows and process-insight
//! facets, while the shell remains the sole selection/request authority.
//!
//! The panel deliberately has two refresh paths:
//!
//! - a selection or shell-fold observer rebuilds the small, bounded surface;
//! - an official Bevy 0.19 `Button` requests a fresh identity-safe insight
//!   sample through the existing app-host client seam.
//!
//! There is no per-frame polling and no native read here. Pending, mismatched,
//! denied, unsupported, and empty observations keep their typed honest copy.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::{Observer, On};
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res};
use bevy::scene::{CommandsSceneExt, Scene, bsn, on};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, Node, Overflow, UiRect, Val, percent,
    px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button, ScrollArea};
use taskmanager_application::process_details_vm::{
    ProcessDetailsField, detail_value, process_details_rows,
};
use taskmanager_application::{ProcessInsightFacetState, ProcessInsightUnavailable, i18n::t};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessLiveKey};
use taskmanager_platform_contract::SubmissionErrorKind;

use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::MISSING_VALUE;

use super::super::processes;
use crate::app::{FrontendTrack, PageContext, SharedRuntimeHandle, ShellTrack};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, space_2, space_8, space_24};
use crate::window::{Role, TextRole, WindowPalette};

const OVERVIEW_FIELDS: &[(ProcessDetailsField, &str)] = &[
    (ProcessDetailsField::Pid, "proc.pid"),
    (ProcessDetailsField::User, "common.user"),
    (ProcessDetailsField::Status, "common.status"),
    (ProcessDetailsField::Cpu, "common.cpu"),
    (ProcessDetailsField::Memory, "common.memory"),
    (ProcessDetailsField::Threads, "common.threads"),
    (ProcessDetailsField::Fds, "proc.fds"),
];

/// One label/value line after the application VM has folded the source fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DetailRow {
    pub(crate) label: String,
    pub(crate) value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InsightCardAction {
    NetworkEscalation,
}

/// A compact card summary. The full bounded facet lists remain a later
/// expansion of this component; this first slice makes every facet visible
/// without growing the page beyond the real window's first viewport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InsightCard {
    pub(crate) title: String,
    pub(crate) value: String,
    pub(crate) action: Option<InsightCardAction>,
}

/// Renderer-neutral input for the mounted panel, kept pure for headless tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProcessDetailsProjection {
    pub(crate) selected: Option<ProcessDetailsSelection>,
    pub(crate) overview: Vec<DetailRow>,
    pub(crate) insights: Vec<InsightCard>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProcessDetailsSelection {
    pub(crate) identity: Option<ProcessLiveKey>,
    pub(crate) pid: u32,
    pub(crate) name: String,
}

/// The persistent panel root. Its children are replaced on each meaningful
/// shell fold, but the root and its observers survive so selection identity
/// stays page-scoped and route changes cleanly despawn the whole surface.
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct ProcessDetailsRoot;

/// Sweep marker for one bounded details render. The root is never swept.
#[derive(Component, Clone, Copy, Default)]
struct ProcessDetailsArtifact;

/// Fold the selected live row through the shared application VM and pair it
/// with only a matching frozen process-insights projection. A same-PID,
/// different-generation answer is therefore rendered as collecting, not as
/// data for the new process.
#[must_use]
pub(crate) fn projection(shell: &ShellApp) -> ProcessDetailsProjection {
    let Some(process) = shell.visible_process_at(shell.selected) else {
        return ProcessDetailsProjection {
            selected: None,
            overview: Vec::new(),
            insights: Vec::new(),
        };
    };

    let vm = process_details_rows(
        process,
        &taskmanager_core::core::units::UnitPreferences::default(),
    );
    let overview = OVERVIEW_FIELDS
        .iter()
        .map(|(field, label)| DetailRow {
            label: t(label).to_owned(),
            value: detail_value(&vm, *field).text_or(MISSING_VALUE).to_owned(),
        })
        .collect();
    let selected = ProcessDetailsSelection {
        identity: ProcessLiveKey::from_process(process),
        pid: process.pid,
        name: process.name.clone(),
    };
    let target = FrozenProcessIdentity::from_process(process);
    let insights = insight_cards(
        shell
            .projection()
            .process_insights
            .as_ref()
            .filter(|candidate| target.as_ref() == Some(&candidate.target)),
    );
    ProcessDetailsProjection {
        selected: Some(selected),
        overview,
        insights,
    }
}

fn insight_cards(
    projection: Option<&taskmanager_application::ProjectedProcessInsights>,
) -> Vec<InsightCard> {
    let collecting = t("proc_insights.collecting").to_owned();
    let network_card = match projection.map(|value| &value.network) {
        None | Some(ProcessInsightFacetState::Pending) => InsightCard {
            title: t("proc_insights.network_throughput").to_owned(),
            value: collecting.clone(),
            action: None,
        },
        Some(ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
            FailureKind::RequiresEscalation,
        ))) => InsightCard {
            title: t("proc_insights.network_throughput").to_owned(),
            value: format!(
                "{} ({})",
                t("proc_insights.network_requires_escalation"),
                t("proc_insights.enable_network_capture")
            ),
            action: Some(InsightCardAction::NetworkEscalation),
        },
        Some(ProcessInsightFacetState::Unavailable(reason)) => InsightCard {
            title: t("proc_insights.network_throughput").to_owned(),
            value: unavailable_text(reason),
            action: None,
        },
        Some(ProcessInsightFacetState::Current(snapshot)) => {
            let has_escalation = snapshot.traffic_failure == Some(FailureKind::RequiresEscalation);
            InsightCard {
                title: t("proc_insights.network_throughput").to_owned(),
                value: network_summary(snapshot),
                action: if has_escalation {
                    Some(InsightCardAction::NetworkEscalation)
                } else {
                    None
                },
            }
        }
    };
    vec![
        InsightCard {
            title: t("proc_insights.threads").to_owned(),
            value: facet_value(
                projection.map(|value| &value.threads),
                threads_summary,
                &collecting,
            ),
            action: None,
        },
        InsightCard {
            title: t("proc_insights.open_files").to_owned(),
            value: facet_value(
                projection.map(|value| &value.open_files),
                open_files_summary,
                &collecting,
            ),
            action: None,
        },
        network_card,
        InsightCard {
            title: t("common.gpu").to_owned(),
            value: facet_value(projection.map(|value| &value.gpu), gpu_summary, &collecting),
            action: None,
        },
        InsightCard {
            title: t("proc_insights.resource_limits").to_owned(),
            value: facet_value(
                projection.map(|value| &value.resources),
                resources_summary,
                &collecting,
            ),
            action: None,
        },
        InsightCard {
            title: t("proc_insights.isolation").to_owned(),
            value: facet_value(
                projection.map(|value| &value.isolation),
                isolation_summary,
                &collecting,
            ),
            action: None,
        },
        InsightCard {
            title: t("prop.environment").to_owned(),
            value: facet_value(
                projection.map(|value| &value.environment),
                environment_summary,
                &collecting,
            ),
            action: None,
        },
    ]
}

fn facet_value<T>(
    state: Option<&ProcessInsightFacetState<T>>,
    current: fn(&T) -> String,
    collecting: &str,
) -> String {
    match state {
        None | Some(ProcessInsightFacetState::Pending) => collecting.to_owned(),
        Some(ProcessInsightFacetState::Unavailable(reason)) => unavailable_text(reason),
        Some(ProcessInsightFacetState::Current(value)) => current(value),
    }
}

fn unavailable_text(reason: &ProcessInsightUnavailable) -> String {
    match reason {
        ProcessInsightUnavailable::Provider(
            FailureKind::PermissionDenied | FailureKind::RequiresEscalation,
        ) => t("proc_insights.permission_denied").to_owned(),
        ProcessInsightUnavailable::Provider(FailureKind::Unsupported)
        | ProcessInsightUnavailable::Submission(SubmissionErrorKind::UnsupportedCapability) => {
            t("proc_insights.unsupported_provider").to_owned()
        }
        _ => t("proc_insights.unavailable").to_owned(),
    }
}

mod formatting;
pub(crate) use formatting::*;

// ---- observer bridge ----------------------------------------------------

fn bootstrap_details_page(
    trigger: On<Add, ProcessDetailsRoot>,
    mut commands: Commands,
    mut track: Option<NonSendMut<FrontendTrack>>,
    runtime: Option<Res<SharedRuntimeHandle>>,
) {
    let root = trigger.event().entity;
    let fold_observer = commands.spawn(Observer::new(on_details_folded)).id();
    let selection_observer = commands
        .spawn(Observer::new(on_details_selection_changed))
        .id();
    commands
        .entity(root)
        .add_one_related::<ChildOf>(fold_observer);
    commands
        .entity(root)
        .add_one_related::<ChildOf>(selection_observer);

    if let Some(mut track) = track.take() {
        queue_selected_process_insights(&mut track, runtime.as_deref(), false);
    }
}

fn on_details_folded(
    _fold: On<ShellProjectionFolded>,
    track: ShellTrack,
    palette: Res<WindowPalette>,
    roots: Query<Entity, With<ProcessDetailsRoot>>,
    artifacts: Query<Entity, With<ProcessDetailsArtifact>>,
    mut commands: Commands,
) {
    let Ok(root) = roots.single() else {
        return;
    };
    rebuild(
        &mut commands,
        root,
        track.shell(),
        &palette.inner,
        &artifacts,
    );
}

fn on_details_selection_changed(
    _selection: On<processes::ProcessSelectionChanged>,
    mut track: NonSendMut<FrontendTrack>,
    palette: Res<WindowPalette>,
    roots: Query<Entity, With<ProcessDetailsRoot>>,
    artifacts: Query<Entity, With<ProcessDetailsArtifact>>,
    mut commands: Commands,
    runtime: Option<Res<SharedRuntimeHandle>>,
) {
    let Ok(root) = roots.single() else {
        return;
    };
    rebuild(
        &mut commands,
        root,
        &track.shell,
        &palette.inner,
        &artifacts,
    );
    // The table observer has already reduced the selection before publishing
    // the event, so this uses exactly the same shell target as the panel.
    queue_selected_process_insights(&mut track, runtime.as_deref(), false);
}

fn on_refresh_activated(
    _activate: On<Activate>,
    mut track: NonSendMut<FrontendTrack>,
    runtime: Option<Res<SharedRuntimeHandle>>,
) {
    queue_selected_process_insights(&mut track, runtime.as_deref(), true);
}

fn queue_selected_process_insights(
    track: &mut FrontendTrack,
    runtime: Option<&SharedRuntimeHandle>,
    force: bool,
) {
    let Some(target) = track.shell.selected_process_identity() else {
        return;
    };
    if !force
        && track
            .shell
            .projection()
            .process_insights
            .as_ref()
            .is_some_and(|projection| projection.target == target)
    {
        return;
    }
    let Some(effect) = track.shell.request_process_insights() else {
        return;
    };
    let Some(runtime) = runtime else {
        return;
    };
    let mut client = runtime.shared.lock_client();
    taskmanager_shell::queue_effect(&mut track.shell, &mut client, effect);
}

fn rebuild(
    commands: &mut Commands,
    root: Entity,
    shell: &ShellApp,
    palette: &UiPalette,
    artifacts: &Query<Entity, With<ProcessDetailsArtifact>>,
) {
    for entity in artifacts.iter() {
        commands.entity(entity).despawn();
    }
    let child = commands
        .spawn_scene(details_content_scene(&projection(shell), palette))
        .id();
    commands.entity(root).add_one_related::<ChildOf>(child);
}

// ---- bsn render adapter -------------------------------------------------

/// Mounted page component. The details body is a bounded, two-column card
/// grid so the first viewport remains useful at both standard and compact
/// capture sizes.
pub(crate) fn panel_scene(context: &PageContext<'_>) -> impl Scene + use<> {
    let palette = context.palette;
    bsn! {
        Node {
            width: percent(32),
            min_width: px(0.0),
            height: percent(100),
            overflow: Overflow::scroll_y(),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        ScrollArea
        on(bootstrap_details_page)
        ProcessDetailsRoot
        Children [
            ( { details_content_scene(&projection(context.shell), palette) } ),
        ]
    }
}

fn details_content_scene(
    projection: &ProcessDetailsProjection,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let (heading, refresh) = match &projection.selected {
        Some(selected) => (
            selected.name.clone(),
            Some(Box::new(refresh_button_scene(palette)) as Box<dyn Scene>),
        ),
        None => (t("proc.unknown_process").to_owned(), None),
    };
    let overview_rows: Vec<Box<dyn Scene>> = projection
        .overview
        .iter()
        .map(|row| Box::new(detail_row_scene(row)) as Box<dyn Scene>)
        .collect();
    let insight_cards: Vec<Box<dyn Scene>> = projection
        .insights
        .iter()
        .map(|card| Box::new(insight_card_scene(card, palette)) as Box<dyn Scene>)
        .collect();
    let selected = projection.selected.is_some();
    let overview_title = if selected {
        t("prop.overview").to_owned()
    } else {
        t("proc_insights.loading").to_owned()
    };
    bsn! {
        Node {
            width: percent(100),
            flex_grow: 1.0,
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
        }
        ProcessDetailsArtifact
        Children [
            (
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_8()),
                }
                    Children [
                        ( Text(t("prop.process_details")) TextRole(Role::Caption) ),
                        ( Text(heading) TextRole(Role::Body) ),
                    ( { refresh } ),
                ]
            ),
            ( Text(t("proc_insights.scroll_hint")) TextRole(Role::Caption) ),
            ( Text(overview_title) TextRole(Role::Caption) ),
            { overview_rows },
            (
                Text(t("prop.insights"))
                TextRole(Role::Caption)
            ),
            (
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                }
                Children [
                    { insight_cards },
                ]
            ),
        ]
    }
}

fn detail_row_scene(row: &DetailRow) -> impl Scene + use<> {
    let label = row.label.clone();
    let value = row.value.clone();
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(space_2()),
        }
        Children [
            ( Node { width: px(92.0) } Children [ ( Text(label) TextRole(Role::Caption) ) ] ),
            ( Text(value) TextRole(Role::Body) ),
        ]
    }
}

fn insight_card_scene(card: &InsightCard, palette: &UiPalette) -> impl Scene + use<> {
    let title = card.title.clone();
    let value = card.value.clone();
    let action_scene = card
        .action
        .map(|_| Box::new(network_escalate_button_scene(palette)) as Box<dyn Scene>);
    bsn! {
        Node {
            width: percent(100),
            min_height: px(palette.control_height_px),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            column_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_2())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ palette.content_bg })
        Children [
            ( Node { width: px(104.0), flex_shrink: 0.0 } Children [ ( Text(title) TextRole(Role::Caption) ) ] ),
            (
                Node {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                }
                Children [
                    ( Text(value) TextRole(Role::Body) ),
                    ( { action_scene } ),
                ]
            ),
        ]
    }
}

fn network_escalate_button_scene(palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node {
            height: px(palette.control_height_px),
            padding: UiRect::horizontal(Val::Px(space_8())),
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ palette.nav_active_bg })
        Button
        on(on_network_escalate_activated)
        Children [
            ( Text(t("proc_insights.enable_network_capture")) TextRole(Role::Body) ),
        ]
    }
}

fn on_network_escalate_activated(
    _activate: On<Activate>,
    mut track: NonSendMut<FrontendTrack>,
    runtime: Option<Res<SharedRuntimeHandle>>,
) {
    let effect = ShellApp::request_process_network_escalation();
    let Some(runtime) = runtime else {
        return;
    };
    let mut client = runtime.shared.lock_client();
    taskmanager_shell::queue_effect(&mut track.shell, &mut client, effect);
}

fn refresh_button_scene(palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node {
            width: px(space_24() * 3.0),
            height: px(palette.control_height_px),
            padding: UiRect::horizontal(Val::Px(space_8())),
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ palette.nav_active_bg })
        Button
        on(on_refresh_activated)
        Children [
            ( Text(t("common.refresh")) TextRole(Role::Body) ),
        ]
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/pages/process_details.rs"]
mod tests;
