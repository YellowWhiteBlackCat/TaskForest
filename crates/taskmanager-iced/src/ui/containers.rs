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
//!
//! Declared quota (the `quota` submodule): the row's limit column states what
//! the container DECLARED — the observed share against the declared soft
//! memory ceiling, the declared CPU budget, or the typed absence of a
//! declaration. The page never invents a cap: a container that declares
//! nothing is never compared against a limit of the page's own making, and a
//! row that crossed a DECLARED ceiling wears the warning ink.

use iced::widget::canvas::{self, Geometry, Path, Stroke};
use iced::widget::{column, container, row, text};
use iced::{Element, Length, Point, Rectangle, Size};
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

use super::components::{message_panel, styled_scrollable};
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::{bytes, missing_value};
use taskmanager_theme::Theme;

mod quota;

use quota::{ContainerQuotaBreach, container_quota_view, quota_readout};

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

/// The neutral empty-state illustration's motif — which vector glyph the
/// empty state draws. The two motifs share one drawing program and one
/// neutral palette, so the Containers and Users empty states read as one
/// family instead of two unrelated decorations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EmptyStateMotif {
    /// Nested containers: the sandbox boundary with its inner process block.
    ContainerStack,
    /// An empty seat: the session glyph with nobody logged in.
    EmptySession,
}

/// One shape of the empty-state illustration, in the illustration's own
/// normalized space (0..1 on both axes). The geometry is a pure projection
/// so the headless tests pin the SHAPES, never host pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum IllustrationShape {
    /// A rounded-rectangle outline.
    RoundedRect {
        /// Left edge (0..1).
        x: f32,
        /// Top edge (0..1).
        y: f32,
        /// Width (0..1).
        width: f32,
        /// Height (0..1).
        height: f32,
        /// Corner radius (0..1 of the illustration's short side).
        radius: f32,
    },
    /// A filled circle.
    Circle {
        /// Center x (0..1).
        x: f32,
        /// Center y (0..1).
        y: f32,
        /// Radius (0..1 of the illustration's short side).
        radius: f32,
    },
}

/// The neutral empty-state illustration's shapes for one motif.
///
/// Every shape stays inside the normalized bounds and carries a real extent
/// (no zero-sized or out-of-frame geometry), so the illustration reads as a
/// deliberate vector glyph on every density.
#[must_use]
pub(crate) fn empty_state_shapes(motif: EmptyStateMotif) -> Vec<IllustrationShape> {
    match motif {
        EmptyStateMotif::ContainerStack => vec![
            IllustrationShape::RoundedRect {
                x: 0.10,
                y: 0.20,
                width: 0.80,
                height: 0.60,
                radius: 0.10,
            },
            IllustrationShape::RoundedRect {
                x: 0.24,
                y: 0.38,
                width: 0.52,
                height: 0.26,
                radius: 0.06,
            },
            IllustrationShape::Circle {
                x: 0.50,
                y: 0.51,
                radius: 0.06,
            },
        ],
        EmptyStateMotif::EmptySession => vec![
            IllustrationShape::RoundedRect {
                x: 0.14,
                y: 0.22,
                width: 0.72,
                height: 0.56,
                radius: 0.10,
            },
            IllustrationShape::Circle {
                x: 0.50,
                y: 0.42,
                radius: 0.10,
            },
            IllustrationShape::RoundedRect {
                x: 0.33,
                y: 0.56,
                width: 0.34,
                height: 0.14,
                radius: 0.07,
            },
        ],
    }
}

/// The empty-state illustration's canvas side (a layout contract, not a
/// spacing token).
pub(crate) const EMPTY_STATE_ILLUSTRATION_SIZE: f32 = 96.0;

/// The compact-density illustration side.
pub(crate) const EMPTY_STATE_ILLUSTRATION_SIZE_COMPACT: f32 = 64.0;

/// The empty-state illustration's side for one density: the compact viewport
/// keeps the glyph small so the message stays above the fold.
#[must_use]
pub(crate) fn empty_state_illustration_size(compact: bool) -> f32 {
    if compact {
        EMPTY_STATE_ILLUSTRATION_SIZE_COMPACT
    } else {
        EMPTY_STATE_ILLUSTRATION_SIZE
    }
}

/// The neutral vector empty-state illustration: the motif's shapes stroked
/// in the skin's muted foreground and filled with a faint accent wash. Both
/// colors are palette tokens, so every skin renders its own illustration.
struct EmptyStateIllustration {
    motif: EmptyStateMotif,
    stroke: iced::Color,
    fill: iced::Color,
}

impl iced::widget::canvas::Program<Message> for EmptyStateIllustration {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let size = bounds.size();
        let short_side = size.width.min(size.height);
        let mut frame = canvas::Frame::new(renderer, size);
        for shape in empty_state_shapes(self.motif) {
            match shape {
                IllustrationShape::RoundedRect {
                    x,
                    y,
                    width,
                    height,
                    radius,
                } => {
                    let path = Path::rounded_rectangle(
                        Point::new(x * size.width, y * size.height),
                        Size::new(width * size.width, height * size.height),
                        (radius * short_side).into(),
                    );
                    frame.stroke(
                        &path,
                        Stroke::default().with_width(1.5).with_color(self.stroke),
                    );
                }
                IllustrationShape::Circle { x, y, radius } => {
                    let path = Path::circle(
                        Point::new(x * size.width, y * size.height),
                        radius * short_side,
                    );
                    frame.fill(&path, self.fill);
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

/// The neutral vector empty-state illustration for one motif, sized
/// `size` px square. Lives with the Containers page (the first page that
/// needed the shared empty state); the Users page reuses the SAME drawing
/// program so both empty states stay one family.
pub(crate) fn empty_state_illustration<'a>(
    theme_snapshot: &'a Theme,
    motif: EmptyStateMotif,
    size: f32,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let palette = theme_snapshot.palette();
    let mut fill = crate::theme_binding::color(palette.accent);
    fill.a = 0.22;
    canvas::Canvas::new(EmptyStateIllustration {
        motif,
        stroke: crate::theme_binding::color(palette.fg_muted),
        fill,
    })
    .width(size)
    .height(size)
    .into()
}

/// How many member-process labels the inset panel shows before the rest
/// collapses into a counted "+N".
pub(crate) const MAX_MEMBER_PROCESS_LABELS: usize = 3;

/// The member-process column width in px (a layout contract, not a token).
pub(crate) const MEMBER_PROCESS_COLUMN_WIDTH: f32 = 150.0;

/// The container's member-process block: the bounded labels the inset panel
/// shows plus how many members it did NOT show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemberProcessBlock {
    /// The member-process labels the panel shows (bounded).
    pub labels: Vec<String>,
    /// How many members the panel collapsed into its "+N" suffix.
    pub hidden: usize,
}

/// The member-process block for one container summary.
///
/// Membership that could not be enumerated (an empty member list) projects
/// to NO block: the panel is never drawn around a fabricated empty list,
/// and the cell keeps its honest dash.
#[must_use]
pub(crate) fn member_process_block(summary: &ContainerSummary) -> Option<MemberProcessBlock> {
    if summary.member_pids.is_empty() {
        return None;
    }
    let labels = summary
        .member_pids
        .iter()
        .take(MAX_MEMBER_PROCESS_LABELS)
        .map(|pid| pid.to_string())
        .collect();
    Some(MemberProcessBlock {
        labels,
        hidden: summary
            .member_pids
            .len()
            .saturating_sub(MAX_MEMBER_PROCESS_LABELS),
    })
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
    theme_snapshot: &'a Theme,
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
    theme_snapshot: &'a Theme,
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
            empty_state_illustration(
                theme_snapshot,
                EmptyStateMotif::ContainerStack,
                empty_state_illustration_size(false),
            ),
            message_panel(theme_snapshot, t("containers.no_containers")),
            text(t("containers.empty_hint"))
                .size(f32::from(tokens::FONT_11))
                .color(theme::muted_text_color(theme_snapshot)),
        ]
        .spacing(f32::from(tokens::SPACE_8))
        .align_x(iced::Alignment::Center)
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
fn containers_rollup(shell: &ShellApp) -> Option<&ContainerRollup> {
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
    /// The member-process block the inset panel draws. `None` when the
    /// membership could not be enumerated — the cell then keeps the honest
    /// dash instead of an empty panel.
    pub member_block: Option<MemberProcessBlock>,
    /// The declared-quota readout: the observed share against the ceiling the
    /// container DECLARED, the declared CPU budget, or the typed absence of
    /// a declaration ("not declared" / "unknown"). Never a percentage for a
    /// container that declared nothing.
    pub quota: String,
    /// Whether the observed usage crossed a ceiling the container DECLARED.
    /// Only such a row wears the over-limit state; a row with no declaration
    /// (or an unreadable one) never does.
    pub over_limit: bool,
    /// Whether the overrun ran twice past the declared soft ceiling — the
    /// deeper step the readout takes.
    pub deep_overrun: bool,
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
    let quota = container_quota_view(summary);
    let breach = quota.breach();
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
        member_block: member_process_block(summary),
        quota: quota_readout(&quota),
        over_limit: breach.is_some(),
        deep_overrun: breach.is_some_and(ContainerQuotaBreach::is_deep_overrun),
    }
}

/// The container's member-process cell: the recessed inset panel that wraps
/// the container's own process block (the sandbox boundary reads as a well
/// inside the page), or the honest dash when the membership could not be
/// enumerated.
fn member_process_cell<'a>(
    theme_snapshot: &'a Theme,
    vm: &ContainerRowVm,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let Some(block) = &vm.member_block else {
        return text(vm.processes.clone())
            .size(f32::from(tokens::FONT_12))
            .width(Length::Fixed(MEMBER_PROCESS_COLUMN_WIDTH))
            .into();
    };
    let mut labels: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = block
        .labels
        .iter()
        .map(|label| {
            text(label.clone())
                .size(f32::from(tokens::FONT_10))
                .font(crate::theme_binding::mono_font(theme_snapshot))
                .into()
        })
        .collect();
    if block.hidden > 0 {
        labels.push(
            text(format!("+{}", block.hidden))
                .size(f32::from(tokens::FONT_10))
                .color(theme::muted_text_color(theme_snapshot))
                .into(),
        );
    }
    container(row(labels).spacing(4))
        .style(move |_| theme::inset_panel_style(theme_snapshot))
        .padding([2, 6])
        .width(Length::Fixed(MEMBER_PROCESS_COLUMN_WIDTH))
        .into()
}

/// The quota column's width in px (a layout contract, not a token): the
/// declared-limit readout ("150.0%", "1.5/1.0 CPU", "Not declared") keeps its
/// own bounded slot, so a long reading never widens the row.
const QUOTA_COLUMN_WIDTH: f32 = 120.0;

fn container_table<'a>(
    theme_snapshot: &'a Theme,
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
            .width(Length::Fixed(MEMBER_PROCESS_COLUMN_WIDTH)),
        text(t("proc_insights.resource_limits"))
            .size(f32::from(tokens::FONT_11))
            .color(muted)
            .width(Length::Fixed(QUOTA_COLUMN_WIDTH)),
    ]
    .spacing(8)
    .padding(4)
    .width(Length::Fill);

    let warning = crate::theme_binding::color(theme_snapshot.palette().warning);
    let (shown, hidden) = container_row_window(rollup.containers.len());
    let mut rows: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> = rollup.containers
        [..shown]
        .iter()
        .map(|summary| {
            let vm = container_row_vm(summary);
            let member_cell = member_process_cell(theme_snapshot, &vm);
            let name = text(vm.name).size(f32::from(tokens::FONT_12));
            // The over-limit state rides the row: only a row that crossed a
            // DECLARED ceiling wears the warning ink.
            let name = if vm.over_limit {
                name.color(warning)
            } else {
                name
            };
            let quota = text(vm.quota).size(f32::from(tokens::FONT_12));
            // The over-limit state rides the limit readout: a row that
            // crossed a DECLARED ceiling wears the warning ink (the same
            // shape the GPUI reference and the TUI use), and a run that went
            // twice past the ceiling takes the heavier mark. The percentage
            // itself stays unclamped.
            let quota = if vm.over_limit {
                quota.color(warning)
            } else {
                quota
            };
            let quota = if vm.deep_overrun {
                quota.font(crate::theme_binding::ui_font_weight(
                    theme_snapshot,
                    tokens::FONT_WEIGHT_BOLD,
                ))
            } else {
                quota
            };
            let row = row![
                name.width(Length::Fill),
                text(vm.runtime)
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fixed(110.0)),
                text(vm.cpu)
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fixed(90.0)),
                text(vm.memory)
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fixed(110.0)),
                member_cell,
                quota.width(Length::Fixed(QUOTA_COLUMN_WIDTH)),
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
    styled_scrollable(
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

#[cfg(test)]
#[path = "../../tests/gui/ui/containers_inset_tests.rs"]
mod inset_tests;

#[cfg(test)]
#[path = "../../tests/gui/ui/empty_state_illustration_tests.rs"]
mod illustration_tests;
