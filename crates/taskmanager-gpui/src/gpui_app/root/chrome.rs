//! Titlebar chrome, tooltip labels, and the Properties dialog body.
//!
//! These are pure UI helper functions that take all needed state as parameters — no
//! `self` on `RootView`, so they live here instead of in the main `root.rs` impl blocks.
//!
//! Page navigation used to live in `top_bar`; it has been extracted into
//! `super::nav::nav_strip` (a horizontal tab row rendered BELOW the titlebar in
//! both decoration modes). `top_bar` now holds ONLY window chrome: a drag handle,
//! the window title, and the per-platform window controls (CSD fallback path).

use super::{Hover, RootView};
use gpui::{
    Context, Div, Entity, InteractiveElement, MouseButton, ParentElement, Rgba, Styled, div, px,
};

use crate::gpui_app::chrome;
use crate::gpui_app::elements;
use crate::gpui_app::formatting::missing_value;
use crate::gpui_app::process_insights::{
    ProcessInsightsLabels, ProcessInsightsRenderState, render_process_insights,
};
use crate::gpui_app::sidebar::{SelectedDevice, network_category_label};
use taskmanager_application::i18n;
use taskmanager_application::process_details_vm::{
    DetailValue, ProcessDetailsField, ProcessDetailsRowVm, detail_value,
};
use taskmanager_assets::product;
use taskmanager_core::core::SystemSnapshot;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_theme::tokens;
use taskmanager_theme::{Theme, WindowControls, WindowCorner};
use taskmanager_ui::data::key_value_row::KeyValueRow;

/// The CSD titlebar: drag handle + centered window title + per-platform window
/// controls. macOS puts traffic-lights on the left; GNOME/KDE/Windows put
/// controls on the right. Page navigation no longer lives here — it moved to
/// `super::nav::nav_strip`, which the renderer places BELOW this titlebar. This
/// bar is rendered ONLY in the compositor-forced-CSD fallback (when the
/// compositor grants `Decorations::Server`, the native titlebar owns the chrome
/// and this bar is not rendered at all).
///
/// Window-activation awareness (Zed #2610): the 1px bottom border is read
/// through `elements::titlebar_border`, which dims it while the window is
/// inactive. The activation flag comes straight from the platform window each
/// frame — `App::active_window()` (gpui window.rs:936) returns the handle of
/// the OS-focused window of this app (None when nothing of ours is focused),
/// and `AnyWindowHandle::update` re-reads `Window::is_window_active`
/// (window.rs:1721) off that window. Unknown platform answers fall back to
/// "active" so the baseline look never flickers.
pub fn top_bar(
    t: &Theme,
    hovered: Option<&Hover>,
    tray_available: bool,
    cx: &mut Context<RootView>,
) -> Div {
    let window_active = cx
        .active_window()
        .and_then(|handle| {
            handle
                .update(cx, |_, window, _| window.is_window_active())
                .ok()
        })
        .unwrap_or(true);

    let mut bar = div()
        .h(px(chrome::titlebar_height(t)))
        .flex()
        .flex_row()
        .items_center()
        .bg(t.sidebar_bg)
        .border_b_1()
        .border_color(elements::titlebar_border(t, window_active))
        // Round the titlebar's two TOP corners: it spans the full window width
        // and would otherwise paint square pixels into the transparent CSD
        // corners (bottom corners sit inside the window, no rounding needed).
        // Per-corner radii collapse to 0 when the window is maximized, tiled
        // at the top, or fullscreen — matching native CSD behavior. This bar is
        // only rendered in the CSD fallback, so the rounding always composites
        // against the transparent Linux CSD surface (server-decorated windows
        // never reach this path).
        .rounded_tl(px(t.window_corner_radius(WindowCorner::TopLeft)))
        .rounded_tr(px(t.window_corner_radius(WindowCorner::TopRight)));

    // Centered, draggable title region (flex_1 fills the space between the
    // control groups). Left mouse-down starts a window move (the CSD titlebar
    // is the drag affordance); the title text sits centered in it.
    let title = product::GPUI_NAME.to_owned();
    let drag_title = div()
        .id("tl-drag")
        // Zero-cost debug tag (no-op in release) so render tests can assert the
        // app titlebar renders ONLY in the CSD fallback (absent when Server
        // decorations are granted).
        .debug_selector(|| "tl-drag".to_string())
        .flex_1()
        .h_full()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .justify_center()
        .on_mouse_down(MouseButton::Left, |_ev, window, _cx| {
            window.start_window_move();
        })
        .child(
            div()
                .text_size(tokens::FONT_13)
                .text_color(t.fg_dim)
                .child(title),
        );

    match t.window_controls {
        WindowControls::TrafficLight => {
            // macOS: traffic lights LEFT, drag+title fills the rest.
            bar = bar
                .child(chrome::traffic_lights(t, hovered, tray_available, cx))
                .child(drag_title);
        }
        _ => {
            // Win/KDE/GNOME: small left inset, drag+title center, controls RIGHT.
            bar = bar.child(div().w(px(12.0))).child(drag_title).child(
                chrome::window_controls_right(t, hovered, tray_available, cx),
            );
        }
    }
    bar
}

/// Friendly display name for a sidebar device slot, shown as its cursor-following
/// tooltip. Mirrors the sidebar row headings; out-of-range indices (e.g. a device
/// removed between ticks) fall back to a generic label.
pub fn device_label(dev: SelectedDevice, snap: &SystemSnapshot) -> String {
    match dev {
        SelectedDevice::Cpu => snap
            .cpu
            .brand
            .as_deref()
            .map(str::trim)
            .filter(|brand| !brand.is_empty())
            .map_or_else(missing_value, str::to_string),
        SelectedDevice::Memory => i18n::t("common.memory").to_string(),
        SelectedDevice::Disk(i) => snap
            .disks
            .get(i)
            .map(|d| {
                format!(
                    "{} ({})",
                    i18n::t("sidebar.drive"),
                    d.name.trim_start_matches("/dev/")
                )
            })
            .unwrap_or_else(|| format!("{} #{}", i18n::t("sidebar.drive"), i)),
        SelectedDevice::Nic(i) => snap
            .networks
            .get(i)
            .map(|n| {
                format!(
                    "{} ({})",
                    network_category_label(n.adapter_type()),
                    n.interface_name
                )
            })
            .unwrap_or_else(|| format!("{} #{}", i18n::t("sidebar.network"), i)),
        SelectedDevice::Gpu(i) => match snap.gpu.get(i) {
            Some(g) if !g.brand.is_empty() => {
                format!("{} {} \u{2014} {}", i18n::t("common.gpu"), i, g.brand)
            }
            _ => format!("{} {}", i18n::t("common.gpu"), i),
        },
        SelectedDevice::Battery(i) => format!("{} {}", i18n::t("common.battery"), i),
        SelectedDevice::Fan(i) => format!("{} {}", i18n::t("common.fan"), i + 1),
    }
}

/// Friendly tooltip label for a static chrome/control id. Page tabs carry an Alt+N
/// hint tying into keyboard shortcuts; window controls + settings get plain labels.
/// Unknown ids return `None` (no tooltip rendered).
///
/// The match arms key on the **stable English identity** string (the same literal
/// `tab` / `gear_btn` / window-control helpers publish via [`Hover::Static`]),
/// so hover→tooltip resolution is locale-independent; the returned text resolves
/// through [`i18n::t`] against the active language and re-translates on the next
/// frame when the user switches language. (`tab` and `gear_btn` live in
/// `super::nav`; the identity literals are stable across the move.)
pub fn static_label(id: &'static str) -> Option<&'static str> {
    match id {
        "Performance" => Some(i18n::t("tooltip.performance")),
        "Apps" => Some(i18n::t("tooltip.apps")),
        "Services" => Some(i18n::t("tooltip.services")),
        "System" => Some(i18n::t("tooltip.system")),
        "Startup" => Some(i18n::t("tooltip.startup")),
        "Users" => Some(i18n::t("tooltip.users")),
        "Containers" => Some(i18n::t("tooltip.containers")),
        "settings-btn" => Some(i18n::t("chrome.settings")),
        "nav-orientation-btn" => Some(i18n::t("chrome.toggle_orientation")),
        "tl-close" | "wnd-close" => Some(i18n::t("chrome.close")),
        "tl-min" | "wnd-min" => Some(i18n::t("chrome.minimize")),
        "tl-zoom" | "wnd-max" => Some(i18n::t("chrome.maximize")),
        // Any `tooltip.*` i18n key resolves to its own localized text, so action
        // buttons can publish a stable tooltip key without a per-id arm.
        _ if id.starts_with("tooltip.") => Some(i18n::t(id)),
        _ => None,
    }
}

// ── process Properties (details) dialog content ───────────────────────────────
// Two leaf helpers used by the Properties modal: [`prop_row`] renders one
// label/value line, [`details_panel_content`] stacks them for a ProcessItem. NO
// outer chrome box / title — the wrapping `elements::dialog_overlay` /
// `gpui_component::dialog::Dialog` supplies those (mirrors the affinity dialog in
// processes_view.rs).

/// One label/value row for the Properties dialog: label (fg_dim, fixed 110px) +
/// value (fg, fills the rest). `text_size(13)` per the Properties dialog spec.
/// The value column is `min_w(0)` so long command lines wrap within the dialog
/// instead of overflowing it.
pub fn prop_row(t: &Theme, label: &str, value: String) -> Div {
    KeyValueRow::new(label, value, t.palette())
        .label_width(taskmanager_theme::Length(110.0))
        .value_align_right(false)
        .selectable_value(gpui::ElementId::Name(
            format!("process-property-value:{label}").into(),
        ))
        .render()
}

/// Localized `"{label}: {value}"` join (full-width colon in zh) — the single
/// spelling for legend-style pairs instead of hardcoded ASCII colons.
fn kv_label_value(label_key: &'static str, value: &str) -> String {
    i18n::t("common.kv")
        .replace("{label}", i18n::t(label_key))
        .replace("{value}", value)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProcessDetailsSection {
    #[default]
    Overview,
    Performance,
    Command,
    Insights,
}

impl ProcessDetailsSection {
    const ALL: [Self; 4] = [
        Self::Overview,
        Self::Performance,
        Self::Command,
        Self::Insights,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Overview => i18n::t("prop.overview"),
            Self::Performance => i18n::t("prop.performance"),
            Self::Command => i18n::t("prop.command"),
            Self::Insights => i18n::t("prop.insights"),
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Overview => "properties-overview",
            Self::Performance => "properties-performance",
            Self::Command => "properties-command",
            Self::Insights => "properties-insights",
        }
    }
}

fn details_section_tabs(
    t: &Theme,
    active: ProcessDetailsSection,
    entity: &Entity<RootView>,
) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(tokens::SPACE_6)
        .mb(tokens::SPACE_6);
    for section in ProcessDetailsSection::ALL {
        let click_entity = entity.clone();
        row = row.child(elements::pill(
            t,
            section.id(),
            section.label(),
            section == active,
            false,
            move |_window, cx| {
                click_entity.update(cx, |view, cx| {
                    view.details_section = section;
                    cx.notify();
                });
            },
            |_, _, _| {},
        ));
    }
    row
}

/// One full-width 60-second resource graph with explicit current, peak, and
/// unit metadata. Values are formatted by the caller while the original f32
/// series feeds the sparkline unchanged.
fn prop_history_graph(
    t: &Theme,
    label: &str,
    current: String,
    peak: String,
    unit: &str,
    samples: &std::rc::Rc<[f32]>,
    color: Rgba,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_4)
        .p(tokens::SPACE_6)
        .rounded(tokens::control_radius(t))
        .bg(t.sidebar_card_bg)
        .child(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .child(
                    div()
                        .text_size(tokens::FONT_12)
                        .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                        .text_color(t.fg)
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .text_size(tokens::FONT_11)
                        .text_color(t.fg_dim)
                        .child(i18n::t("prop.last_60_seconds")),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap(tokens::SPACE_12)
                .text_size(tokens::FONT_11)
                .text_color(t.fg_dim)
                .child(kv_label_value("prop.current", &current))
                .child(kv_label_value("prop.peak", &peak))
                .child(kv_label_value("prop.unit", unit)),
        )
        .child(elements::sparkline(
            std::rc::Rc::clone(samples),
            color,
            396.0,
            36.0,
        ))
}

/// The unit preferences the Properties dialog folds rows under. The dialog's
/// callers (`DetailsPanelProps` consumers in `render.rs`) do not thread the
/// window's live preferences into this pure module, and the dialog previously
/// hardcoded decimal-MB strings ignoring the preferences entirely; folding
/// under the Mission-Center-parity default (bytes, base-2 — the same
/// ladder the TUI/Iced properties surfaces render) is the neutral-VM
/// convergence. Threading live preferences here needs a props-field change
/// outside this file and stays a follow-up.
fn properties_unit_preferences() -> UnitPreferences {
    UnitPreferences::default()
}

/// One VM row as the dialog's display string: the folded text, or the shared
/// dash for [`DetailValue::Missing`] — never a fabricated value.
fn vm_display(rows: &[ProcessDetailsRowVm], field: ProcessDetailsField) -> String {
    match detail_value(rows, field) {
        DetailValue::Text(text) => text.clone(),
        DetailValue::Missing => missing_value(),
    }
}

/// The Overview section's field order (label keys + VM fields) — the single
/// list `details_overview` renders.
const OVERVIEW_FIELDS: [(ProcessDetailsField, &str); 7] = [
    (ProcessDetailsField::Name, "common.name"),
    (ProcessDetailsField::Pid, "proc.pid"),
    (ProcessDetailsField::ParentPid, "prop.parent_pid"),
    (ProcessDetailsField::User, "common.user"),
    (ProcessDetailsField::Status, "common.status"),
    (ProcessDetailsField::Threads, "common.threads"),
    (ProcessDetailsField::StartTime, "prop.start_time"),
];

/// The Command section's field order (label keys + VM fields) — the single
/// list `details_command` renders.
const COMMAND_FIELDS: [(ProcessDetailsField, &str); 3] = [
    (ProcessDetailsField::Name, "common.name"),
    (ProcessDetailsField::Exe, "prop.location"),
    (ProcessDetailsField::Cmdline, "prop.command_line"),
];

/// Rows every `(field, label)` pair folds to, in order — shared by the
/// section renderers and the parity tests.
fn vm_rows(
    item: &ProcessItem,
    fields: &[(ProcessDetailsField, &'static str)],
    local_time_rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
) -> Vec<(&'static str, String)> {
    let vm = taskmanager_application::process_details_vm::process_details_rows_with_local_time(
        item,
        &properties_unit_preferences(),
        local_time_rules,
    );
    fields
        .iter()
        .map(|&(field, key)| (i18n::t(key), vm_display(&vm, field)))
        .collect()
}

fn details_overview(
    t: &Theme,
    item: &ProcessItem,
    local_time_rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
) -> Div {
    let mut section = div().flex().flex_col().gap(tokens::SPACE_6);
    for (label, value) in vm_rows(item, &OVERVIEW_FIELDS, local_time_rules) {
        section = section.child(prop_row(t, label, value));
    }
    section
}

fn details_performance(
    t: &Theme,
    item: &ProcessItem,
    histories: &super::ProcessHistories,
    local_time_rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
) -> Div {
    let prefs = properties_unit_preferences();
    let vm = taskmanager_application::process_details_vm::process_details_rows_with_local_time(
        item,
        &prefs,
        local_time_rules,
    );
    let peaks = super::process_details_stats::performance_peaks(item, histories, &prefs);

    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_6)
        .child(prop_history_graph(
            t,
            i18n::t("common.cpu"),
            vm_display(&vm, ProcessDetailsField::Cpu),
            peaks.cpu,
            "%",
            &histories.cpu,
            t.cpu.into(),
        ))
        .child(prop_history_graph(
            t,
            i18n::t("common.memory"),
            vm_display(&vm, ProcessDetailsField::Memory),
            peaks.memory,
            // The tiered neutral ladder carries its own magnitude unit
            // (KiB/MiB/GiB), so the legend shows the family's base unit the
            // way the CPU graph shows "%".
            "B",
            &histories.memory,
            t.memory.into(),
        ))
        .child(prop_history_graph(
            t,
            i18n::t("proc.disk_read"),
            vm_display(&vm, ProcessDetailsField::DiskReadRate),
            peaks.disk_read,
            i18n::t("prop.bytes_per_second"),
            &histories.disk_read,
            t.disk.into(),
        ))
        .child(prop_history_graph(
            t,
            i18n::t("proc.disk_write"),
            vm_display(&vm, ProcessDetailsField::DiskWriteRate),
            peaks.disk_write,
            i18n::t("prop.bytes_per_second"),
            &histories.disk_write,
            t.disk.into(),
        ))
}

fn details_command(
    t: &Theme,
    item: &ProcessItem,
    local_time_rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
) -> Div {
    let mut section = div().flex().flex_col().gap(tokens::SPACE_6);
    for (label, value) in vm_rows(item, &COMMAND_FIELDS, local_time_rules) {
        section = section.child(prop_row(t, label, value));
    }
    section
}

/// Straight-through inputs for the Properties dialog body (design-debt #1
/// props consolidation): the memoized target item plus its shared-series
/// pack, the active section, and the dialog chrome dependencies.
pub(crate) struct DetailsPanelProps<'a> {
    pub(crate) t: &'a Theme,
    pub(crate) item: &'a ProcessItem,
    pub(crate) histories: &'a super::ProcessHistories,
    pub(crate) active: ProcessDetailsSection,
    pub(crate) insights: ProcessInsightsRenderState<'a>,
    pub(crate) available_width: f32,
    pub(crate) net_escalation: taskmanager_application::NetworkEscalationState,
    pub(crate) entity: Entity<RootView>,
    pub(crate) local_time_rules: &'a taskmanager_core::core::time::LocalTimeRulesObservation,
}

/// Properties content split into explicit Overview / Performance / Command
/// sections. RootView owns the active section and the view only consumes it.
pub(crate) fn details_panel_content(props: DetailsPanelProps<'_>) -> Div {
    let DetailsPanelProps {
        t,
        item,
        histories,
        active,
        insights,
        available_width,
        net_escalation,
        entity,
        local_time_rules,
    } = props;
    let section = match active {
        ProcessDetailsSection::Overview => details_overview(t, item, local_time_rules),
        ProcessDetailsSection::Performance => {
            details_performance(t, item, histories, local_time_rules)
        }
        ProcessDetailsSection::Command => details_command(t, item, local_time_rules),
        ProcessDetailsSection::Insights => render_process_insights(
            t,
            insights,
            &process_insights_labels(),
            available_width,
            net_escalation,
            entity.clone(),
        ),
    };
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_6)
        .w_full()
        .min_w(px(0.0))
        .child(details_section_tabs(t, active, &entity))
        .child(section)
}

fn process_insights_labels() -> ProcessInsightsLabels {
    ProcessInsightsLabels {
        loading: i18n::t("proc_insights.loading"),
        connections: i18n::t("proc_insights.connections"),
        no_connections: i18n::t("proc_insights.no_connections"),
        network_throughput: i18n::t("proc_insights.network_throughput"),
        received: i18n::t("proc_insights.received"),
        sent: i18n::t("proc_insights.sent"),
        gpu: i18n::t("common.gpu"),
        no_gpu: i18n::t("proc_insights.no_gpu"),
        gpu_usage: i18n::t("proc_insights.gpu_usage"),
        vram: i18n::t("proc_insights.vram"),
        resource_limits: i18n::t("proc_insights.resource_limits"),
        memory: i18n::t("common.memory"),
        cpu: i18n::t("proc_insights.cpu_quota"),
        pids: i18n::t("proc_insights.pids"),
        resource_group: i18n::t("proc_insights.resource_group"),
        isolation: i18n::t("proc_insights.isolation"),
        container_id: i18n::t("proc_insights.container_id"),
        sandboxed: i18n::t("proc_insights.sandboxed"),
        host_process: i18n::t("proc_insights.host_process"),
        open_files: i18n::t("proc_insights.open_files"),
        no_open_files: i18n::t("proc_insights.no_open_files"),
        unreadable: i18n::t("proc_insights.unreadable"),
        threads: i18n::t("proc_insights.threads"),
        no_threads: i18n::t("proc_insights.no_threads"),
        thread_id: i18n::t("proc.pid"),
        thread_name: i18n::t("common.name"),
        thread_state: i18n::t("common.status"),
        thread_cpu_time: i18n::t("proc_insights.thread_cpu_time"),
        thread_cpu_percent: i18n::t("proc_insights.thread_cpu_percent"),
        yes: i18n::t("common.yes"),
        no: i18n::t("common.no"),
        unknown: i18n::t("proc_insights.unknown"),
        unlimited: i18n::t("proc_insights.unlimited"),
        healthy: i18n::t("proc_insights.available"),
        stale: i18n::t("proc_insights.process_unavailable"),
        permission_denied: i18n::t("device.permission_denied"),
        provider_unavailable: i18n::t("proc_insights.provider_unavailable"),
        unsupported: i18n::t("proc_insights.unsupported_provider"),
        worker_disconnected: i18n::t("proc_insights.worker_disconnected"),
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_chrome_tests.rs"]
mod tests;
