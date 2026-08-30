//! Applications page assembly and lazy table body.

use super::*;
use crate::IcedApp;
use crate::saved_views::{PresetsRibbonState, presets_ribbon};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{column, text_input};
use iced::{Alignment, Element, Length, Renderer, Theme};
use taskmanager_core::core::process::{ProcessBatchAction, descendant_live_keys};

use taskmanager_shell::{ProcessRowId, SortDir};

/// Owned inputs for the lazy Applications body. The projection owns the
/// preformatted cells and the process facts/history required by row widgets;
/// the renderer-local context owns the small selection/theme/query state. The
/// whole model is cloned only as cheap `Rc`/small-value handles between view
/// calls, while the lazy widget retains its built body on an unchanged key.
#[derive(Clone)]
struct ApplicationsTableModel {
    projection: Rc<ProcessProjection>,
    render: RowRender,
    window: VirtualWindow,
}

/// Extend the stable body key with the materialized row range. Projection and
/// renderer changes retain the existing invalidation contract; scrolling only
/// invalidates the lazy body when the bounded window crosses a row boundary.
fn applications_virtual_table_key(
    generation: u64,
    render: &RowRender,
    window: VirtualWindow,
) -> u64 {
    virtual_table_key(applications_table_key(generation, render), window)
}

fn render_table_body(model: &ApplicationsTableModel) -> Element<'static, Message, Theme, Renderer> {
    let width = Length::Fixed(apps_table_width_with(
        model.render.swap_visible,
        model.render.hidden_columns.as_ref(),
        model.render.column_widths.as_ref(),
    ));
    virtual_table_body(model.window, width, |start, end| {
        applications_table_rows_range(&model.render, model.projection.as_ref(), start, end)
    })
}

/// The applications page: search field + the shared filtered/sorted process
/// table.
pub(crate) fn applications_page(app: &IcedApp) -> Element<'_, Message, Theme, Renderer> {
    let shell = &app.shell;
    let theme_snapshot = app.theme();
    let visible_indices = shell.visible_process_indices();
    // A measured zero swap total means the host has no swap device and the
    // Swap column is hidden. An unknown total keeps it visible so unavailable
    // values are not mistaken for confirmed absence.
    let swap_visible = projection::swap_column_visible(shell.projection().snapshot.as_ref());

    let search: Element<'_, Message, Theme, Renderer> = if shell.search_active() {
        row![
            text_input(t("search.processes"), shell.query.as_str())
                .on_input(Message::SearchChanged)
                .on_submit(Message::CloseSearch),
            focus::button(
                theme_snapshot,
                FocusTarget::SearchClose,
                t("common.close"),
                Message::CloseSearch,
                false,
            ),
        ]
        .spacing(4)
        .into()
    } else {
        row![
            focus::button(
                theme_snapshot,
                FocusTarget::SearchTrigger,
                t("search.processes_shortcut"),
                Message::FocusSearch,
                false,
            ),
            text(format!(
                "{} {}",
                visible_indices.len(),
                t("proc.process_count")
            )),
        ]
        .spacing(8)
        .into()
    };

    let active_sort = shell.process_sort;
    // One shared snapshot of the session-local column overrides per frame:
    // header cells, body rows and the table extent all resolve through it, so
    // a drag keeps the sticky header pixel-aligned with the body.
    let column_widths = Rc::new(app.process_column_sizing.overrides.clone());
    let table_extent = apps_table_width_with(
        swap_visible,
        &app.process_presentation.hidden_columns,
        column_widths.as_ref(),
    );
    let visible_columns = visible_apps_columns_with(
        swap_visible,
        &app.process_presentation.hidden_columns,
        column_widths.as_ref(),
    );
    let trend_index = trend_header_index_for(&visible_columns);
    let mut header_cells: Vec<Element<'_, Message, Theme, Renderer>> = visible_columns
        .into_iter()
        .map(|(column, width)| header_cell(theme_snapshot, column, width, active_sort))
        .collect();
    // The non-sortable Trend header sits immediately after the CPU column
    // (Pid, Name, Cpu → Trend) so it lines up with the per-row sparkline cell
    // below. It is NOT part of `apps_columns` (no `SortCol` — a per-row visual
    // has no underlying scalar to rank), mirroring the gpui processes_view
    // chrome (`i18n::t("proc.trend")`, width 56, left-aligned, muted).
    header_cells.insert(trend_index, trend_header_cell(theme_snapshot));
    // The same column gutter as the body rows (rows.rs) — header and body
    // must agree or the headers drift off their columns.
    let header_row = container(
        row(header_cells)
            .padding(4)
            .spacing(theme::table_column_spacing()),
    )
    .height(Length::Fixed(APPLICATION_HEADER_HEIGHT))
    .width(Length::Fixed(table_extent));

    let (projection, projection_generation) = app.projected_table_model();
    // The cached projection is byte-identical to what a fresh
    // `ProcessProjection::project_with_local_time(&rows, ...)` would produce
    // for these inputs — the cache only decides whether to rebuild, never
    // what to build
    // (round-3 vsync-frame memo).

    let row_context = RowRender {
        theme: *theme_snapshot,
        query: shell.query.clone(),
        search_active: shell.search_active(),
        swap_visible,
        compact: app.compact_density(),
        ui_size: app.ui_size(),
        selected_identities: Rc::new(shell.selected_identities().clone()),
        selected_row: shell.selected_row,
        gray_zero: app.preferences().gray_zero_values,
        hidden_columns: Rc::new(app.process_presentation.hidden_columns.clone()),
        column_widths,
        open_menu_identity: app.process_menu_identity(),
    };
    let row_height = application_row_height(row_context.compact);
    let (scroll_y, viewport_height) = app.applications_virtual_scroll();
    // Sticky header: the header row sits outside the body scrollable, so the
    // body window carries no header prefix — offsets are pure row extents.
    let window =
        VirtualWindow::for_sticky_rows(projection.len(), scroll_y, viewport_height, row_height);

    let table_body: Element<'_, Message, Theme, Renderer> = if projection.is_empty() {
        // GPUI-parity empty state: a centered message instead of a blank
        // header + scrollable (render.rs: `proc.no_processes` /
        // `proc.no_processes_match`).
        container(
            text(empty_state_message(shell.query.as_str()))
                .size(f32::from(taskmanager_theme::tokens::FONT_13)),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| theme::panel_style(theme_snapshot))
        .into()
    } else {
        let key = applications_virtual_table_key(projection_generation, &row_context, window);
        let model = ApplicationsTableModel {
            projection: Rc::clone(&projection),
            render: row_context,
            window,
        };
        iced::widget::lazy(key, move |_| render_table_body(&model)).into()
    };
    let table = virtual_table(
        app.applications_scroll_id(),
        header_row.into(),
        table_body,
        Length::Fixed(table_extent),
        Direction::Both {
            vertical: Scrollbar::default(),
            horizontal: Scrollbar::default(),
        },
        Message::ApplicationsScrolled,
    );

    let action_bar: Element<'_, Message, Theme, Renderer> = if let Some(intent) =
        shell.pending_batch()
    {
        tables::confirm_batch_bar(theme_snapshot, intent)
    } else if let Some(target) = shell.pending_end() {
        tables::confirm_bar(theme_snapshot, target)
    } else {
        let selected_application_root = shell.selected_row.and_then(ProcessRowId::application_root);
        let selected_target_count = selected_application_root
            .and_then(|root| {
                shell
                    .projection()
                    .processes
                    .as_deref()
                    .map(|processes| descendant_live_keys(processes, root).len())
            })
            .unwrap_or_else(|| shell.selected_identities().len());
        let mut actions: Vec<Element<'_, Message, Theme, Renderer>> = Vec::new();
        if selected_application_root.is_none() {
            actions.push(focus::button(
                theme_snapshot,
                FocusTarget::EndTask,
                t("proc.end_task"),
                Message::RequestEndTask,
                true,
            ));
        }
        actions.push(focus::button(
            theme_snapshot,
            FocusTarget::KillProcess,
            t("proc.kill"),
            Message::RequestProcessBatch(ProcessBatchAction::Kill),
            true,
        ));
        if selected_application_root.is_none() {
            actions.push(focus::button(
                theme_snapshot,
                FocusTarget::OpenProcessLocation,
                t("proc.open_location"),
                Message::OpenProcessLocation,
                false,
            ));
            actions.push(focus::button(
                theme_snapshot,
                FocusTarget::SearchProcessOnline,
                t("proc.search_online"),
                Message::SearchProcessOnline,
                false,
            ));
        }
        actions.push(focus::button(
            theme_snapshot,
            FocusTarget::SuspendProcess,
            t("proc.suspend"),
            Message::RequestProcessBatch(ProcessBatchAction::Suspend),
            false,
        ));
        actions.push(focus::button(
            theme_snapshot,
            FocusTarget::ResumeProcess,
            t("proc.resume"),
            Message::RequestProcessBatch(ProcessBatchAction::Resume),
            false,
        ));
        if selected_application_root.is_none() {
            actions.push(focus::button(
                theme_snapshot,
                FocusTarget::ProcessAffinityOpen,
                t("proc.affinity"),
                Message::OpenProcessAffinity,
                false,
            ));
        }
        let columns_trigger = focus::button(
            theme_snapshot,
            FocusTarget::ProcessColumnsTrigger,
            t("proc.choose_columns"),
            Message::OpenProcessColumnsMenu,
            false,
        );
        // The column chooser floats on its trigger: anchored below the
        // button and dismissed by an outside press, instead of shoving
        // the page column down while it is open.
        actions.push(if app.process_columns_menu_open() {
            crate::ui::components::Popover::new(
                columns_trigger,
                super::column_menu::render(app, theme_snapshot),
                Message::CloseProcessColumnsMenu,
            )
            .into()
        } else {
            columns_trigger
        });
        actions.push(focus::ghost_button(
            theme_snapshot,
            FocusTarget::RunTaskOpen,
            t("proc.run_new_task"),
            Message::OpenRunTask,
        ));
        actions.push(
            row![
                text(t("proc.priority"))
                    .size(f32::from(taskmanager_theme::tokens::FONT_CAPTION))
                    .color(theme::muted_text_color(theme_snapshot)),
                iced::widget::pick_list(
                    &PriorityChoice::ALL[..],
                    None::<PriorityChoice>,
                    |choice: PriorityChoice| Message::RequestProcessBatch(choice.action()),
                )
                .placeholder(t("proc.choose_priority")),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .into(),
        );
        actions.push(text(selection_hint(selected_target_count)).into());
        if app.compact_layout() {
            // Keep the action strip one bounded line so the Applications
            // table retains a real row viewport. The full action vocabulary
            // remains reachable through its own horizontal scroll axis.
            scrollable(row(actions).spacing(8))
                .direction(Direction::Horizontal(Scrollbar::default()))
                .height(Length::Fixed(40.0))
                .width(Length::Fill)
                .into()
        } else {
            row(actions).spacing(8).into()
        }
    };

    let view_selector = process_view_selector(theme_snapshot);
    let status_selector =
        process_status_filter_selector(theme_snapshot, app.process_status_filter());
    let ribbon = presets_ribbon(
        theme_snapshot,
        &app.saved_views,
        PresetsRibbonState {
            filter: app.process_status_filter(),
            sort: shell.process_sort.0,
            ascending: shell.process_sort.1 == SortDir::Asc,
            feedback: app.saved_view_feedback,
            compact: app.compact_layout(),
        },
    );

    column![
        search,
        ribbon,
        view_selector,
        status_selector,
        table,
        action_bar,
    ]
    .spacing(8)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
