//! Iced Containers page: per-container CPU + memory rollup table.
//!
//! GPUI TopPage parity: the surface renders as a full-width page (page
//! header, status body and table) instead of the old centered modal card.
//! The rollup still comes from the shared shell data (the field the platform
//! container-rollup lane feeds) and the open/close lifecycle still rides the
//! local-surface slot (`Message::OpenContainers` opens, Escape / the header
//! close / `Message::DismissOverlay` dismiss) — so the route stays live with
//! zero new message vocabulary. Honesty contract mirrors the core module: an
//! empty list on a healthy host is "no containers running", a typed
//! `DeviceState` explains unsupported/denied/stale sources, and no cell ever
//! fabricates a zero.

use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_application::container_row_window;
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::core::metrics::{ScalarAvailability, ScalarObservation};
use taskmanager_core::core::process_telemetry::{ContainerRollup, ContainerSummary, IsolationKind};

// Shared locale catalog for the per-container table column headers and the
// typed source states (the page title and status messages stay on the
// renderer-local `crate::i18n`).
use taskmanager_application::i18n::t;

use crate::app::Message;
use crate::i18n::{self, Key};
use crate::theme;
use taskmanager_theme::tokens;

use super::components::message_panel;
use taskmanager_shell::presentation::{bytes, missing_value};

/// Which branch the page body takes for one rollup projection. Pure seam the
/// headless tests pin: a typed non-healthy source and a genuinely
/// container-free host never share a branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContainersPageBranch {
    /// No rollup has ever arrived yet.
    Waiting,
    /// cgroup-v1 host (no unified rollup exists).
    Unsupported,
    /// The unified cgroup mount is not readable.
    PermissionDenied,
    /// The rollup source went stale.
    Stale,
    /// Healthy source, zero containers (a real, healthy state).
    Empty,
    /// Healthy source with rows to render.
    Table,
}

/// Resolve the page branch from the current projection (pure).
pub(crate) fn page_branch(rollup: Option<&ContainerRollup>) -> ContainersPageBranch {
    let Some(rollup) = rollup else {
        return ContainersPageBranch::Waiting;
    };
    match rollup.state.status {
        DeviceStatus::Unsupported => ContainersPageBranch::Unsupported,
        DeviceStatus::PermissionDenied => ContainersPageBranch::PermissionDenied,
        DeviceStatus::Stale => ContainersPageBranch::Stale,
        // Healthy covers a populated list, a genuinely container-free host,
        // and a missing-tool host (which is an empty-but-healthy projection).
        DeviceStatus::Healthy | DeviceStatus::MissingTool => {
            if rollup.containers.is_empty() {
                ContainersPageBranch::Empty
            } else {
                ContainersPageBranch::Table
            }
        }
    }
}

/// Render the Containers page from the current shell rollup. The surface
/// replaces the whole tree (the local-surface slot's modal precedence), so
/// the page owns its header and close affordance; Escape dismisses through
/// the same slot. A missing rollup renders the honest waiting state; a typed
/// non-healthy `DeviceState` explains why there are no rows.
pub(super) fn render(app: &crate::IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let language = app.language();
    let shell = &app.shell;

    let body: Element<'_, Message, iced::Theme, iced::Renderer> =
        page_body(theme_snapshot, containers_rollup(shell));
    let page = column![page_header(theme_snapshot, language), body]
        .spacing(f32::from(tokens::SPACE_12))
        .width(Length::Fill)
        .height(Length::Fill);
    container(page)
        .padding(f32::from(tokens::SPACE_16))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// The page header: title, the honest scope subtitle, and the close action
/// (the local-surface dismissal — same semantics as Escape).
fn page_header<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    language: crate::i18n::Language,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    row![
        column![
            text(i18n::t(language, Key::Containers)).size(f32::from(tokens::FONT_20)),
            text(i18n::t(language, Key::ContainersHeader))
                .size(f32::from(tokens::FONT_11))
                .color(theme::muted_text_color(theme_snapshot)),
        ]
        .spacing(f32::from(tokens::SPACE_2)),
        iced::widget::Space::new().width(Length::Fill),
        crate::focus::modal_close(theme_snapshot),
    ]
    .spacing(f32::from(tokens::SPACE_8))
    .align_y(iced::Alignment::Center)
    .width(Length::Fill)
    .into()
}

fn page_body<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    rollup: Option<&'a ContainerRollup>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    match page_branch(rollup) {
        ContainersPageBranch::Waiting => {
            message_panel(theme_snapshot, t("containers.telemetry_not_collected"))
        }
        ContainersPageBranch::Unsupported => {
            message_panel(theme_snapshot, t("containers.unsupported"))
        }
        ContainersPageBranch::PermissionDenied => {
            message_panel(theme_snapshot, t("containers.permission_denied"))
        }
        ContainersPageBranch::Stale => message_panel(theme_snapshot, t("containers.unavailable")),
        ContainersPageBranch::Empty => column![
            message_panel(theme_snapshot, t("containers.no_containers")),
            text(t("containers.empty_hint"))
                .size(f32::from(tokens::FONT_11))
                .color(theme::muted_text_color(theme_snapshot)),
        ]
        .spacing(f32::from(tokens::SPACE_8))
        .into(),
        // `page_branch` only selects Table for a populated rollup; the
        // fallback keeps the fold panic-free by construction.
        ContainersPageBranch::Table => rollup.map_or_else(
            || message_panel(theme_snapshot, t("containers.telemetry_not_collected")),
            |rollup| container_table(theme_snapshot, rollup),
        ),
    }
}

/// The rollup source seam. The shared `SystemProjectionStore::containers` field lands
/// with the container-rollup lane (parallel shared-layer work); until then
/// the page honestly renders the waiting state.
fn containers_rollup(shell: &taskmanager_shell::ShellApp) -> Option<&ContainerRollup> {
    // Shared shell data (ADR-027): the rollup arrives via the platform batch.
    shell.projection().containers.as_ref()
}

/// Pre-folded display strings for one container row (ARCH.md §8.1 data
/// layer): the telemetry→display fold happens once here; the table helpers
/// only lay out and paint. GPUI `ContainerRowVm` parity.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContainerRowVm {
    /// Container name (grow column).
    pub name: String,
    /// Friendly runtime label or the shared dash.
    pub runtime: String,
    /// `x.y%` or the shared dash — a first-sample gap never renders `0.0%`.
    pub cpu: String,
    /// Decimal memory string or the shared dash.
    pub memory: String,
    /// Member-process count or the shared dash.
    pub processes: String,
}

/// Fold one container summary into its row display strings.
pub(crate) fn container_row_vm(summary: &ContainerSummary) -> ContainerRowVm {
    // One shared dash spelling for uncollected cells (first-sample CPU rate,
    // vanished cgroup memory) — never a fabricated `0.0%` / `0 MB`.
    let cpu = match summary.cpu_percentage.availability() {
        ScalarAvailability::Available | ScalarAvailability::Partial(_) => {
            scalar_text(summary.cpu_percentage, |value| format!("{value:.1}%"))
        }
        _ => missing_value(),
    };
    ContainerRowVm {
        name: summary.name.clone(),
        runtime: summary
            .runtime
            .as_ref()
            .map_or_else(missing_value, |kind| runtime_label(Some(kind)).to_owned()),
        cpu,
        memory: scalar_text(summary.memory_bytes, bytes),
        processes: if summary.member_pids.is_empty() {
            missing_value()
        } else {
            summary.member_pids.len().to_string()
        },
    }
}

fn container_table<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    rollup: &'a ContainerRollup,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    let header = row![
        text(t("containers.name"))
            .size(f32::from(tokens::FONT_11))
            .color(muted)
            .width(Length::Fill),
        text(t("containers.runtime"))
            .size(f32::from(tokens::FONT_11))
            .color(muted)
            .width(Length::Fixed(110.0)),
        text(t("containers.cpu"))
            .size(f32::from(tokens::FONT_11))
            .color(muted)
            .width(Length::Fixed(90.0)),
        text(t("containers.memory"))
            .size(f32::from(tokens::FONT_11))
            .color(muted)
            .width(Length::Fixed(110.0)),
        text(t("containers.processes"))
            .size(f32::from(tokens::FONT_11))
            .color(muted)
            .width(Length::Fixed(90.0)),
    ]
    .spacing(8)
    .padding(4)
    .width(Length::Fill);

    let (shown, hidden) = container_row_window(rollup.containers.len());
    let mut rows: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> = rollup.containers
        [..shown]
        .iter()
        .map(|summary| {
            let vm = container_row_vm(summary);
            let row = row![
                text(vm.name)
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fill),
                text(vm.runtime)
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fixed(110.0)),
                text(vm.cpu)
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fixed(90.0)),
                text(vm.memory)
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fixed(110.0)),
                text(vm.processes)
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fixed(90.0)),
            ]
            .spacing(8)
            .padding(4)
            .width(Length::Fill);
            container(row)
                .style(|_| theme::row_style(theme_snapshot, false, false))
                .width(Length::Fill)
                .into()
        })
        .collect();
    if hidden > 0 {
        rows.push(
            container(text(more_rows_label(hidden)).size(f32::from(tokens::FONT_12)))
                .padding(4)
                .width(Length::Fill)
                .into(),
        );
    }

    // The page body owns the full remaining height (the old modal capped the
    // scroll at a fixed 360px card).
    scrollable(
        column(Some(header.into()).into_iter().chain(rows))
            .spacing(1)
            .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

fn more_rows_label(hidden: usize) -> String {
    t("common.more_rows").replace("{count}", &hidden.to_string())
}

/// Render a typed scalar observation: the value when available, `—`
/// otherwise (a first sample or an unreadable file must never look like a
/// zero). The production cell seam — `container_row_vm` renders through it
/// and the headless tests pin the same function.
fn scalar_text<T: Copy>(observation: ScalarObservation<T>, format: impl Fn(T) -> String) -> String {
    observation
        .current_value()
        .copied()
        .map_or_else(missing_value, format)
}

fn runtime_label(runtime: Option<&IsolationKind>) -> &'static str {
    match runtime {
        Some(IsolationKind::Docker) => "Docker",
        Some(IsolationKind::Podman) => "Podman",
        Some(IsolationKind::Kubernetes) => "Kubernetes",
        Some(IsolationKind::Lxc) => "LXC",
        Some(IsolationKind::SystemdNspawn) => "nspawn",
        Some(IsolationKind::Flatpak) => "Flatpak",
        Some(IsolationKind::Snap) => "Snap",
        Some(IsolationKind::Wsl) => "WSL",
        Some(IsolationKind::OtherContainer) | None => "Container",
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/containers_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/gui/ui/containers_page_tests.rs"]
mod page_tests;
