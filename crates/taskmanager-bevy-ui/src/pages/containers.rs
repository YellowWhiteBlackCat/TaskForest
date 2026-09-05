//! Containers page: cgroup-v2 container rollup (CPU%, memory, member processes).
//!
//! Honesty contract:
//! - An empty list on a healthy host is "no containers running".
//! - A missing rollup is "waiting" (telemetry not collected yet).
//! - Unsupported (cgroup-v1) and PermissionDenied (unreadable cgroup mount)
//!   render typed explanations, never fabricated empty lists.
//! - Scalar metrics (CPU%, memory) render missing values as dashes, never zeroes.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn, template_value};
use bevy::ui::prelude::{
    AlignItems, BorderRadius, FlexDirection, JustifyContent, Node, Overflow, UiRect, Val, percent,
    px,
};
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::core::metrics::ScalarAvailability;
use taskmanager_core::core::process_telemetry::{ContainerRollup, ContainerSummary};
use taskmanager_shell::presentation::{bytes, missing_value};

use crate::app::{Page, PageContext};
use crate::palette::{UiPalette, no_wrap_text, space_2, space_8, space_24};
use crate::window::{Role, TextRole};

/// Alias for [`ContainerSummary`] for caller parity.
#[allow(dead_code)]
pub(crate) type ContainerItem = ContainerSummary;

/// Which branch the page body takes for one rollup projection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Component)]
pub(crate) enum ContainersPageBranch {
    /// No rollup has arrived yet.
    #[default]
    Waiting,
    /// cgroup-v1 host (no unified rollup exists).
    Unsupported,
    /// The unified cgroup mount is not readable.
    PermissionDenied,
    /// Healthy source, zero containers (a real, healthy state).
    Empty,
    /// Healthy source with container rows to render.
    Table,
}

/// Root marker on the mounted Containers page.
#[derive(Clone, Component, Default)]
pub(crate) struct ContainersPageRoot;

/// Marker on the container row entity holding the container ID.
#[derive(Clone, Component, Default)]
pub(crate) struct ContainerRowMarker(pub(crate) String);

/// Marker on the page recording which honest branch was rendered.
#[derive(Clone, Component, Default)]
pub(crate) struct ContainersBranchMarker(pub(crate) ContainersPageBranch);

/// Resolve the page branch from the current projection (pure function).
pub(crate) fn page_branch(rollup: Option<&ContainerRollup>) -> ContainersPageBranch {
    let Some(rollup) = rollup else {
        return ContainersPageBranch::Waiting;
    };
    match rollup.state.status {
        DeviceStatus::Unsupported => ContainersPageBranch::Unsupported,
        DeviceStatus::PermissionDenied => ContainersPageBranch::PermissionDenied,
        _ => {
            if rollup.containers.is_empty() {
                ContainersPageBranch::Empty
            } else {
                ContainersPageBranch::Table
            }
        }
    }
}

/// Pre-folded display strings for one container row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContainerRowModel {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) cpu: String,
    pub(crate) memory: String,
    pub(crate) pids: String,
}

/// Fold one container summary into its row display strings.
pub(crate) fn container_row_model(summary: &ContainerSummary) -> ContainerRowModel {
    let cpu = match summary.cpu_percentage.availability() {
        ScalarAvailability::Available | ScalarAvailability::Partial(_) => summary
            .cpu_percentage
            .current_value()
            .map(|val| format!("{val:.1}%"))
            .unwrap_or_else(missing_value),
        _ => missing_value(),
    };
    let memory = summary
        .memory_bytes
        .current_value()
        .copied()
        .map(bytes)
        .unwrap_or_else(missing_value);
    let pids = if summary.member_pids.is_empty() {
        missing_value()
    } else {
        summary.member_pids.len().to_string()
    };
    ContainerRowModel {
        id: summary.id.clone(),
        name: summary.name.clone(),
        cpu,
        memory,
        pids,
    }
}

/// Primary scene entry for the Containers page.
pub(crate) fn scene(ctx: &PageContext<'_>) -> impl Scene + use<> {
    let rollup = ctx.shell.projection().containers.as_ref();
    let branch = page_branch(rollup);
    let title = Page::Containers.title();
    let body: Vec<Box<dyn Scene>> = match branch {
        ContainersPageBranch::Waiting => {
            vec![message_scene(t("containers.telemetry_not_collected"))]
        }
        ContainersPageBranch::Unsupported => vec![message_scene(t("containers.unsupported"))],
        ContainersPageBranch::PermissionDenied => {
            vec![message_scene(t("containers.permission_denied"))]
        }
        ContainersPageBranch::Empty => vec![message_scene(t("containers.no_containers"))],
        ContainersPageBranch::Table => {
            let containers = rollup.map(|r| r.containers.as_slice()).unwrap_or(&[]);
            vec![table_scene(containers, ctx.palette)]
        }
    };
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_8())),
        }
        ContainersPageRoot
        ContainersBranchMarker({ branch })
        Children [
            ( Text(title) TextRole(Role::Heading) ),
            { body },
        ]
    }
}

/// Content alias for page-agent contract compatibility.
#[allow(dead_code)]
pub(crate) fn content(ctx: &PageContext<'_>) -> impl Scene + use<> {
    scene(ctx)
}

fn message_scene(text: &'static str) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100),
            padding: UiRect::all(Val::Px(space_24())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        }
        Children [
            ( Text(text) TextRole(Role::Body) ),
        ]
    })
}

fn table_scene(containers: &[ContainerSummary], palette: &UiPalette) -> Box<dyn Scene> {
    let header = header_scene(palette);
    let rows: Vec<Box<dyn Scene>> = containers
        .iter()
        .map(|summary| {
            let model = container_row_model(summary);
            Box::new(container_row_scene(&model, palette)) as Box<dyn Scene>
        })
        .collect();
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            ( { header } ),
            { rows },
        ]
    })
}

fn header_scene(palette: &UiPalette) -> impl Scene + use<> {
    let _ = palette;
    bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(space_8()),
            padding: UiRect::horizontal(Val::Px(space_8())),
        }
        Children [
            (
                Node { width: px(200.0), overflow: Overflow::clip_x() }
                Children [
                    ( Text("ID") TextRole(Role::Caption) template_value(no_wrap_text()) ),
                ]
            ),
            (
                Node { width: px(220.0), overflow: Overflow::clip_x() }
                Children [
                    ( Text(t("containers.name")) TextRole(Role::Caption) template_value(no_wrap_text()) ),
                ]
            ),
            (
                Node { width: px(90.0), overflow: Overflow::clip_x() }
                Children [
                    ( Text(t("containers.cpu")) TextRole(Role::Caption) template_value(no_wrap_text()) ),
                ]
            ),
            (
                Node { width: px(110.0), overflow: Overflow::clip_x() }
                Children [
                    ( Text(t("containers.memory")) TextRole(Role::Caption) template_value(no_wrap_text()) ),
                ]
            ),
            (
                Node { width: px(90.0), overflow: Overflow::clip_x() }
                Children [
                    ( Text(t("containers.processes")) TextRole(Role::Caption) template_value(no_wrap_text()) ),
                ]
            ),
        ]
    }
}

fn container_row_scene(model: &ContainerRowModel, palette: &UiPalette) -> impl Scene + use<> {
    let height = palette.control_height_px;
    let radius = palette.control_radius_px;
    let id = model.id.clone();
    let name = model.name.clone();
    let cpu = model.cpu.clone();
    let memory = model.memory.clone();
    let pids = model.pids.clone();
    bsn! {
        Node {
            width: percent(100),
            height: px(height),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::horizontal(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        ContainerRowMarker({ id.clone() })
        Children [
            (
                Node { width: px(200.0), align_items: AlignItems::FlexStart }
                Children [
                    ( Text(id) TextRole(Role::Mono) ),
                ]
            ),
            (
                Node { width: px(220.0), align_items: AlignItems::FlexStart }
                Children [
                    ( Text(name) TextRole(Role::Body) ),
                ]
            ),
            (
                Node { width: px(90.0), align_items: AlignItems::FlexStart }
                Children [
                    ( Text(cpu) TextRole(Role::Body) ),
                ]
            ),
            (
                Node { width: px(110.0), align_items: AlignItems::FlexStart }
                Children [
                    ( Text(memory) TextRole(Role::Body) ),
                ]
            ),
            (
                Node { width: px(90.0), align_items: AlignItems::FlexStart }
                Children [
                    ( Text(pids) TextRole(Role::Body) ),
                ]
            ),
        ]
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/containers.rs"]
mod tests;
