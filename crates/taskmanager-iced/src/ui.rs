//! The iced view layer: pages, gauges and the process table, all driven by
//! the shared [`ShellApp`] state (ADR-027) and styled from the neutral theme
//! snapshot via [`crate::theme`].
//!
//! The heavy render groups live in child modules and are glob-reexported here
//! so the root [`view`] stays a thin dispatch and the headless tests reach the
//! pure seams by name:
//! - `perf_overview`: the CPU/memory gauges + chart + composition panel.
//! - `perf_devices`: the GPU / Disk / Network / Battery detail panels.
//! - `applications`: the Applications page + table rows + view modes.
//! - `app_history_view`: the App-history page.

use iced::Element;
use iced::widget::{column, container, row, scrollable, text};
// Shared locale catalog (the process-wide resolver). The renderer-local
// `crate::i18n::t(language, key)` still backs the iced-owned modal chrome
// (settings / about / health / containers); this `t` carries the shared-page
// body strings (Performance / Applications) that were previously hard-coded.
use taskmanager_application::AppPage;
use taskmanager_application::i18n::{alert_severity_label, t};
use taskmanager_shell::{PageHelp, ShellApp};
use taskmanager_theme::tokens;
use taskmanager_ui_contract::IconId;

use crate::app::{FocusTarget, Message, PerfDevice};
use crate::focus;
use crate::i18n::{self, Key};
use crate::theme;
use responsive::ChromePresentation;

mod about;
mod affinity;
mod alerts;
pub(crate) mod app_history_view;
pub(crate) mod applications;
mod column_menu;
pub(crate) mod components;
mod containers;
// The first-run dialog is wired end-to-end (boot observation, correlated
// platform answers and the `LocalSurface::FirstRun` slot live in
// `app::update::first_run`). The System dashboard segment's page mount still
// lands with its owner (`ui::system_table`, a parallel workflow); until then
// that module is exercised by its headless behavior tests, which is why it
// carries a scoped dead-code allowance.
pub(crate) mod core_grid;
mod device_chart;
pub(crate) mod directory_usage;
mod fan;
pub(crate) mod first_run;
pub(crate) mod format;
pub(crate) mod health;
pub(crate) mod history_replay;
mod insights;
pub(crate) mod overlays;
pub(crate) mod perf_devices;
mod perf_layout;
pub(crate) mod perf_overview;
pub(crate) mod perf_rail;
mod performance;
pub(crate) mod process_projection;
pub(crate) mod process_sparkline;
pub mod responsive;
pub(crate) mod service_details;
mod service_menu;
mod settings;
pub(crate) mod spinner;
mod startup_menu;
pub(crate) mod startup_table;
pub(crate) mod system_dashboard;
mod system_dashboard_model;
pub(crate) mod system_table;
pub(crate) mod tables;
pub(crate) mod users;
mod virtual_list;

pub(crate) use virtual_list::{
    VIRTUAL_TABLE_HEADER_HEIGHT, VirtualWindow, virtual_horizontal_body, virtual_table,
    virtual_table_body, virtual_table_key, virtual_table_row,
};

// Re-export the extracted render groups so the root view and the headless
// tests reach every seam by its original (unqualified) name. Each glob only
// re-imports items the child defined as `pub(super)`/`pub(crate)`; private
// `use` imports inside the children are not re-exported.
use app_history_view::*;
use applications::page::applications_page;
pub(crate) use device_chart::GraphPrefs;
use perf_devices::*;
use perf_overview::*;
pub(crate) use performance::{
    UnitPrefs, chunked_rows, compact_toolbar_columns, perf_device_label, performance_page,
};

// The About modal's clipboard payload seam (G-16) — named re-export so the
// update path builds the copy text through the same rows the modal renders.
pub(crate) use about::about_copy_payload;

/// The current-window capture trigger button in the top navigation strip.
pub(crate) fn current_window_capture_btn<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    language: crate::i18n::Language,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    focus::ghost_button_with_icon(
        theme_snapshot,
        FocusTarget::WindowCapture,
        IconId::Export,
        i18n::t(language, Key::WindowCapture),
        Message::RequestCurrentWindowCapture,
    )
}

/// Build the root element for one render. The root input observer watches
/// pointer presses for the focus-visible tracker
/// (`crate::input_modality`) before the tree below handles the same event —
/// the iced counterpart of the GPUI root's capture-phase listeners.
pub fn view(app: &crate::IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    crate::input_modality::Observer::new(view_root(app)).into()
}

/// The observed root: page scaffold, overlays, and warm-up body for one render.
fn view_root(app: &crate::IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let shell = &app.shell;
    let theme_snapshot = app.theme();
    let language = app.language();

    // One GPUI-shaped nav strip: the page tabs (accent-filled when active — the
    // same `choice_pill` the Performance device rail uses) on the left, a flex
    // space, then the toolbar triggers pinned right. Collapses the old three
    // plain rows (static title / text tabs / toolbar) into the single chrome bar
    // GPUI renders, so the active page reads at a glance. Page and primary
    // toolbar icons use the shared semantic SVG registry through Iced's own
    // `svg` widget.
    let current_page = shell.page();
    // The frontend-local alerts route suppresses the shared-tab highlight so
    // only one route reads as active at a time.
    let alerts_open = app.alerts_page_open();
    let mut page_tabs: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> =
        taskmanager_shell::page_help()
            .into_iter()
            .map(|PageHelp { page, label, .. }| {
                focus::choice_pill_with_icon(
                    theme_snapshot,
                    FocusTarget::PageTab(page),
                    page_icon(page),
                    label.to_string(),
                    page == current_page && !alerts_open,
                    Message::SelectPage(page),
                )
            })
            .collect();
    // The alerts page rides the same tab strip as the shared pages (an
    // Iced-local route outside the `AppPage` set).
    page_tabs.push(alerts::page_tab_pill(app));
    let toolbar_items: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> = vec![
        focus::ghost_button_with_icon(
            theme_snapshot,
            FocusTarget::SettingsTrigger,
            IconId::Settings,
            i18n::t(language, Key::Settings),
            Message::OpenSettings,
        ),
        focus::ghost_button(
            theme_snapshot,
            FocusTarget::ContainersTrigger,
            i18n::t(language, Key::Containers),
            Message::OpenContainers,
        ),
        focus::ghost_button_with_icon(
            theme_snapshot,
            FocusTarget::HealthTrigger,
            IconId::Health,
            i18n::t(language, Key::Health),
            Message::OpenHealth,
        ),
        current_window_capture_btn(theme_snapshot, language),
        focus::ghost_button_with_icon(
            theme_snapshot,
            FocusTarget::Export,
            IconId::Export,
            i18n::t(language, Key::Export),
            Message::ExportSnapshot,
        ),
        focus::ghost_button_with_icon(
            theme_snapshot,
            FocusTarget::AboutTrigger,
            IconId::System,
            i18n::t(language, Key::About),
            Message::OpenAbout,
        ),
    ];
    // The full page vocabulary plus the five toolbar actions is wider than a
    // normal 1180px desktop viewport. Give the route strip its own horizontal
    // viewport and put actions on a second bounded row before they can paint
    // past the right edge. The wide 1440px+ layout keeps the original one-row
    // desktop composition. The 1320px single-row seam is the frame budget's
    // chrome presentation (responsive.rs), not a local literal.
    let chrome = ChromePresentation::for_width(app.viewport_width());
    let wrapped_chrome = app.compact_layout() || chrome.is_wrapped();
    let toolbar: Element<'_, Message, iced::Theme, iced::Renderer> = if wrapped_chrome {
        chunked_rows(toolbar_items, compact_toolbar_columns(app.viewport_width()))
    } else {
        row(toolbar_items).spacing(4).into()
    };
    // The wide layout keeps the action toolbar pinned to the trailing edge.
    // Compact windows get intentional rows: routes remain in one bounded
    // strip and actions get their own wrapped full-width rows. Mixing both in
    // one horizontal scroller made the first screenshot look like controls
    // had disappeared behind the right edge even though they were reachable.
    let nav: Element<'_, Message, iced::Theme, iced::Renderer> = if wrapped_chrome {
        let page_nav = scrollable(row(page_tabs).spacing(4).padding([
            f32::from(taskmanager_theme::tokens::SPACE_2),
            f32::from(taskmanager_theme::tokens::SPACE_4),
        ]))
        .direction(iced::widget::scrollable::Direction::Horizontal(
            iced::widget::scrollable::Scrollbar::default(),
        ))
        .height(iced::Length::Fixed(44.0))
        .width(iced::Length::Fill);
        column![page_nav, toolbar]
            .spacing(4)
            .width(iced::Length::Fill)
            .into()
    } else {
        row(page_tabs)
            .spacing(4)
            .push(iced::widget::Space::new().width(iced::Length::Fill))
            .push(toolbar)
            .padding([
                f32::from(taskmanager_theme::tokens::SPACE_2),
                f32::from(taskmanager_theme::tokens::SPACE_4),
            ])
            .into()
    };

    let collecting = shell.telemetry_frame_state().is_collecting();
    let body: Element<'_, Message, iced::Theme, iced::Renderer> = if collecting {
        telemetry_warmup_body(theme_snapshot, app.warmup_spin_phase())
    } else if app.alerts_page_open() {
        alerts::render(app)
    } else {
        match shell.page() {
            AppPage::Performance => performance_page(app),
            AppPage::Applications => applications_page(app),
            AppPage::Services => tables::services_page(app),
            AppPage::System => system_table::system_page(app),
            AppPage::Startup => startup_table::startup_page(app),
            AppPage::Users => users::render(app),
            AppPage::AppHistory => app_history_page(app),
        }
    };

    let status = footer_status(shell);
    let status_color = if shell.should_quit() {
        crate::theme_binding::color(theme_snapshot.palette().warning)
    } else if let Some(notice) = shell.feedback_notice() {
        match notice.severity() {
            taskmanager_shell::FeedbackSeverity::Info => {
                crate::theme_binding::color(theme_snapshot.palette().fg)
            }
            taskmanager_shell::FeedbackSeverity::Success => {
                crate::theme_binding::color(theme_snapshot.palette().success)
            }
            taskmanager_shell::FeedbackSeverity::Warning => {
                crate::theme_binding::color(theme_snapshot.palette().warning)
            }
            taskmanager_shell::FeedbackSeverity::Error => {
                crate::theme_binding::color(theme_snapshot.palette().danger)
            }
        }
    } else {
        crate::theme_binding::color(theme_snapshot.palette().fg)
    };
    // Active-alert indicator (BN-07): count + worst severity tone from the
    // shared alert center's last evaluation, rendered through the shared
    // badge grammar (see [`alert_badge_tone`] — the same palette semantics
    // the old inline colored-text pill used). Rendered before the shortcut
    // hint; absent when no alert is active. The hover tooltip surfaces the
    // worst severity's label so the tone is never color-only information.
    let alert_text: Option<String> = (!shell.projection().alert_active.is_empty()).then(|| {
        taskmanager_application::i18n::t("alerts.active").replacen(
            "{}",
            &shell.projection().alert_active.len().to_string(),
            1,
        )
    });
    let worst_severity = shell
        .projection()
        .alert_active
        .iter()
        .map(|a| a.severity)
        .max();
    let alert_pill = match (&alert_text, worst_severity) {
        (Some(label), Some(worst)) => Some(components::tooltip(
            theme_snapshot,
            components::badge(theme_snapshot, label, alert_badge_tone(worst)),
            alert_severity_label(worst),
        )),
        _ => None,
    };
    // Persistent paused indicator (GPUI parity): hold-Ctrl freeze and
    // Ctrl+Space pause both land in the shared refresh policy, so one accent
    // ⏸ badge keeps the state visible in the footer instead of the graphs
    // silently going still — the same BadgeTone::Accent pill the GPUI
    // titlebar renders. Hovering recalls the shortcut vocabulary that
    // toggles the state.
    let paused_label = shell
        .paused()
        .then(|| format!("\u{23f8} {}", t("common.paused")));
    let paused_pill = paused_label.as_deref().map(|label| {
        components::tooltip(
            theme_snapshot,
            components::badge(theme_snapshot, label, components::BadgeTone::Accent),
            t("hint.footer_shortcuts"),
        )
    });
    let footer = row![
        text(status)
            .size(f32::from(tokens::FONT_12))
            .color(status_color),
        paused_pill,
        alert_pill,
        text(t("hint.footer_shortcuts"))
            .size(f32::from(tokens::FONT_12))
            // Let the shortcut hint consume the remaining row width and
            // wrap at word boundaries in compact windows. Without an
            // explicit Fill width Iced measures the full string
            // intrinsically and the right half disappears beyond the
            // 720px viewport.
            .width(iced::Length::Fill),
    ]
    .spacing(8)
    .padding(6);

    let base: Element<'_, Message, iced::Theme, iced::Renderer> =
        components::page_scaffold(nav, body, footer.into());

    if !collecting {
        if let Some(overlay) = overlays::render(app) {
            // Do not keep the application tree underneath a modal. Besides
            // blocking pointer events, removing it from the returned tree makes
            // Iced's focus operation see only the modal scope; this is the
            // renderer-side counterpart of the shell's modal precedence.
            return overlay;
        }
        if let Some(overlay) = local_modal(app) {
            return overlay;
        }
    }

    base
}

fn telemetry_warmup_body<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    spin: Option<f32>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    container(
        column![
            // The revolving accent arc (GPUI-spinner counterpart): the
            // per-frame pump advances the phase while the shell collects its
            // first frame; a static arc renders when motion is disabled.
            spinner::canvas_view(theme_snapshot, spin),
            text(t("common.loading")).size(f32::from(tokens::FONT_16)),
            text(t("common.telemetry_warming_up"))
                .size(f32::from(tokens::FONT_12))
                .color(muted),
        ]
        .spacing(f32::from(tokens::SPACE_8))
        .align_x(iced::Alignment::Center),
    )
    .style(move |_| theme::card_style(theme_snapshot))
    .padding(f32::from(tokens::SPACE_16))
    .width(iced::Length::Fill)
    .height(iced::Length::Fill)
    .center_x(iced::Length::Fill)
    .center_y(iced::Length::Fill)
    .into()
}

fn page_icon(page: AppPage) -> IconId {
    match page {
        AppPage::Performance => IconId::Performance,
        AppPage::Applications => IconId::Applications,
        AppPage::Services => IconId::Services,
        AppPage::System => IconId::System,
        AppPage::Startup => IconId::Startup,
        AppPage::Users => IconId::Users,
        AppPage::AppHistory => IconId::History,
    }
}

/// Exhaustive projection of the one Iced-owned primary surface.
fn local_modal(app: &crate::IcedApp) -> Option<Element<'_, Message, iced::Theme, iced::Renderer>> {
    use crate::app::LocalSurface;
    Some(match app.local_surface()? {
        LocalSurface::Settings => settings::render(app),
        LocalSurface::About => about::render(app),
        LocalSurface::Health => health::render(app),
        LocalSurface::Containers => containers::render(app),
        LocalSurface::DiskSmart { index } => overlays::smart_overlay(app, *index),
        LocalSurface::ProcessAffinity { .. } => affinity::render(app),
        LocalSurface::ServiceDetails { .. } => service_details::render(app),
        LocalSurface::RunTask => {
            overlays::run_task_overlay(app.theme(), &app.run_task, app.modal_appear_progress())
        }
        LocalSurface::AlertCenter => overlays::alert_center_overlay(
            app.theme(),
            app.shell.projection().alert_center.event_history(),
            app.shell.projection().alert_center.policy(),
            app.modal_appear_progress(),
        ),
        LocalSurface::FirstRun => {
            first_run::render_first_run(app.theme(), &app.first_run, app.modal_appear_progress())
        }
    })
}

/// The Performance page: MC's select-a-device detail model. A row of resource
/// tabs selects which device's detail panel renders below; the detail shows
/// ONLY the selected resource, reusing the existing per-resource section fns
/// (CPU → fixed available history stack + per-core grid; Memory →
/// composition + utilization history; Disk → [`perf_devices::disk_section`];
/// Network → [`perf_devices::network_section`]; Gpu →
/// [`perf_devices::gpu_section`]). The selector state is frontend-local
/// ([`IcedApp::perf_device`]); the shell never sees it.
/// The footer status line: the localized quitting notice once the shell has
/// consumed a quit request, otherwise the live shell status verbatim. Pure seam
/// so the quitting branch is table-testable headless.
#[must_use]
pub(crate) fn footer_status(shell: &ShellApp) -> String {
    if shell.should_quit() {
        t("hint.quitting").to_string()
    } else {
        shell.feedback_text().to_owned()
    }
}

/// The footer alert pill's badge tone: the worst active severity mapped onto
/// the shared badge grammar — Critical → danger, Warning → caution, Info →
/// accent. The same palette semantic colors the old inline severity-colored
/// text pill used; only the presentation moved to the tone-filled capsule.
fn alert_badge_tone(
    severity: taskmanager_core::core::alerts::AlertSeverity,
) -> components::BadgeTone {
    use taskmanager_core::core::alerts::AlertSeverity;
    match severity {
        AlertSeverity::Critical => components::BadgeTone::Danger,
        AlertSeverity::Warning => components::BadgeTone::Warning,
        AlertSeverity::Info => components::BadgeTone::Accent,
    }
}

pub(crate) use format::*;
pub(crate) mod lazy_key;

#[cfg(test)]
#[path = "../tests/gui/ui/tests.rs"]
mod tests;

/// Chrome-seam tests for the strings this module owns.
#[cfg(test)]
#[path = "../tests/gui/ui/chrome_tests.rs"]
mod chrome_tests;
