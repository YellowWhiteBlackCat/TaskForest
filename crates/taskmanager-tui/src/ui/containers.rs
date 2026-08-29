//! Containers overlay: the aggregated cgroup-v2 rollup.
//!
//! The rollup is consumed from the renderer-independent `SystemProjectionStore::containers`
//! projection. The TUI does not mirror or mutate the platform event; the shell
//! applies the correlated event once and owns the refresh truth.
//!
//! Honesty contract (mirrors `ContainerRollup`): an `Unsupported` /
//! `PermissionDenied` state renders its typed marker, a healthy host with no
//! containers renders "no containers running", and per-container readings
//! that are unavailable render as `—`, never a fabricated zero.
//!
//! This module also hosts two shared presentation contracts. The modal
//! contract ([`Modal`] — the `Clear` + bordered, titled host block — and
//! [`KeyHint`], the footer chord-hint vocabulary with the typed
//! [`KeyHintTone`] chord palette) serves every overlay surface. The
//! [`render_windowed_table`] contract serves the row-table pages
//! (Applications / Services / Startup / Users): one bordered-table-with-window
//! paint (window clipping, selection highlight, column layout) with the
//! honest empty/failure state panel as the zero-row branch. Both own only
//! terminal presentation — geometry arrives from the caller (frame_plan owns
//! it), the domain facts (rows, chord and state texts) stay with the calling
//! surface.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, HighlightSpacing, Paragraph, Row, Table, TableState,
};
use taskmanager_application::container_row_window;
use taskmanager_application::i18n::t;
use taskmanager_core::core::process_telemetry::{ContainerRollup, IsolationKind};
use taskmanager_shell::presentation::{bytes, missing_value};
use taskmanager_ui_contract::IconId;

use super::frame_plan::TablePanelProjection;
use super::{panel, render_empty_panel};
use crate::TuiApp;
use crate::TuiTheme;
use crate::ui::{DeviceHealth, classify_device_state};

/// The shared modal host: `Clear` plus a bordered, titled overlay block.
/// Presentation only — the popup `Rect` arrives from the caller and the inner
/// body/footer layout stays with each surface; the host owns the backdrop
/// clear, the border tone, and the title row.
pub(super) struct Modal {
    block: Block<'static>,
}

impl Modal {
    /// The standard surface modal: accent border, overlay backdrop, and an
    /// iconified title.
    pub(super) fn new(theme: TuiTheme, icon: IconId, title: &str) -> Self {
        Self::titled(
            theme,
            theme.accent,
            format!(" {} {} ", theme.glyph(icon), title),
        )
    }

    /// The confirmation-family modal: an explicit border tone with a plain
    /// padded title (no icon).
    pub(super) fn alert(theme: TuiTheme, border: Color, title: &str) -> Self {
        Self::plain(theme, border, title)
    }

    /// The plain-titled surface modal: an explicit border tone with a padded
    /// title and no icon — for surfaces whose identity lives in the title text
    /// itself (the process-properties modal). [`Modal::alert`] shares this
    /// shape for the confirmation family.
    pub(super) fn plain(theme: TuiTheme, border: Color, title: &str) -> Self {
        Self::titled(theme, border, format!(" {title} "))
    }

    fn titled(theme: TuiTheme, border: Color, title: String) -> Self {
        Self {
            block: Block::new()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(border))
                .style(Style::new().bg(theme.overlay_bg))
                .title(title),
        }
    }

    /// Paint `Clear` + the host block over `popup` and return the inner area
    /// the surface lays its own body and footer into.
    pub(super) fn render(self, frame: &mut Frame<'_>, popup: Rect) -> Rect {
        frame.render_widget(Clear, popup);
        let inner = self.block.inner(popup);
        frame.render_widget(self.block, popup);
        inner
    }
}

/// The shared modal footer key-hint vocabulary: every chord paints
/// black-on-tone (see [`KeyHintTone`]), every label carries the treatment its
/// tone fixes. The component owns only the terminal presentation of the pairs
/// — the chord/label text and the footer `Rect` stay with the calling surface.
pub(super) struct KeyHint;

impl KeyHint {
    /// The accent chord+label spans, in order (the standard modal footer
    /// vocabulary: black-on-accent chords, dim labels).
    pub(super) fn spans(theme: TuiTheme, hints: Vec<(&'static str, String)>) -> Vec<Span<'static>> {
        Self::spans_toned(
            theme,
            hints
                .into_iter()
                .map(|(chord, label)| (KeyHintTone::Accent, chord, label))
                .collect(),
        )
    }

    /// Chord-toned hint pairs, in order: each pair names its own
    /// [`KeyHintTone`], so a two-chord footer (confirm + dismiss) can carry
    /// both tones on one line.
    pub(super) fn spans_toned<S: Into<String>>(
        theme: TuiTheme,
        hints: Vec<(KeyHintTone, S, String)>,
    ) -> Vec<Span<'static>> {
        hints
            .into_iter()
            .flat_map(|(tone, chord, label)| {
                let (chord_style, label_style) = tone.styles(theme);
                [
                    Span::styled(chord.into(), chord_style),
                    Span::styled(label, label_style),
                ]
            })
            .collect()
    }

    /// The single styled accent hint line (for footers that compose extra
    /// spans or a leading blank row around the hint).
    pub(super) fn line(theme: TuiTheme, hints: Vec<(&'static str, String)>) -> Line<'static> {
        Line::from(Self::spans(theme, hints))
    }

    /// The single styled chord-toned hint line.
    pub(super) fn line_toned<S: Into<String>>(
        theme: TuiTheme,
        hints: Vec<(KeyHintTone, S, String)>,
    ) -> Line<'static> {
        Line::from(Self::spans_toned(theme, hints))
    }

    /// The centered single-line hint paragraph (footers that paint the hint on
    /// their first row).
    pub(super) fn centered(
        theme: TuiTheme,
        hints: Vec<(&'static str, String)>,
    ) -> Paragraph<'static> {
        Paragraph::new(Self::line(theme, hints)).alignment(Alignment::Center)
    }
}

/// The chord tone of one key-hint pair: the painted cell background behind
/// the black chord text. The tone fixes the pair's whole vocabulary —
/// [`KeyHintTone::Accent`] pairs carry the modal footer's dim labels, while
/// the confirmation family's [`KeyHintTone::Danger`] and
/// [`KeyHintTone::Inverse`] pairs keep the default-foreground labels the
/// popups have always painted (preserved presentation, not a restyle).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyHintTone {
    /// Black-on-accent — the standard modal footer chord.
    Accent,
    /// Black-on-danger — the destructive confirm chord.
    Danger,
    /// Black-on-white — the dismissive chord of a confirm/dismiss footer.
    Inverse,
}

impl KeyHintTone {
    /// The (chord, label) style pair this tone paints.
    fn styles(self, theme: TuiTheme) -> (Style, Style) {
        match self {
            Self::Accent => (
                Style::new().fg(theme.color(Color::Black)).bg(theme.accent),
                Style::new().fg(theme.dim),
            ),
            Self::Danger => (
                Style::new().fg(theme.color(Color::Black)).bg(theme.danger),
                Style::new(),
            ),
            Self::Inverse => (
                Style::new()
                    .fg(theme.color(Color::Black))
                    .bg(theme.color(Color::White)),
                Style::new(),
            ),
        }
    }
}

// ── Windowed bordered table ──────────────────────────────────────────────────
//
// The shared bordered-table-with-window paint of the row-table pages
// (Applications / Services / Startup / Users). Like [`Modal`], the component
// owns only terminal presentation: the geometry arrives as the frame plan's
// [`TablePanelProjection`], the page names its own columns, builds its header
// row and every visible row from its own domain facts, and the empty/failure
// state text stays with the page. Window clipping and the selection highlight
// are single-sourced here so the pages cannot drift.

/// What [`render_windowed_table`] painted for this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WindowedTableOutcome {
    /// `total == 0`: the honest empty/failure state panel owns `state_area`
    /// and no table painted. Callers skip furniture that presumes rows (the
    /// Services log band) while furniture that only surrounds the table (the
    /// boot timeline, the Users feedback line) keeps its own placement.
    StatePanel,
    /// The windowed table painted; page furniture draws into its own,
    /// disjoint areas.
    Table,
}

/// The standard bordered-table header vocabulary shared by the flat
/// inventory pages: plain column labels, with the active keyboard sort
/// marked by a direction arrow and the active column's emphasis. One
/// delegation to the renderer's [`header_row`] keeps the arrow strings and
/// the style pairs single-sourced.
pub(super) fn sort_header_row<'a, const N: usize>(
    headers: [&'a str; N],
    theme: TuiTheme,
    sort: Option<(usize, taskmanager_shell::SortDir)>,
) -> Row<'a> {
    super::header_row(headers, theme.accent, theme.color(Color::White), sort)
}

/// Every axis of one windowed-table paint, named: geometry from the frame
/// plan, the column and header contract prebuilt by the page (the header is a
/// finished [`Row`] so pages with dynamic column inventories — the
/// Applications column visibility, the per-row sparkline — can express it),
/// and the honest state text for the zero-row branch.
pub(super) struct WindowedTableProps<'a> {
    pub(super) theme: TuiTheme,
    /// The frame plan's table projection: painted area, canonical row count,
    /// and the committed visible window. Pages restate the plan's own values
    /// as this typed input — the window is never recomputed at the render
    /// boundary.
    pub(super) panel: TablePanelProjection,
    pub(super) title: &'a str,
    /// The finished header row (labels + optional sort marker + margins).
    pub(super) header: Row<'a>,
    /// One constraint per column, in cell order.
    pub(super) widths: Vec<Constraint>,
    /// Blanks between columns. The inventory pages share the product-wide
    /// two-blank gutter; the Applications table adapts it to terminal width.
    pub(super) column_spacing: u16,
    /// Where the state panel paints when `panel.total == 0`; a page may widen
    /// this beyond `panel.area` (the Services page folds the log band's slot
    /// into its empty state).
    pub(super) state_area: Rect,
    /// The caller-derived empty/failure text — never fabricated here.
    pub(super) state_message: &'a str,
}

/// Paint the shared bordered table over the frame plan's projection, or the
/// honest state panel when the page has no rows. `row_cells` receives the
/// absolute canonical row index for every index in the committed window, so
/// only visible rows are materialized.
pub(super) fn render_windowed_table<'a>(
    frame: &mut Frame<'_>,
    props: WindowedTableProps<'a>,
    row_cells: impl FnMut(usize) -> Row<'a>,
) -> WindowedTableOutcome {
    if props.panel.total == 0 {
        render_empty_panel(
            frame,
            props.theme,
            props.state_area,
            props.title,
            props.state_message,
        );
        return WindowedTableOutcome::StatePanel;
    }
    // Fail-closed bounding: the frame plan never emits a window past `total`,
    // but the render boundary re-bounds the slice so a stale projection can
    // neither panic nor paint fabricated rows. A plan-derived window is
    // unaffected.
    let end = props.panel.window.end.min(props.panel.total);
    let start = props.panel.window.start.min(end);
    let rows: Vec<Row<'a>> = (start..end).map(row_cells).collect();
    let table = Table::new(rows, props.widths)
        .header(props.header)
        .row_highlight_style(
            Style::new()
                .bg(props.theme.highlight_bg)
                .fg(props.theme.color(Color::White)),
        )
        .column_spacing(props.column_spacing)
        .highlight_symbol("› ")
        .highlight_spacing(HighlightSpacing::Always)
        .block(panel(props.title, props.theme));
    let mut state = TableState::default().with_selected(Some(props.panel.window.selected));
    frame.render_stateful_widget(table, props.panel.area, &mut state);
    WindowedTableOutcome::Table
}

/// Render the containers overlay centred over `area`.
#[cfg(test)]
#[allow(dead_code)]
pub fn render_containers_overlay(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    render_containers_overlay_at(
        frame,
        app,
        theme,
        super::planned_popup(
            area,
            crate::TuiInputScope::LocalSurface(crate::TuiSurfaceKind::Containers),
        ),
    );
}

pub(super) fn render_containers_overlay_at(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    popup: Rect,
) {
    let inner = Modal::new(theme, IconId::Process, t("containers.title")).render(frame, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(8), Constraint::Length(3)]).areas(inner);

    match app.shell.projection().containers.as_ref() {
        None => {
            let lines = vec![
                Line::from(Span::styled(
                    t("containers.telemetry_not_collected"),
                    Style::new().fg(theme.dim),
                )),
                Line::from(Span::styled(
                    t("containers.accrual_hint"),
                    Style::new().fg(theme.dim),
                )),
            ];
            frame.render_widget(Paragraph::new(lines), body);
        }
        Some(rollup) => {
            render_rollup(frame, rollup, theme, body);
        }
    }

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            KeyHint::line(
                theme,
                crate::command_palette::surface_hint_pairs(
                    crate::command_palette::TuiSurfaceScope::StatusOverlay,
                    crate::command_palette::TuiSurfaceAction::ToggleContainers,
                ),
            ),
        ])
        .alignment(Alignment::Center),
        footer,
    );
}

fn render_rollup(frame: &mut Frame<'_>, rollup: &ContainerRollup, theme: TuiTheme, area: Rect) {
    let state_line = state_line(rollup, theme);
    let [state, table_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(4)]).areas(area);
    frame.render_widget(Paragraph::new(state_line), state);

    if rollup.containers.is_empty() {
        let message = match classify_device_state(&rollup.state) {
            DeviceHealth::Healthy => t("containers.none_running"),
            _ => t("containers.none_listed"),
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(message, Style::new().fg(theme.dim))),
            ]),
            table_area,
        );
        return;
    }

    let (shown, hidden) = container_row_window(rollup.containers.len());
    let mut rows: Vec<Row<'_>> = rollup.containers[..shown]
        .iter()
        .map(|container| {
            Row::new([
                Cell::from(container.name.as_str()),
                Cell::from(runtime_label(container.runtime.as_ref(), theme)),
                Cell::from(
                    container
                        .cpu_percentage
                        .current_value()
                        .copied()
                        .map_or_else(missing_value, |value| format!("{value:>6.1}%")),
                ),
                Cell::from(
                    container
                        .memory_bytes
                        .current_value()
                        .copied()
                        .map_or_else(missing_value, bytes),
                ),
                Cell::from(t("containers.pids_count").replacen(
                    "{}",
                    &container.member_pids.len().to_string(),
                    1,
                )),
            ])
        })
        .collect();
    if hidden > 0 {
        rows.push(Row::new([
            Cell::from(more_rows_label(hidden)),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ]));
    }
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(28),
            Constraint::Length(18),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(10),
        ],
    )
    // Same two-blank gutter as the process/services tables — column
    // separation is a product-wide readability rule.
    .column_spacing(2)
    .header(
        Row::new([
            t("containers.name"),
            t("containers.runtime"),
            t("containers.cpu"),
            t("containers.memory"),
            t("containers.members"),
        ])
        .style(Style::new().fg(theme.accent).add_modifier(Modifier::BOLD))
        .bottom_margin(1),
    )
    .row_highlight_style(
        Style::new()
            .bg(theme.highlight_bg)
            .fg(theme.color(Color::White)),
    );
    frame.render_widget(table, table_area);
}

fn state_line(rollup: &ContainerRollup, theme: TuiTheme) -> Line<'static> {
    let (label, color) = match classify_device_state(&rollup.state) {
        DeviceHealth::Healthy => (
            t("containers.source_healthy").replacen("{}", &rollup.containers.len().to_string(), 1),
            theme.good,
        ),
        DeviceHealth::Stale => (t("containers.source_stale").to_owned(), theme.warn),
        DeviceHealth::PermissionDenied => (
            t("containers.source_permission_denied").to_owned(),
            theme.danger,
        ),
        DeviceHealth::MissingTool => (t("containers.source_missing_tool").to_owned(), theme.warn),
        DeviceHealth::Unsupported => (t("containers.source_unsupported").to_owned(), theme.dim),
    };
    Line::from(Span::styled(
        format!("{} {label}", theme.glyph(IconId::Process)),
        Style::new().fg(color),
    ))
}

fn runtime_label(runtime: Option<&IsolationKind>, theme: TuiTheme) -> Span<'static> {
    match runtime {
        Some(kind) => Span::styled(kind_label(kind), Style::new().fg(theme.dim)),
        None => Span::styled(t("containers.unknown_runtime"), Style::new().fg(theme.warn)),
    }
}

/// TUI-local label for the typed runtime family (the shared layer has no
/// presentation mapping for this enum).
fn kind_label(kind: &IsolationKind) -> &'static str {
    match kind {
        IsolationKind::Docker => "docker",
        IsolationKind::Podman => "podman",
        IsolationKind::Kubernetes => "k8s",
        IsolationKind::Lxc => "lxc",
        IsolationKind::SystemdNspawn => "systemd-nspawn",
        IsolationKind::Flatpak => "flatpak",
        IsolationKind::Snap => "snap",
        IsolationKind::Wsl => "wsl",
        IsolationKind::OtherContainer => "container",
    }
}

fn more_rows_label(hidden: usize) -> String {
    t("common.more_rows").replace("{count}", &hidden.to_string())
}

#[cfg(test)]
#[path = "../../tests/gui/ui/containers_tests.rs"]
mod tests;
