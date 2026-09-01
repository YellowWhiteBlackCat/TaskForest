//! The Applications-page row builders extracted from [`super`]: the
//! projection dispatch (`project_row_element`) plus the canonical aggregate /
//! process-tree row renderers and their shared cell helpers. Moved verbatim
//! so `applications.rs` stays under the source-size budget; the projection
//! dispatch and the `RowRender` context are re-exported so callers keep
//! compiling unchanged.

use super::process_projection::{ProcessRowFacts, ProjectedRow};
use super::*;
use iced::widget::canvas;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_core::core::process::aggregate::AggregateMetric;
use taskmanager_shell::presentation::{
    missing_value, optional_bytes, optional_duration, optional_nice,
};
use taskmanager_theme::tokens;

/// Render one projected row: the process lookup joins the projection's
/// `flat_index` back into the shared list so the row always shows the process
/// the projection named (a missing row is skipped defensively, never panics).
/// `row_index` is the row's position in the projected render order — the
/// zebra parity source, so stripes follow what the user sees.
pub(super) fn project_row_element(
    ctx: &RowRender,
    projection: &ProcessProjection,
    row: &ProjectedRow,
    row_index: usize,
) -> Option<Element<'static, Message, iced::Theme, iced::Renderer>> {
    let zebra = theme::zebra_index(row_index);
    match row {
        ProjectedRow::GroupHeader { .. } => Some(group_header_row(ctx, row)),
        ProjectedRow::Tree { flat_index, .. } => {
            let process = projection.process_facts(*flat_index)?;
            Some(tree_node_row(ctx, row, process, zebra))
        }
    }
}

/// One recursive process row: the depth indent, the expand/collapse chevron (a leaf
/// gets a dot so columns align), and the shared process cells
/// layout. Clicking a node with children toggles its subtree
/// ([`Message::ActivateTreeNode`]) and selects the parent; a leaf click
/// selects the row. `index` is the row's flat position in the shared process
/// list (the projection guarantees clicks and keyboard land on the pid the
/// row renders).
fn tree_node_row(
    ctx: &RowRender,
    row: &ProjectedRow,
    process: &ProcessRowFacts,
    zebra: bool,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let ProjectedRow::Tree {
        flat_index,
        pid,
        depth,
        has_children,
        collapsed,
        cells,
        ..
    } = row
    else {
        // The dispatch arms guarantee a Tree row here; an empty element is the
        // safe fallback so the projection can never crash the renderer.
        return text("").into();
    };
    let theme_snapshot = ctx.theme;
    let selected = row
        .row_key()
        .and_then(|key| key.live_key())
        .is_some_and(|identity| ctx.selected_identities.contains(&identity))
        || ctx
            .selected_row
            .is_some_and(|selected| Some(selected) == row.row_key());
    let row_padding = theme::row_padding(ctx.compact);
    let marker = if *has_children {
        if *collapsed { "▶" } else { "▼" }
    } else {
        "·"
    };
    let guide_prefix = if *depth > 0 {
        format!("{}└─ {} ", "  ".repeat(depth.saturating_sub(1)), marker)
    } else {
        format!("{marker} ")
    };
    let mut name_row = iced::widget::row![
        iced::widget::text(guide_prefix).size(f32::from(taskmanager_theme::tokens::FONT_CAPTION)),
        crate::ui::components::highlight::cell(
            &theme_snapshot,
            process.name.as_str(),
            ctx.query.as_str(),
            ctx.search_active,
            Length::Shrink,
        ),
    ]
    .spacing(2)
    .align_y(iced::Alignment::Center);

    if *has_children && *collapsed {
        name_row = name_row.push(
            iced::widget::container(
                iced::widget::text("subtree")
                    .size(f32::from(tokens::FONT_9))
                    .style(move |_| iced::widget::text::Style {
                        color: Some(theme::muted_text_color(&theme_snapshot)),
                    }),
            )
            .padding([0, 4])
            .style(move |_| iced::widget::container::Style {
                background: Some(iced::Background::Color(crate::theme_binding::color(
                    theme_snapshot.shade,
                ))),
                border: iced::Border {
                    radius: 3.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        );
    }

    let mut elements: Vec<Element<'static, Message, iced::Theme, iced::Renderer>> = vec![
        text_cell(ctx, cells.pid.clone(), SortCol::Pid),
        iced::widget::container(name_row)
            .width(Length::Fixed(ctx.resolved_column_width(SortCol::Name)))
            .into(),
        zero_tinted_text(ctx, cells.cpu.clone(), process.cpu_zero, SortCol::Cpu),
        // Per-row CPU-history sparkline: Tree leaves only (a parent row carries
        // no single history) — same gate as the gpui per-row sparkline.
        process_sparkline_cell(
            theme_snapshot,
            &process.cpu_history,
            row.row_key().and_then(|key| match key {
                taskmanager_shell::ProcessRowId::Process(identity) => Some(identity),
                taskmanager_shell::ProcessRowId::Category(_)
                | taskmanager_shell::ProcessRowId::Application(_) => None,
            }),
            !*has_children,
        ),
        zero_tinted_text(
            ctx,
            cells.memory.clone(),
            process.memory_zero,
            SortCol::Memory,
        ),
        zero_tinted_text(ctx, cells.pss.clone(), process.pss_zero, SortCol::Pss),
    ];
    if ctx.swap_visible {
        elements.push(zero_tinted_text(
            ctx,
            cells.swap.clone(),
            process.swap_zero,
            SortCol::Swap,
        ));
    }
    // Pre-formatted by the projection (`RowCells`); only the zero-tint
    // booleans are computed here.
    elements.push(zero_tinted_text(
        ctx,
        cells.disk_read.clone(),
        process.disk_read_zero,
        SortCol::DiskRead,
    ));
    elements.push(zero_tinted_text(
        ctx,
        cells.disk_write.clone(),
        process.disk_write_zero,
        SortCol::DiskWrite,
    ));
    elements.push(zero_tinted_text(
        ctx,
        cells.cpu_time.clone(),
        process.cpu_time_zero,
        SortCol::CpuTime,
    ));
    elements.push(zero_tinted_text(
        ctx,
        cells.threads.clone(),
        process.threads_zero,
        SortCol::Threads,
    ));
    elements.push(text_cell(ctx, cells.user.clone(), SortCol::User));
    // GPUI-parity advanced cells, in lockstep with apps_columns. fds/nice ride
    // the typed observations: an unavailable value renders the projection's
    // honest dash and only a measured zero takes the gray-zero tint.
    elements.push(text_cell(ctx, cells.status.clone(), SortCol::State));
    elements.push(zero_tinted_text(
        ctx,
        cells.fds.clone(),
        process.fds_zero,
        SortCol::Fds,
    ));
    elements.push(zero_tinted_text(
        ctx,
        cells.nice.clone(),
        process.nice_zero,
        SortCol::Nice,
    ));
    elements.push(text_cell(
        ctx,
        cells.start_clock.clone(),
        SortCol::StartTime,
    ));
    let elements = visible_column_elements(elements, ctx.swap_visible, &ctx.hidden_columns, false);
    let row_content = container(
        iced::widget::row(elements)
            .padding(row_padding)
            .spacing(theme::table_column_spacing()),
    )
    .style(move |_| theme::row_style(&theme_snapshot, selected, zebra))
    .width(Length::Fill);
    if *has_children {
        // The whole row activates the node: one click toggles the subtree AND
        // selects the parent (a parent's flat index is stable across collapse,
        // so both can happen on one activation). The header-button styling
        // reads as an actionable parent row.
        iced::widget::button(row_content)
            .on_press(Message::ActivateTreeNode {
                identity: row.row_key().and_then(|key| key.live_key()),
                flat_index: *flat_index,
            })
            .style(move |_theme, status| {
                theme::header_button_style(&theme_snapshot, status, selected)
            })
            .padding(0)
            .into()
    } else {
        let identity = row.row_key().and_then(|key| key.live_key());
        let row_element = match identity {
            Some(identity) => focus::selectable_row_with_menu(
                &theme_snapshot,
                AppPage::Applications,
                *flat_index,
                row_content.into(),
                Message::OpenProcessRowMenu { identity },
            ),
            None => focus::selectable_row(
                &theme_snapshot,
                AppPage::Applications,
                *flat_index,
                row_content.into(),
            ),
        };
        if ctx.open_menu_identity == identity {
            // The open menu floats on its own row: anchored by the popover
            // primitive, dismissible by an outside press, and never clipped
            // by the table viewport the way the old inline panel was.
            let panel = super::process_menu::panel(theme_snapshot, process.name.clone(), *pid);
            crate::ui::components::Popover::new(row_element, panel, Message::CloseProcessRowMenu)
                .into()
        } else {
            row_element
        }
    }
}

/// Render-time parameters shared by every Flat/member row in one Applications
/// frame, bundled so the row builders stay under clippy's argument budget and
/// the call sites stay readable. The lifetime ties to the theme snapshot (the
/// only borrow the returned cells carry).
#[derive(Clone)]
pub(crate) struct RowRender {
    pub(crate) theme: taskmanager_theme::Theme,
    pub(crate) query: String,
    pub(crate) search_active: bool,
    pub(crate) swap_visible: bool,
    pub(crate) compact: bool,
    pub(crate) ui_size: taskmanager_theme::tokens::UiSize,
    /// The full multi-select target set. A row highlights when its pid is a
    /// member (covers the keyboard anchor AND every Ctrl/Shift-selected row).
    pub(crate) selected_identities: std::rc::Rc<std::collections::HashSet<ProcessLiveKey>>,
    pub(crate) selected_row: Option<taskmanager_shell::ProcessRowId>,
    /// GPUI-parity zero-value policy: when enabled, measured zero resource
    /// values render in the muted foreground instead of their category color
    /// (unavailable values stay dashes and are never dimmed as zero).
    pub(crate) gray_zero: bool,
    /// Applications columns hidden by the renderer-local chooser. Name is
    /// still forced visible by the final cell filter.
    pub(crate) hidden_columns: std::rc::Rc<std::collections::HashSet<SortCol>>,
    /// Session-local column width overrides (header-drag results). Cells
    /// resolve through [`Self::resolved_column_width`] — the same
    /// override-or-default source the header cells read.
    pub(crate) column_widths: std::rc::Rc<crate::app::ColumnWidthOverrides>,
    /// The live identity whose process context menu is currently open, if any. The row
    /// carrying it hosts the floating menu panel; every other row renders
    /// unchanged. Part of the lazy-body key so opening/closing the menu
    /// rebuilds exactly the materialized window.
    pub(crate) open_menu_identity: Option<ProcessLiveKey>,
}

impl RowRender {
    /// Resolved width of one column for this frame's body cells: the drag
    /// override when present, the contract default otherwise. The header cells
    /// resolve through the same override state, which keeps the sticky header
    /// pixel-aligned with the body after a drag.
    pub(crate) fn resolved_column_width(&self, column: SortCol) -> f32 {
        self.column_widths
            .get(column)
            .unwrap_or_else(|| column_width(column))
    }
}

/// A numeric table cell that honors the gray-zero-values preference: a
/// confirmed zero renders muted; `None` values are rendered by the caller as
/// dashes and never reach this helper (so an unavailable value can never be
/// mistaken for a measured zero). Width and alignment come from the shared
/// contract kit ([`RowRender::resolved_column_width`]/[`column_alignment`])
/// — the same sources the header cells read, which keeps header/body
/// boundaries pixel-aligned under the sticky header.
fn zero_tinted_text(
    ctx: &RowRender,
    label: String,
    is_zero: bool,
    column: SortCol,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let mut cell = text(label);
    if ctx.gray_zero && is_zero {
        cell = cell.color(theme::muted_text_color(&ctx.theme));
    }
    column_cell(ctx, cell, column)
}

/// A plain text cell bounded and aligned by the column's contract spec.
fn text_cell(
    ctx: &RowRender,
    label: String,
    column: SortCol,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    column_cell(ctx, text(label), column)
}

/// Bound one cell element to its column's resolved width and align it
/// (numeric columns right-align so digits stack; text columns stay left).
fn column_cell(
    ctx: &RowRender,
    cell: iced::widget::Text<'static>,
    column: SortCol,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    container(cell)
        .width(Length::Fixed(ctx.resolved_column_width(column)))
        .align_x(column_alignment(column))
        .into()
}

/// The per-column visibility tags zipped against the row cells. Tree/flat
/// rows carry one cell per column; grouped aggregate rows FUSE Pid+Name into
/// one leading cell, so their tag list starts at `Name` (the fused cell is
/// the identity cell and never hides) — without that shift every trailing
/// cell would be filtered by its neighbor's hidden state.
fn visible_column_tags(swap_visible: bool, grouped: bool) -> Vec<Option<SortCol>> {
    // Grouped aggregate rows fuse Pid+Name into one leading identity cell.
    let mut columns = if grouped {
        vec![Some(SortCol::Name), Some(SortCol::Cpu)]
    } else {
        vec![Some(SortCol::Pid), Some(SortCol::Name), Some(SortCol::Cpu)]
    };
    columns.push(None); // the Trend sparkline cell
    columns.extend([Some(SortCol::Memory), Some(SortCol::Pss)]);
    if swap_visible {
        columns.push(Some(SortCol::Swap));
    }
    columns.extend([
        Some(SortCol::DiskRead),
        Some(SortCol::DiskWrite),
        Some(SortCol::CpuTime),
        Some(SortCol::Threads),
        Some(SortCol::User),
        Some(SortCol::State),
        Some(SortCol::Fds),
        Some(SortCol::Nice),
        Some(SortCol::StartTime),
    ]);
    columns
}

pub(crate) fn visible_column_elements(
    elements: Vec<Element<'static, Message, iced::Theme, iced::Renderer>>,
    swap_visible: bool,
    hidden: &std::collections::HashSet<SortCol>,
    grouped: bool,
) -> Vec<Element<'static, Message, iced::Theme, iced::Renderer>> {
    elements
        .into_iter()
        .zip(visible_column_tags(swap_visible, grouped))
        .filter_map(|(element, column)| {
            column
                .is_none_or(|column| column == SortCol::Name || !hidden.contains(&column))
                .then_some(element)
        })
        .collect()
}

/// One Applications-table data row, shared by the Flat view and the expanded
/// member rows of a grouped view. The columns/cells are identical in both
/// modes (the reuse rule): PID, name, CPU%, memory, PSS, [swap], disk rates,
/// cumulative CPU time, thread count, owner — each an honest dash when the
/// provider supplied no observation. The row stays keyboard-reachable through
/// the focusable wrapper; `index` is the row's position in
/// `shell.visible_processes()` so selection maps back to the shared shell
/// state regardless of which view mode rendered it. `zebra` (the row's render
/// parity from [`theme::zebra_index`]) stripes even rows.
/// One hierarchy aggregate header row: the shared process cell set (so the
/// header lines up with the member rows beneath it), with the group's summed
/// observations where a member row would show its own. `×N` marks a
/// multi-member group; `▶`/`▼` shows the expansion state. The whole row is a
/// button: application aggregates select their PID-less semantic row and
/// toggle expansion; structural category/type headers only toggle expansion.
fn group_header_row(
    ctx: &RowRender,
    row: &ProjectedRow,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let ProjectedRow::GroupHeader {
        flat_index: _,
        main_pid: _,
        row_key,
        name,
        expansion_key,
        member_count,
        expanded,
        metrics,
        user,
        status,
        nice,
        start_time_secs: _,
        start_clock,
    } = row
    else {
        // The dispatch arms guarantee a GroupHeader row here; an empty element
        // is the safe fallback so the projection can never crash the renderer.
        return text("").into();
    };
    let theme_snapshot = ctx.theme;
    let selected = row_key.is_some_and(|key| ctx.selected_row == Some(key));
    let marker = if *expanded { "▼" } else { "▶" };
    // The typed aggregate keeps the PSS-preferred display metric and the PSS
    // metric separate. Only current values become strings; stale and
    // unavailable values remain explicit dashes.
    let (cpu_text, cpu_zero) = aggregate_f32_text(&metrics.cpu, |cpu| format!("{cpu:>5.1}%"));
    let memory = metrics.memory_display.current_value().copied();
    let pss = metrics.memory_pss.current_value().copied();
    let swap = metrics.swap.current_value().copied();
    let disk_read = metrics.disk_read.current_value().copied();
    let disk_write = metrics.disk_write.current_value().copied();
    let cpu_time = metrics.cpu_time.current_value().copied();
    let threads = aggregate_count_text(&metrics.threads);
    let fds = aggregate_count_text(&metrics.fds);
    // The fused leading cell spans Pid+Name while Pid is visible and collapses
    // to the Name extent once Pid is hidden, so the identity column keeps its
    // boundary aligned with the member rows beneath it.
    let identity_width = if ctx.hidden_columns.contains(&SortCol::Pid) {
        ctx.resolved_column_width(SortCol::Name)
    } else {
        ctx.resolved_column_width(SortCol::Pid) + ctx.resolved_column_width(SortCol::Name)
    };
    let mut cells: Vec<Element<'static, Message, iced::Theme, iced::Renderer>> = vec![
        iced::widget::row![
            text(marker).size(f32::from(tokens::FONT_12)),
            text(name.clone()).size(f32::from(tokens::FONT_14)),
            text(format!("x{member_count}")).size(f32::from(tokens::FONT_12)),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .width(Length::Fixed(identity_width))
        .into(),
        zero_tinted_text(ctx, cpu_text, cpu_zero, SortCol::Cpu),
        // Aggregate group headers carry no single CPU history, so the Trend
        // column stays blank — but the same-width cell keeps the column
        // boundary pixel-aligned with the member rows beneath it.
        iced::widget::Space::new()
            .width(Length::Fixed(PROCESS_SPARK_WIDTH))
            .height(Length::Shrink)
            .into(),
        zero_tinted_text(
            ctx,
            optional_bytes(memory),
            memory == Some(0),
            SortCol::Memory,
        ),
        zero_tinted_text(ctx, optional_bytes(pss), pss == Some(0), SortCol::Pss),
    ];
    if ctx.swap_visible {
        cells.push(zero_tinted_text(
            ctx,
            optional_bytes(swap),
            swap == Some(0),
            SortCol::Swap,
        ));
    }
    cells.push(zero_tinted_text(
        ctx,
        optional_bytes(disk_read),
        disk_read == Some(0),
        SortCol::DiskRead,
    ));
    cells.push(zero_tinted_text(
        ctx,
        optional_bytes(disk_write),
        disk_write == Some(0),
        SortCol::DiskWrite,
    ));
    cells.push(zero_tinted_text(
        ctx,
        optional_duration(cpu_time),
        cpu_time == Some(0),
        SortCol::CpuTime,
    ));
    cells.push(zero_tinted_text(
        ctx,
        threads.0,
        threads.1,
        SortCol::Threads,
    ));
    cells.push(text_cell(ctx, user.clone(), SortCol::User));
    cells.push(text_cell(ctx, status.clone(), SortCol::State));
    cells.push(zero_tinted_text(ctx, fds.0, fds.1, SortCol::Fds));
    cells.push(zero_tinted_text(
        ctx,
        optional_nice(*nice),
        *nice == Some(0),
        SortCol::Nice,
    ));
    cells.push(text_cell(ctx, start_clock.clone(), SortCol::StartTime));
    let cells = visible_column_elements(cells, ctx.swap_visible, &ctx.hidden_columns, true);
    iced::widget::button(container(
        iced::widget::row(cells)
            .padding(theme::row_padding(ctx.compact))
            .spacing(theme::table_column_spacing()),
    ))
    .on_press(Message::ToggleGroupExpansion {
        name: expansion_key.clone(),
        row_key: *row_key,
    })
    .style(move |_theme, status| theme::header_button_style(&theme_snapshot, status, selected))
    .padding(0)
    .width(Length::Fill)
    .into()
}

fn aggregate_f32_text(
    metric: &AggregateMetric<f32>,
    format: impl FnOnce(f32) -> String,
) -> (String, bool) {
    match metric.current_value().copied() {
        Some(value) => (format(value), value == 0.0),
        None => (missing_value(), false),
    }
}

fn aggregate_count_text(metric: &AggregateMetric<u64>) -> (String, bool) {
    match metric.current_value().copied() {
        Some(value) => (value.to_string(), value == 0),
        None => (missing_value(), false),
    }
}

/// One per-row CPU sparkline cell, or a same-width blank container when the
/// row carries no single history (aggregate headers, Tree parents, the lone
/// member of a singleton group). `show` is the view-mode gate the caller
/// resolves; the blank branch keeps the column boundary pixel-aligned so the
/// Trend header stays over the sparkline column for every row.
fn process_sparkline_cell(
    theme_snapshot: taskmanager_theme::Theme,
    history: &std::rc::Rc<[f32]>,
    identity: Option<ProcessLiveKey>,
    show: bool,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    if let Some(identity) = identity.filter(|_| show) {
        canvas::Canvas::new(ProcessCpuSparkline::new(
            std::rc::Rc::clone(history),
            crate::theme_binding::color(theme_snapshot.cpu),
            identity,
        ))
        .width(Length::Fixed(PROCESS_SPARK_WIDTH))
        .height(Length::Fixed(PROCESS_SPARK_HEIGHT))
        .into()
    } else {
        iced::widget::Space::new()
            .width(Length::Fixed(PROCESS_SPARK_WIDTH))
            .height(Length::Shrink)
            .into()
    }
}
