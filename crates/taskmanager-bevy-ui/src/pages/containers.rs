//! Containers page: cgroup-v2 container rollup (CPU%, memory, member processes).
//!
//! Honesty contract:
//! - An empty list on a healthy host is "no containers running".
//! - A missing rollup is "waiting" (telemetry not collected yet).
//! - Unsupported (cgroup-v1) and PermissionDenied (unreadable cgroup mount)
//!   render typed explanations, never fabricated empty lists.
//! - Scalar metrics (CPU%, memory) render missing values as dashes, never zeroes.
//!
//! Sandbox identity and quota pressure:
//! - Every row carries the provider's runtime family (Docker, Podman, …) as a
//!   badge: a semantic icon plate plus the friendly runtime label. The label
//!   vocabulary mirrors the GPUI reference; the mark comes from the shared
//!   semantic icon registry, never from a font codepoint — Docker and Podman
//!   wear their OWN brand glyphs, never a neighbour's.
//! - The quota column states what the container DECLARED: the observed share
//!   of the declared soft memory ceiling, the declared CPU budget, or the
//!   typed absence of a declaration ("not declared" / "unknown"). The page
//!   never invents a cap: a container that declares nothing is never
//!   compared against a limit of the page's own making.
//! - A container whose observed usage crossed a DECLARED ceiling wears a
//!   pulsing warning halo. The halo's phase follows the shared projection
//!   revision, so it breathes once per accepted telemetry fold — the only
//!   cadence this page owns (a per-frame animation would need a page-owned
//!   system registration outside this file's boundary).
//!
//! Layout contract: the seven columns are percentage shares of the row's
//! content box, not pixel widths, so the header row and every data row share
//! ONE basis (the columns stay aligned), the row shrinks instead of
//! overflowing the page in a narrow window, and every value clips inside its
//! own bounded slot.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::system::{Res, ResMut};
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BorderRadius, FlexDirection, JustifyContent, Node, Overflow, UiRect, Val, percent,
    px, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::ScrollArea;
use taskmanager_application::i18n::t;
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::core::metrics::ScalarAvailability;
use taskmanager_core::core::process_telemetry::{ContainerRollup, ContainerSummary, IsolationKind};
use taskmanager_shell::presentation::{bytes, missing_value};
use taskmanager_ui_contract::IconId;

use crate::app::{Page, PageContext, PageMount, Route};
use crate::drain::ShellProjectionFolded;
use crate::palette::{UiPalette, space_2, space_4, space_8, space_24};
use crate::window::{Role, TextRole};

#[path = "containers/quota.rs"]
mod quota;

use quota::{
    ContainerQuotaBreach, ContainerQuotaView, container_quota_view, quota_cell_scene,
    quota_readout, warning_halo_scene,
};

pub(crate) fn containers_fold_observer(
    _fold: On<ShellProjectionFolded>,
    route: Res<Route>,
    mut mount: ResMut<PageMount>,
) {
    if route.page == Page::Containers {
        mount.requested = true;
    }
}

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

/// Marker on the runtime badge cell: the recognized sandbox family of the
/// container the row belongs to. `None` means the provider could not name the
/// runtime (a typed "unknown container", never a guess).
#[derive(Clone, Copy, Component, Debug, Default, PartialEq, Eq)]
pub(crate) struct ContainerRuntimeBadge(pub(crate) Option<ContainerRuntime>);

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

/// The sandbox family the provider recognized for one container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContainerRuntime {
    /// Docker containers.
    Docker,
    /// Podman containers.
    Podman,
    /// Kubernetes workloads.
    Kubernetes,
    /// LXC containers.
    Lxc,
    /// systemd-nspawn machines.
    SystemdNspawn,
    /// Flatpak sandboxes.
    Flatpak,
    /// Snap packages.
    Snap,
    /// Windows Subsystem for Linux distributions.
    Wsl,
    /// A container of an unrecognized family.
    Other,
}

impl From<IsolationKind> for ContainerRuntime {
    fn from(kind: IsolationKind) -> Self {
        match kind {
            IsolationKind::Docker => ContainerRuntime::Docker,
            IsolationKind::Podman => ContainerRuntime::Podman,
            IsolationKind::Kubernetes => ContainerRuntime::Kubernetes,
            IsolationKind::Lxc => ContainerRuntime::Lxc,
            IsolationKind::SystemdNspawn => ContainerRuntime::SystemdNspawn,
            IsolationKind::Flatpak => ContainerRuntime::Flatpak,
            IsolationKind::Snap => ContainerRuntime::Snap,
            IsolationKind::Wsl => ContainerRuntime::Wsl,
            IsolationKind::OtherContainer => ContainerRuntime::Other,
        }
    }
}

impl ContainerRuntime {
    /// The friendly runtime label (GPUI `runtime_label` parity — the brand
    /// names are product vocabulary, not translatable copy).
    pub(crate) fn label(self) -> &'static str {
        match self {
            ContainerRuntime::Docker => "Docker",
            ContainerRuntime::Podman => "Podman",
            ContainerRuntime::Kubernetes => "Kubernetes",
            ContainerRuntime::Lxc => "LXC",
            ContainerRuntime::SystemdNspawn => "nspawn",
            ContainerRuntime::Flatpak => "Flatpak",
            ContainerRuntime::Snap => "Snap",
            ContainerRuntime::Wsl => "WSL",
            ContainerRuntime::Other => "Container",
        }
    }

    /// The semantic mark that identifies the sandbox family.
    ///
    /// The marks stay inside the shared registry: Docker's corrugated
    /// container blocks and Podman's pod of satellites are their own brand
    /// glyphs (`IconId::Docker` / `IconId::Podman`), so the runtime column
    /// wears the runtime's OWN mark, never a neighbour's. The families
    /// without a dedicated registry plate carry the neutral information
    /// plate — a dedicated brand glyph for each of them is a merge-owner
    /// TODO.
    pub(crate) fn icon(self) -> IconId {
        match self {
            ContainerRuntime::Docker => IconId::Docker,
            ContainerRuntime::Podman => IconId::Podman,
            ContainerRuntime::Kubernetes
            | ContainerRuntime::Lxc
            | ContainerRuntime::SystemdNspawn
            | ContainerRuntime::Flatpak
            | ContainerRuntime::Snap
            | ContainerRuntime::Wsl
            | ContainerRuntime::Other => IconId::System,
        }
    }
}

/// Pre-folded display strings for one container row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContainerRowModel {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) runtime: Option<ContainerRuntime>,
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
        runtime: summary
            .runtime
            .as_ref()
            .map(|kind| ContainerRuntime::from(kind.clone())),
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
    let revision = ctx.shell.projection().system_revision;
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
            vec![table_scene(containers, revision, ctx.palette)]
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
             Text(title) TextRole(Role::Heading) --
            { body }
        ]
    }
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
             Text(text) TextRole(Role::Body)
        ]
    })
}

// ---- the elastic table ----

/// One table column: its label and its share of the row's content box.
#[derive(Clone, Copy)]
struct ContainerColumn {
    label: &'static str,
    basis: f32,
}

/// The seven columns, in display order. The shares add up to the whole row,
/// so the header and the data rows line up and the row can shrink as a whole.
fn columns() -> [ContainerColumn; 7] {
    [
        ContainerColumn {
            label: "ID",
            basis: 14.0,
        },
        ContainerColumn {
            label: t("containers.name"),
            basis: 18.0,
        },
        ContainerColumn {
            label: t("containers.runtime"),
            basis: 14.0,
        },
        ContainerColumn {
            label: t("containers.cpu"),
            basis: 10.0,
        },
        ContainerColumn {
            label: t("containers.memory"),
            basis: 14.0,
        },
        ContainerColumn {
            label: t("containers.processes"),
            basis: 12.0,
        },
        ContainerColumn {
            label: t("proc_insights.resource_limits"),
            basis: 18.0,
        },
    ]
}

fn table_scene(
    containers: &[ContainerSummary],
    revision: u64,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let header = header_scene();
    let rows: Vec<Box<dyn Scene>> = containers
        .iter()
        .map(|summary| {
            let model = container_row_model(summary);
            let quota = container_quota_view(summary);
            Box::new(container_row_scene(&model, &quota, revision, palette)) as Box<dyn Scene>
        })
        .collect();
    let rail = crate::widgets::scrollbar::scrollbar_rail_scene(palette);
    let lane = crate::widgets::scrollbar::rail_lane(UiRect::default());
    Box::new(bsn! {
        // The viewport wrapper owns the scroll area AND the floating rail:
        // the rail is a sibling of the scroll area, so it never scrolls with
        // the content and never takes layout space away from it.
        Node {
            width: percent(100),
            flex_grow: 1.0,
            flex_basis: px(0.0),
            min_height: px(0.0),
            flex_direction: FlexDirection::Column,
        }
        Children [
            Node {
                width: percent(100),
                flex_grow: 1.0,
                flex_basis: px(0.0),
                min_height: px(0.0),
                overflow: Overflow::scroll_y(),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(space_2()),
                padding: { lane },
            }
            ScrollArea
            Children [
                 @{ header } --
                { rows }
            ]
            --
            @{ rail }
        ]
    })
}

fn header_scene() -> impl Scene + use<> {
    let cells: Vec<Box<dyn Scene>> = columns()
        .into_iter()
        .map(|column| Box::new(header_cell_scene(&column)) as Box<dyn Scene>)
        .collect();
    bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(space_8()),
            padding: UiRect::horizontal(Val::Px(space_8())),
        }
        Children [
            { cells }
        ]
    }
}

/// The header cell: the column word in caption ink, clipped inside its own
/// bounded slot.
fn header_cell_scene(column: &ContainerColumn) -> impl Scene + use<> {
    let label = column.label.to_owned();
    let basis = column.basis;
    bsn! {
        Node {
            flex_basis: percent(basis),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            min_width: px(0.0),
            overflow: Overflow::clip_x(),
        }
        Children [
             Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
        ]
    }
}

/// One data cell: the value in its own clipped slot.
fn value_cell_scene(text: String, basis: f32, role: Role) -> impl Scene + use<> {
    bsn! {
        Node {
            flex_basis: percent(basis),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            min_width: px(0.0),
            align_items: AlignItems::FlexStart,
            overflow: Overflow::clip_x(),
        }
        Children [
             Text(text) TextRole(role) TextLayout { linebreak: LineBreak::NoWrap }
        ]
    }
}

/// The runtime cell: the sandbox family's mark plus its friendly label. The
/// mark is a semantic icon plate; the label is the shared brand word.
fn runtime_cell_scene(
    runtime: Option<ContainerRuntime>,
    basis: f32,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let label = runtime.map_or_else(missing_value, |family| family.label().to_owned());
    let mark = crate::icons::icon_scene(
        runtime.map_or(IconId::System, ContainerRuntime::icon),
        palette,
        palette.dim_color,
    );
    bsn! {
        Node {
            flex_basis: percent(basis),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            min_width: px(0.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
            overflow: Overflow::clip_x(),
        }
        ContainerRuntimeBadge({ runtime })
        Children [
             @{ mark } Pickable::IGNORE --
             Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
        ]
    }
}

/// The name cell: the container's name, preceded by the warning halo when the
/// row crossed a declared ceiling.
fn name_cell_scene(
    text: String,
    basis: f32,
    breach: Option<ContainerQuotaBreach>,
    revision: u64,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let mut children: Vec<Box<dyn Scene>> = Vec::new();
    if let Some(breach) = breach {
        children.push(Box::new(warning_halo_scene(breach, revision, palette)) as Box<dyn Scene>);
    }
    children.push(Box::new(bsn! {
        Text(text) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap }
    }) as Box<dyn Scene>);
    bsn! {
        Node {
            flex_basis: percent(basis),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            min_width: px(0.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_4()),
            overflow: Overflow::clip_x(),
        }
        Children [
            { children }
        ]
    }
}

fn container_row_scene(
    model: &ContainerRowModel,
    quota: &ContainerQuotaView,
    revision: u64,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let radius = palette.control_radius_px;
    let id = model.id.clone();
    let name = model.name.clone();
    let runtime = model.runtime;
    let cpu = model.cpu.clone();
    let memory = model.memory.clone();
    let pids = model.pids.clone();
    let [
        id_col,
        name_col,
        runtime_col,
        cpu_col,
        memory_col,
        pids_col,
        limits_col,
    ] = columns();
    let over_limit = quota.breach();
    bsn! {
        Node {
            width: percent(100),
            height: rem(palette.control_height_rem),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::horizontal(Val::Px(space_8())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        ContainerRowMarker({ id.clone() })
        Children [
             @value_cell_scene(id, id_col.basis, Role::Mono) --
             @name_cell_scene(name, name_col.basis, over_limit, revision, palette) --
             @runtime_cell_scene(runtime, runtime_col.basis, palette) --
             @value_cell_scene(cpu, cpu_col.basis, Role::Body) --
             @value_cell_scene(memory, memory_col.basis, Role::Body) --
             @value_cell_scene(pids, pids_col.basis, Role::Body) --
             @quota_cell_scene(quota_readout(quota), limits_col.basis, quota.state, over_limit.is_some(), palette)
        ]
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/containers.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/pages/container_quota_parity.rs"]
mod container_quota_parity;
