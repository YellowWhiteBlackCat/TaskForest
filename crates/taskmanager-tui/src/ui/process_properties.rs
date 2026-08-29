//! Process Properties modal (Applications page): a 4-tab overlay mirroring the
//! GPUI reference dialog (`crates/taskmanager-gpui/src/gpui_app/root/chrome.rs::ProcessDetailsSection`
//! + `details_panel_content`).
//!
//! Tabs — Overview / Performance / Command / Insights — consume the SAME shared
//! projections the inline `process_details` panel already reads, with domain
//! facts imported directly from `taskmanager-core`. The modal
//! freezes the selected [`ProcessItem`] at open time so a list refresh cannot
//! redirect the view; the Insights tab additionally reads the live
//! `process_insights` projection (last-wins for the frozen target pid),
//! rendering honest typed states (Pending / Unavailable / Current) — never
//! fabricated values.
//!
//! Tab confirmation against the GPUI source: the four sections defined by
//! `ProcessDetailsSection` in `chrome.rs` are `Overview`, `Performance`,
//! `Command`, `Insights` (NOT "Threads / Open Files" — those are cards inside
//! the Insights tab; `threads::threads_card` / `open_files::open_files_card`
//! in `process_insights/view.rs`). This modal mirrors that exact structure.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use taskmanager_application::i18n::t;
use taskmanager_application::process_details_vm::ProcessDetailsField;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_shell::presentation::value_with_peak;

use super::containers::Modal;
use super::kv;
use super::process_details::vm_text;
use crate::TuiApp;
use crate::TuiTheme;

/// The active Properties section. Mirrors GPUI's `ProcessDetailsSection`
/// (`crates/taskmanager-gpui/src/gpui_app/root/chrome.rs`): Overview / Performance / Command / Insights.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProcessDetailsSection {
    #[default]
    Overview,
    Performance,
    Command,
    Insights,
}

impl ProcessDetailsSection {
    /// Every section in the order the Tab / Left-Right cycle walks them.
    /// Enumerated here (not re-listed at the call site) so the quality gate's
    /// "no duplicated variant list" rule holds.
    pub const ALL: [Self; 4] = [
        Self::Overview,
        Self::Performance,
        Self::Command,
        Self::Insights,
    ];

    /// The next section in the Overview → Performance → Command → Insights cycle.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Overview => Self::Performance,
            Self::Performance => Self::Command,
            Self::Command => Self::Insights,
            Self::Insights => Self::Overview,
        }
    }

    /// The previous section (BackTab / Left cycle).
    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::Overview => Self::Insights,
            Self::Performance => Self::Overview,
            Self::Command => Self::Performance,
            Self::Insights => Self::Command,
        }
    }

    /// The existing i18n catalog key for the tab label (reused as-is so the
    /// modal adds no locales edits).
    #[must_use]
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Overview => "prop.overview",
            Self::Performance => "prop.performance",
            Self::Command => "prop.command",
            Self::Insights => "prop.insights",
        }
    }
}

/// The Properties modal's frozen target: the process row plus the active tab,
/// plus the tab body's vertical-scroll intent. The process is cloned at open
/// time so a list refresh cannot redirect the view while the modal is open
/// (mirrors the process-action menu's freeze). `scroll` is the user's scroll
/// intent for the current tab body; the renderer clamps it to
/// `[0, max(0, content_lines - visible_height)]` so a short terminal can still
/// reach every row. It is reset to 0 on open and on tab switch (each tab is
/// independent content).
#[derive(Clone, Debug)]
pub struct ProcessPropertiesTarget {
    pub item: ProcessItem,
    pub section: ProcessDetailsSection,
    pub scroll: usize,
}

/// Render the Process Properties modal centred over `area`.  Test-only entry:
/// the caller supplies the committed focus plan so the highlighted tab stays
/// the plan's decision, not the frozen target state's.
#[cfg(test)]
pub fn render_process_properties(
    frame: &mut Frame<'_>,
    target: &ProcessPropertiesTarget,
    app: &TuiApp,
    theme: TuiTheme,
    focus: super::TuiFocusPlan,
    area: Rect,
) {
    render_process_properties_at(
        frame,
        target,
        app,
        theme,
        focus,
        super::planned_popup(
            area,
            crate::TuiInputScope::SharedSurface(
                taskmanager_application::SurfaceKind::ProcessProperties,
            ),
        ),
    );
}

/// Render the Process Properties modal from the committed focus plan. The
/// highlighted tab is the plan's `PropertiesTab` control; any other control
/// paints every tab dim (fail-closed).
pub(super) fn render_process_properties_at(
    frame: &mut Frame<'_>,
    target: &ProcessPropertiesTarget,
    app: &TuiApp,
    theme: TuiTheme,
    focus: super::TuiFocusPlan,
    popup: Rect,
) {
    // Modal geometry: wide enough for the kv rows + command line, tall enough
    // for the tab row + a bounded body. Clamped by the frame plan to the frame so
    // a small terminal never overflows. The plain-titled Modal host paints the
    // identity in the title text itself (no icon): "Process details <name> · <pid>".
    let inner = Modal::plain(
        theme,
        theme.accent,
        &format!(
            "{} {} · {}",
            t("prop.process_details"),
            target.item.name,
            target.item.pid
        ),
    )
    .render(frame, popup);

    let [tab_row, body] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(inner);

    frame.render_widget(tab_row_line(focus.properties_tab(), theme), tab_row);

    let lines = match target.section {
        ProcessDetailsSection::Overview => {
            overview_lines(&target.item, &app.local_time_rules, theme)
        }
        ProcessDetailsSection::Performance => {
            performance_lines(&target.item, &app.local_time_rules, theme)
        }
        ProcessDetailsSection::Command => command_lines(&target.item, &app.local_time_rules, theme),
        ProcessDetailsSection::Insights => {
            super::process_details::insights_lines(app, theme, target.item.pid)
        }
    };
    // Short-terminal scroll: a tab body can exceed the modal's bounded body
    // area, so the paragraph scrolls by the clamped user intent (Up / Down or
    // Ctrl+Up / Ctrl+Down — the modal traps those keys). The wrap-aware height
    // comes from ratatui's own line_count against the body width, so the clamp
    // matches what the renderer actually draws.
    let content_lines = super::process_details::wrapped_content_height(&lines, body.width);
    let (effective, _max) =
        super::process_details::clamped_scroll(content_lines, body.height, target.scroll);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((effective as u16, 0)),
        body,
    );
}

/// The tab selector row. The active tab wears the highlight background + bold;
/// inactive tabs are dim. A trailing hint documents the switch chords so the
/// modal is discoverable without the help overlay.
fn tab_row_line(active: Option<ProcessDetailsSection>, theme: TuiTheme) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for section in ProcessDetailsSection::ALL {
        let is_active = active == Some(section);
        spans.push(Span::styled(
            format!(" {} ", t(section.label_key())),
            if is_active {
                Style::new()
                    .fg(theme.color(Color::White))
                    .bg(theme.highlight_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme.dim)
            },
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        t("tui.properties_footer"),
        Style::new().fg(theme.dim),
    ));
    Line::from(spans)
}

/// Overview tab: frozen identity facts (mirrors GPUI `details_overview`),
/// folded once by the neutral process-details VM; this builder only assigns
/// labels. Every unavailable scalar renders an explicit dash, never a
/// fabricated value.
fn overview_pairs(
    item: &ProcessItem,
    local_time_rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
) -> Vec<(&'static str, String)> {
    let rows = taskmanager_application::process_details_vm::process_details_rows_with_local_time(
        item,
        &UnitPreferences::default(),
        local_time_rules,
    );
    let text = |field| vm_text(&rows, field);
    vec![
        (t("common.name"), text(ProcessDetailsField::Name)),
        (t("proc.pid"), text(ProcessDetailsField::Pid)),
        (t("prop.parent_pid"), text(ProcessDetailsField::ParentPid)),
        (t("common.user"), text(ProcessDetailsField::User)),
        (t("common.status"), text(ProcessDetailsField::Status)),
        (t("common.threads"), text(ProcessDetailsField::Threads)),
        (t("prop.start_time"), text(ProcessDetailsField::StartTime)),
    ]
}

fn overview_lines(
    item: &ProcessItem,
    local_time_rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
    theme: TuiTheme,
) -> Vec<Line<'static>> {
    overview_pairs(item, local_time_rules)
        .into_iter()
        .map(|(label, value)| kv(label, value, theme))
        .collect()
}

/// Performance tab: the VM's current value plus the 60-sample peak for each
/// resource series (mirrors GPUI `details_performance`'s current/peak
/// headers). The GPUI dialog draws one sparkline graph per series; a
/// bounded terminal cannot fit four graphs comfortably, so this tab renders
/// the same current/peak numbers as honest typed text rows instead. Peaks
/// reuse the shared shell fold (`presentation::peak_of`, max finite sample
/// floored by the live reading) — a series with NO current reading and NO
/// history renders the shared dash, never a fabricated `0.0`.
fn performance_pairs(
    item: &ProcessItem,
    local_time_rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
) -> Vec<(&'static str, String)> {
    let rows = taskmanager_application::process_details_vm::process_details_rows_with_local_time(
        item,
        &UnitPreferences::default(),
        local_time_rules,
    );
    let text = |field| vm_text(&rows, field);
    let peaks = super::process_data::process_performance_peaks(item);
    vec![
        (
            t("common.cpu"),
            value_with_peak(Some(text(ProcessDetailsField::Cpu)), peaks.cpu),
        ),
        (
            t("common.memory"),
            value_with_peak(Some(text(ProcessDetailsField::Memory)), peaks.memory),
        ),
        (
            t("proc.disk_read"),
            value_with_peak(
                Some(text(ProcessDetailsField::DiskReadRate)),
                peaks.disk_read,
            ),
        ),
        (
            t("proc.disk_write"),
            value_with_peak(
                Some(text(ProcessDetailsField::DiskWriteRate)),
                peaks.disk_write,
            ),
        ),
    ]
}

fn performance_lines(
    item: &ProcessItem,
    local_time_rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
    theme: TuiTheme,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = performance_pairs(item, local_time_rules)
        .into_iter()
        .map(|(label, value)| kv(label, value, theme))
        .collect();
    lines.push(Line::from(Span::styled(
        t("prop.last_60_seconds"),
        Style::new().fg(theme.dim),
    )));
    lines
}

/// Command tab: executable location + full command line (mirrors GPUI
/// `details_command`), folded by the neutral VM; this builder only assigns
/// labels. An absent exe path / empty cmdline renders an explicit dash,
/// never a fabricated path.
fn command_pairs(
    item: &ProcessItem,
    local_time_rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
) -> Vec<(&'static str, String)> {
    let rows = taskmanager_application::process_details_vm::process_details_rows_with_local_time(
        item,
        &UnitPreferences::default(),
        local_time_rules,
    );
    let text = |field| vm_text(&rows, field);
    vec![
        (t("common.name"), text(ProcessDetailsField::Name)),
        (t("prop.location"), text(ProcessDetailsField::Exe)),
        (t("prop.command_line"), text(ProcessDetailsField::Cmdline)),
    ]
}

fn command_lines(
    item: &ProcessItem,
    local_time_rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
    theme: TuiTheme,
) -> Vec<Line<'static>> {
    command_pairs(item, local_time_rules)
        .into_iter()
        .map(|(label, value)| kv(label, value, theme))
        .collect()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/process_properties_tests.rs"]
mod vm_parity_tests;
