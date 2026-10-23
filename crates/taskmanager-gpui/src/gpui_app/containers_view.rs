//! Containers page: per-container aggregated CPU + memory rollup.
//!
//! Mirrors the Processes/Services page structure (a header row + data rows in a
//! card surface) but is self-contained — it does not touch the shared process
//! table column code (owned elsewhere). The rollup data comes from
//! [`taskmanager_core::core::ContainerRollup`]; the page only renders it.
//!
//! Honesty contract: an empty list is an explicit, localized "no containers
//! detected" state (never a blank panel), and a typed-unavailable source
//! (cgroup-v1 host, EACCES on the unified mount) renders the typed reason
//! rather than masquerading as an empty system. A CPU% that is still a
//! first-sample gap renders an em dash, not a fabricated `0.0%`.
//!
//! Declared quota (the `quota` submodule): the row's limit column states what
//! the container DECLARED — the observed share against the declared soft
//! memory ceiling, the declared CPU budget, or the typed absence of a
//! declaration. The page never invents a cap: a container that declares
//! nothing is never compared against a limit of the page's own making, and a
//! row that crossed a DECLARED ceiling wears the over-limit marker.

use gpui::{Div, ParentElement, Styled, div, px};
use taskmanager_theme::Weight;
use taskmanager_ui::icons_binding::icon;
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::font_weight;
use taskmanager_ui::theme_binding::hsla;

use taskmanager_core::core::{
    ContainerRollup, ContainerSummary, DeviceStatus, IsolationKind, ScalarAvailability,
};

use crate::gpui_app::elements;
use crate::gpui_app::formatting;
use taskmanager_application::container_row_window;
use taskmanager_application::i18n;
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_theme::tokens;
use taskmanager_theme::{Color, Theme};
use taskmanager_ui::data::row::DataRow;
use taskmanager_ui::primitives::state_panel::StatePanel;
use taskmanager_ui_contract::IconId;

mod quota;

use quota::{ContainerQuotaBreach, container_quota_view, quota_readout};

/// Render the Containers page body (no outer padding — the caller wraps it).
/// The rollup's typed [`DeviceStatus`] drives which branch is taken so a
/// failed source and a genuinely empty host never share copy.
pub fn render_containers(t: &Theme, rollup: &ContainerRollup, units: UnitPreferences) -> Div {
    let body = match rollup.state.status {
        DeviceStatus::Unsupported => typed_message(
            t,
            "tm-containers-state-unsupported",
            i18n::t("containers.unsupported"),
            None,
        ),
        DeviceStatus::PermissionDenied => typed_message(
            t,
            "tm-containers-state-permission",
            i18n::t("containers.permission_denied"),
            None,
        ),
        DeviceStatus::Stale => typed_message(
            t,
            "tm-containers-state-stale",
            i18n::t("containers.unavailable"),
            None,
        ),
        // Healthy covers both a populated list and a genuinely container-free host.
        DeviceStatus::Healthy | DeviceStatus::MissingTool => {
            if rollup.containers.is_empty() {
                typed_message(
                    t,
                    "tm-containers-empty",
                    i18n::t("containers.no_containers"),
                    Some(i18n::t("containers.empty_hint")),
                )
            } else {
                container_list(t, &rollup.containers, units)
            }
        }
    };
    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .gap(definite_length(tokens::SPACE_6))
        .child(header_row(t))
        .child(body)
}

fn header_row(t: &Theme) -> Div {
    row_skeleton(
        t,
        &RowCells {
            name: i18n::t("containers.name"),
            runtime: i18n::t("containers.runtime"),
            cpu: i18n::t("containers.cpu"),
            memory: i18n::t("containers.memory"),
            processes: i18n::t("containers.processes"),
            quota: i18n::t("proc_insights.resource_limits"),
        },
        true,
        None,
        None,
    )
}

/// The fixed column widths of the containers row, in px. The name column
/// takes the remaining space; every fixed column may shrink (and truncate)
/// before the row could leave the page's content box.
const RUNTIME_COLUMN_WIDTH: f32 = 88.0;
const CPU_COLUMN_WIDTH: f32 = 72.0;
const MEMORY_COLUMN_WIDTH: f32 = 92.0;
const PROCESSES_COLUMN_WIDTH: f32 = 76.0;
const QUOTA_COLUMN_WIDTH: f32 = 116.0;

/// The row's cell strings, in display order. The header row and every data
/// row share ONE column order, so the columns stay aligned.
struct RowCells<'a> {
    name: &'a str,
    runtime: &'a str,
    cpu: &'a str,
    memory: &'a str,
    processes: &'a str,
    quota: &'a str,
}

fn container_list(t: &Theme, containers: &[ContainerSummary], units: UnitPreferences) -> Div {
    let (shown, hidden) = container_row_window(containers.len());
    let mut list = div().flex().flex_col();
    for (index, container) in containers[..shown].iter().enumerate() {
        list = list.child(with_row_selector(
            row_for(t, container, units, index),
            index,
        ));
    }
    if hidden > 0 {
        list = list.child(with_more_hint_selector(elements::more_rows_hint(t, hidden)));
    }
    list
}

/// Geometry breakpoint per data row — the render-path assertions count these
/// to prove the list paints one bounded row per materialized container and
/// stops exactly at the shared row-window bound. Noop outside test support.
#[cfg(any(test, feature = "test-support"))]
fn with_row_selector(row: Div, index: usize) -> Div {
    use gpui::InteractiveElement;
    row.debug_selector(move || format!("tm-containers-row:{index}"))
}

#[cfg(not(any(test, feature = "test-support")))]
fn with_row_selector(row: Div, _index: usize) -> Div {
    row
}

/// Geometry breakpoint for the "+N more" overflow line.
#[cfg(any(test, feature = "test-support"))]
fn with_more_hint_selector(hint: Div) -> Div {
    use gpui::InteractiveElement;
    hint.debug_selector(|| "tm-containers-more".to_string())
}

#[cfg(not(any(test, feature = "test-support")))]
fn with_more_hint_selector(hint: Div) -> Div {
    hint
}

/// Pre-folded display strings for one container row (ARCH.md §8.1 data
/// layer): the telemetry→display fold happens once here; the `row_for` /
/// `row_skeleton` render helpers only lay out and paint. No theme or gpui
/// types.
pub struct ContainerRowVm {
    /// Container name (grow column).
    pub name: String,
    /// Friendly runtime label (`runtime_label`) or the shared dash.
    pub runtime: String,
    /// `x.y%` or the shared dash — a first-sample gap never renders `0.0%`.
    pub cpu: String,
    /// Decimal memory string or the shared dash.
    pub memory: String,
    /// Member-process count or the shared dash.
    pub processes: String,
    /// The declared-quota readout: the observed share against the ceiling the
    /// container DECLARED, the declared CPU budget, or the typed absence of
    /// a declaration ("not declared" / "unknown"). Never a percentage for a
    /// container that declared nothing.
    pub quota: String,
}

/// Fold one [`ContainerSummary`] into its row display strings, mirroring the
/// exact formatter/dash conventions the inline render path used.
pub fn container_row_vm(container: &ContainerSummary, units: UnitPreferences) -> ContainerRowVm {
    // One shared dash spelling for uncollected cells (first-sample CPU rate,
    // vanished cgroup memory) — never a fabricated `0.0%` / `0 MB`, and the
    // memory cell no longer borrows the CPU-specific pending key.
    let cpu = match container.cpu_percentage.availability() {
        ScalarAvailability::Available | ScalarAvailability::Partial(_) => container
            .cpu_percentage
            .current_value()
            .map(|value| format!("{value:.1}%"))
            .unwrap_or_else(formatting::missing_value),
        // First-sample gap, vanished cgroup, or a read failure: never `0.0%`.
        _ => formatting::missing_value(),
    };
    ContainerRowVm {
        name: container.name.clone(),
        runtime: container
            .runtime
            .as_ref()
            .map(runtime_label)
            .unwrap_or_else(formatting::missing_value),
        cpu,
        memory: container
            .memory_bytes
            .current_value()
            .map(|bytes| units.format_quantity(*bytes, QuantityFamily::Memory, false))
            .unwrap_or_else(formatting::missing_value),
        processes: if container.member_pids.is_empty() {
            formatting::missing_value()
        } else {
            container.member_pids.len().to_string()
        },
        quota: quota_readout(&container_quota_view(container)),
    }
}

/// Render one container row.
///
/// `breach` is the declared dimension the observation crossed, if any: a row
/// that crossed nothing wears no over-limit marker.
fn row_for(t: &Theme, container: &ContainerSummary, units: UnitPreferences, index: usize) -> Div {
    let vm = container_row_vm(container, units);
    let breach = container_quota_view(container).breach();
    row_skeleton(
        t,
        &RowCells {
            name: &vm.name,
            runtime: &vm.runtime,
            cpu: &vm.cpu,
            memory: &vm.memory,
            processes: &vm.processes,
            quota: &vm.quota,
        },
        false,
        Some(index),
        breach,
    )
}

/// One six-column row. The header variant dims the text and bolds it; data
/// rows use the card surface so the list reads as a table without depending on
/// the shared (process-owned) table primitive.
///
/// `index` names the row for the geometry breakpoints (the header row has
/// none); `breach` raises the row's over-limit state.
fn row_skeleton(
    t: &Theme,
    cells: &RowCells<'_>,
    is_header: bool,
    index: Option<usize>,
    breach: Option<ContainerQuotaBreach>,
) -> Div {
    let bg = if is_header {
        t.sidebar_bg
    } else {
        t.sidebar_card_bg
    };
    let fg = if is_header { t.fg_dim } else { t.fg };
    let weight = if is_header {
        tokens::FONT_WEIGHT_BOLD
    } else {
        tokens::FONT_WEIGHT_NORMAL
    };
    // The over-limit state: the readout wears the warning ink, and a run that
    // went twice past the declared ceiling takes the heavier mark.
    let quota_fg = if breach.is_some() { t.warning } else { fg };
    let quota_weight = if breach.is_some_and(ContainerQuotaBreach::is_deep_overrun) {
        tokens::FONT_WEIGHT_BOLD
    } else {
        weight
    };
    DataRow::new(t.palette())
        .background(bg)
        .radius(tokens::control_radius(t))
        .child(name_cell(
            t,
            cells.name,
            fg,
            weight,
            index,
            breach.is_some(),
        ))
        .child(fixed_cell(cells.runtime, RUNTIME_COLUMN_WIDTH, fg, weight))
        .child(fixed_cell(cells.cpu, CPU_COLUMN_WIDTH, fg, weight))
        .child(fixed_cell(cells.memory, MEMORY_COLUMN_WIDTH, fg, weight))
        .child(fixed_cell(
            cells.processes,
            PROCESSES_COLUMN_WIDTH,
            fg,
            weight,
        ))
        .child(quota_cell(cells.quota, quota_fg, quota_weight, index))
        .render()
}

/// The name cell: the container's identity in its own bounded slot, preceded
/// by the over-limit marker when the row crossed a DECLARED ceiling.
fn name_cell(
    t: &Theme,
    label: &str,
    fg: Color,
    weight: Weight,
    index: Option<usize>,
    over_limit: bool,
) -> Div {
    let mut cell = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .flex_1()
        .min_w(px(0.0))
        .text_size(font_size(tokens::FONT_12))
        .text_color(hsla(fg))
        .font_weight(font_weight(weight));
    if over_limit && let Some(index) = index {
        cell = cell.child(over_limit_marker(t, index));
    }
    cell.child(
        div()
            .flex_1()
            .min_w(px(0.0))
            .truncate()
            .child(label.to_string()),
    )
}

/// The over-limit marker: the triangle that marks a row whose observed usage
/// crossed a ceiling the container declared. It is painted ONLY for such a
/// row — an undeclared or unreadable quota never wears it.
fn over_limit_marker(t: &Theme, index: usize) -> Div {
    use gpui::InteractiveElement;
    div()
        .flex_shrink_0()
        .child(
            icon(IconId::TriangleAlert)
                .size(px(12.0))
                .text_color(hsla(t.warning)),
        )
        .debug_selector(move || format!("tm-containers-over-limit:{index}"))
}

/// The quota cell: the row's declared-limit readout in its own bounded slot.
fn quota_cell(label: &str, fg: Color, weight: Weight, index: Option<usize>) -> Div {
    let cell = fixed_cell(label, QUOTA_COLUMN_WIDTH, fg, weight);
    match index {
        Some(index) => {
            use gpui::InteractiveElement;
            cell.debug_selector(move || format!("tm-containers-quota:{index}"))
        }
        None => cell,
    }
}

/// One bounded fixed-width cell: the value is clipped inside its own slot so
/// a long reading can never push the row past the page's content box.
fn fixed_cell(label: &str, width: f32, fg: Color, weight: Weight) -> Div {
    div()
        .w(px(width))
        .flex_shrink(1.0)
        .min_w(px(0.0))
        .overflow_hidden()
        .truncate()
        .text_size(font_size(tokens::FONT_12))
        .text_color(hsla(fg))
        .font_weight(font_weight(weight))
        .child(label.to_string())
}

/// Centered typed empty/unavailable message with an optional secondary hint.
/// Mirrors [`crate::gpui_app::list_view::unavailable_state`] but localized for
/// the containers domain. `state_selector` is the per-branch geometry
/// breakpoint (test support) so render-path assertions can prove WHICH typed
/// branch painted — a failed source and an empty host must never share copy.
fn typed_message(
    t: &Theme,
    state_selector: &'static str,
    primary: &str,
    secondary: Option<&str>,
) -> Div {
    let mut panel =
        StatePanel::new(IconId::Applications, primary.to_owned(), t.palette()).tone(t.warning);
    if let Some(hint) = secondary {
        panel = panel.detail(hint.to_owned());
    }
    let rendered = panel.render();
    #[cfg(any(test, feature = "test-support"))]
    let rendered = {
        use gpui::InteractiveElement;
        rendered.debug_selector(move || state_selector.to_owned())
    };
    #[cfg(not(any(test, feature = "test-support")))]
    let _ = state_selector;
    rendered
}

/// Friendly runtime label for the runtime column. Kept short so the fixed
/// 96 px column does not truncate the common cases.
fn runtime_label(kind: &IsolationKind) -> String {
    match kind {
        IsolationKind::Docker => "Docker",
        IsolationKind::Podman => "Podman",
        IsolationKind::Kubernetes => "Kubernetes",
        IsolationKind::Lxc => "LXC",
        IsolationKind::SystemdNspawn => "nspawn",
        IsolationKind::Flatpak => "Flatpak",
        IsolationKind::Snap => "Snap",
        IsolationKind::Wsl => "WSL",
        IsolationKind::OtherContainer => "Container",
    }
    .to_string()
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_containers_view_tests.rs"]
mod tests;
