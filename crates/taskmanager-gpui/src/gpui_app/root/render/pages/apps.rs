//! Apps and application-history page bodies.
//!
//! The process table (with its status bar) and the replay-backed application
//! history table (with its status bar), from the frame-local render context.

use gpui::{Context, Div, ParentElement, Styled, Window, div, px};
use taskmanager_shell::SortDir;
use taskmanager_shell::presentation::process_anomaly_summary;
use taskmanager_shell::presentation::uninterruptible_process_count;
use taskmanager_ui::layout::PageScaffold;

use super::{PageRenderContext, RootView, init_search_entity, vm};
use crate::gpui_app::app_history_view;
use crate::gpui_app::root::{elements, i18n, processes_view};

impl RootView {
    /// Render the Apps page: the process table and its status bar.
    pub(super) fn render_apps_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        context: PageRenderContext<'_>,
    ) -> Div {
        let PageRenderContext {
            theme: t,
            snapshot: snap,
            hovered,
            selected_identity,
            layout,
            page_padding,
            content_width,
            presentation,
            ..
        } = context;
        let graph_cache = std::rc::Rc::clone(&self.graph_cache);
        let appearance = presentation.appearance;
        let page_metrics = vm::process_page_metrics(snap);
        let process_count = self.processes().len();
        let uninterruptible_count = uninterruptible_process_count(
            self.projection()
                .processes
                .as_ref()
                .map(|items| items.as_slice()),
        );
        let anomaly_summary = process_anomaly_summary(
            self.projection()
                .processes
                .as_ref()
                .map(|items| items.as_slice()),
        );
        let hidden_cols = self.effective_process_hidden_cols();
        let (sort_col, sort_direction) = self.effective_process_sort();
        let sort_asc = matches!(sort_direction, SortDir::Asc);
        let (rows, _pids, query) = self.processes_projection();
        let application_count = self.process_application_count();
        // Own TextInput backed by this window's persistent per-window
        // state (lazily created on the first Apps render). The field
        // owns its focus, caret blink, and key handling; an
        // InputEvent::Change subscription (see init_search_entity)
        // mirrors its value into the shell-owned process query so the
        // shared match grammar keeps filtering.
        let search_input = self
            .search_input
            .get_or_insert_with(|| init_search_entity(cx))
            .clone();
        PageScaffold::new(
            processes_view::render_processes(
                processes_view::ProcessesViewProps {
                    theme: t,
                    application_count,
                    process_count,
                    uninterruptible_count,
                    anomaly_summary,
                    search_input: &search_input,
                    rows: &rows,
                    query: &query,
                    selected_identity,
                    control: self.process_control_availability(),
                    selected_row: self.selected_process_row(),
                    selected_identities: self.selected_process_identities(),
                    hovered: hovered.cloned(),
                    sort_col,
                    sort_asc,
                    filter: self.process_status_filter(),
                    affinity_identity: self.process_affinity_identity(),
                    affinity_state: self.shell.process_affinity_state(),
                    affinity_cpus: &self.processes_state.affinity_editor.cpus,
                    affinity_hover: self.processes_state.affinity_editor.hover,
                    hidden_cols: &hidden_cols,
                    swap_auto_hidden: page_metrics.swap_auto_hidden,
                    batch_history_available: !self.process_batch_history.is_empty(),
                    col_widths: &self.processes_state.col_widths,
                    viewport_width: content_width,
                    processes_scroll: &self.processes_scroll.vertical,
                    horizontal_scroll: &self.processes_scroll.horizontal,
                    column_cursor: self.processes_state.column_cursor,
                    graph_cache: graph_cache.clone(),
                    gray_zero_values: presentation.gray_zero_values,
                    density: appearance.density,
                    ui_size: appearance.ui_size,
                    presentation: processes_view::ProcessChromePresentation::from_page_layout(
                        layout,
                    ),
                },
                window,
                cx,
            ),
            px(page_padding),
        )
        .footer(elements::status_bar(
            t,
            &[
                format!("{}: {}", i18n::t("proc.total"), process_count),
                format!(
                    "{}: {}",
                    i18n::t("proc.running"),
                    self.running_process_count()
                ),
            ],
            &[
                format!("{}: {}", i18n::t("common.cpu"), page_metrics.cpu_usage),
                format!(
                    "{}: {}",
                    i18n::t("common.memory"),
                    page_metrics.memory_usage
                ),
            ],
        ))
        .render()
    }

    /// Render the AppHistory page: the replay-backed application table and its
    /// status bar.
    pub(super) fn render_app_history_page(
        &mut self,
        cx: &mut Context<Self>,
        context: PageRenderContext<'_>,
    ) -> Div {
        let PageRenderContext {
            theme: t,
            layout,
            page_padding,
            presentation,
            ..
        } = context;
        let graph_cache = std::rc::Rc::clone(&self.graph_cache);
        let appearance = presentation.appearance;
        let history = self
            .history_runtime
            .replay()
            .application_history_projection(self.history_runtime.application_history_capability());
        let history_rows = self.app_history_rows(&history);
        let history_count = history.rows.len();
        PageScaffold::new(
            div().flex().flex_col().flex_1().min_h(px(0.0)).child(
                app_history_view::render_app_history(app_history_view::AppHistoryViewProps {
                    theme: t,
                    projection: history,
                    rows: history_rows,
                    scroll: &self.app_history_scroll,
                    entity: cx.entity(),
                    graph_cache: graph_cache.clone(),
                    ui_size: appearance.ui_size,
                    columns: app_history_view::AppHistoryColumns::from_page_layout(layout),
                    units: self.display_units(),
                }),
            ),
            px(page_padding),
        )
        .footer(elements::status_bar(
            t,
            &[format!(
                "{}: {}",
                i18n::t("history.application.title"),
                history_count
            )],
            &[],
        ))
        .render()
    }
}
