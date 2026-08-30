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

use gpui::{Div, ParentElement, Styled, div, px};

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
        .gap(tokens::SPACE_6)
        .child(header_row(t))
        .child(body)
}

fn header_row(t: &Theme) -> Div {
    row_skeleton(
        t,
        i18n::t("containers.name"),
        i18n::t("containers.runtime"),
        i18n::t("containers.cpu"),
        i18n::t("containers.memory"),
        i18n::t("containers.processes"),
        true,
    )
}

fn container_list(t: &Theme, containers: &[ContainerSummary], units: UnitPreferences) -> Div {
    let (shown, hidden) = container_row_window(containers.len());
    let mut list = div().flex().flex_col();
    for (index, container) in containers[..shown].iter().enumerate() {
        list = list.child(with_row_selector(row_for(t, container, units), index));
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
    }
}

fn row_for(t: &Theme, container: &ContainerSummary, units: UnitPreferences) -> Div {
    let vm = container_row_vm(container, units);
    row_skeleton(
        t,
        &vm.name,
        &vm.runtime,
        &vm.cpu,
        &vm.memory,
        &vm.processes,
        false,
    )
}

/// One five-column row. The header variant dims the text and bolds it; data
/// rows use the card surface so the list reads as a table without depending on
/// the shared (process-owned) table primitive.
fn row_skeleton(
    t: &Theme,
    name: &str,
    runtime: &str,
    cpu: &str,
    memory: &str,
    processes: &str,
    is_header: bool,
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
    let cell = |label: &str, grow: bool| {
        let mut cell = div()
            .text_size(tokens::FONT_12)
            .text_color(fg)
            .font_weight(weight.into())
            .min_w(px(0.0));
        if grow {
            cell = cell.flex_1();
        }
        cell.child(label.to_string())
    };
    DataRow::new(t.palette())
        .background(bg)
        .radius(tokens::control_radius(t))
        .child(cell(name, true))
        .child(fixed_cell(runtime, 96.0, fg, weight))
        .child(fixed_cell(cpu, 80.0, fg, weight))
        .child(fixed_cell(memory, 96.0, fg, weight))
        .child(fixed_cell(processes, 84.0, fg, weight))
        .render()
}

fn fixed_cell(label: &str, width: f32, fg: Color, weight: taskmanager_theme::Weight) -> Div {
    div()
        .w(px(width))
        .min_w(px(0.0))
        .text_size(tokens::FONT_12)
        .text_color(fg)
        .font_weight(weight.into())
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
