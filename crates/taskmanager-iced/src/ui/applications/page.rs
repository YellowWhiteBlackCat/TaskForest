//! Applications page assembly and lazy table body.

use super::*;
use crate::IcedApp;
use crate::saved_views::{PresetsRibbonState, presets_ribbon};
use iced::widget::column;
use iced::widget::scrollable::Direction;
use iced::{Alignment, Element, Length, Padding, Renderer, Theme};
use taskforest_core::core::process::ProcessBatchAction;
use zhuge_ui_core::virtual_list::VirtualWindow;
use zhuge_ui_kit_iced::surface::{self, StatePanelGeometry};
use zhuge_ui_kit_iced::virtual_list::{virtual_table, virtual_table_body};

use crate::ui::virtual_list::resolve_table_window;
use taskmanager_shell::presentation::uninterruptible_process_summary;
use taskmanager_shell::presentation::{process_anomaly_summary, swap_column_visible};
use taskmanager_shell::{ProcessControlScope, SortDir};
use taskmanager_theme::tokens::{FONT_13, FONT_CAPTION};
use taskmanager_ui_contract::IconId;

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
    let swap_visible = swap_column_visible(shell.projection().snapshot.as_ref());

    let search: Element<'_, Message, Theme, Renderer> = if shell.search_active() {
        row![
            crate::ui::components::plain_text_input(
                theme_snapshot,
                Some(FocusTarget::SearchTrigger),
                t("search.processes"),
                shell.query.as_str(),
                Message::SearchChanged,
                Some(Message::CloseSearch),
                false,
            ),
            crate::ui::components::icon_button(
                theme_snapshot,
                FocusTarget::SearchClose,
                IconId::Close,
                t("common.close"),
                Message::CloseSearch,
                true,
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
            text({
                let count = format!("{} {}", visible_indices.len(), t("proc.process_count"));
                let processes = shell
                    .projection()
                    .processes
                    .as_ref()
                    .map(|items| items.as_slice());
                [
                    uninterruptible_process_summary(processes),
                    process_anomaly_summary(processes),
                ]
                .into_iter()
                .flatten()
                .fold(count, |line, summary| format!("{line} · {summary}"))
            }),
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
    // The sticky header row shares the body rows' row padding and column
    // gutter, so a header cell and the body cells under it share one left
    // inset and one column pitch.
    let header_row = applications_header_row(
        theme_snapshot,
        &visible_columns,
        active_sort,
        app.compact_density(),
        table_extent,
    );

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
        tooltip_capture_marker: app.tooltip_capture_marker(),
    };
    let row_height = application_row_height(row_context.compact);
    let (scroll_y, viewport_height) = app.applications_virtual_scroll();
    // Sticky header: the header row sits outside the body scrollable, so the
    // body window carries no header prefix — offsets are pure row extents.
    // The body materializes the visible rows plus the product's pre-render
    // band, so a fast wheel flick that outruns the one-frame scroll report
    // still paints materialized rows instead of a blank block.
    let window = resolve_table_window(projection.len(), scroll_y, viewport_height, row_height)
        .materialized();

    let table_body: Element<'_, Message, Theme, Renderer> = if projection.is_empty() {
        // GPUI-parity empty state: a centered message instead of a blank
        // header + scrollable (render.rs: `proc.no_processes` /
        // `proc.no_processes_match`).
        container(surface::state_panel(
            Some(empty_state_message(shell.query.as_str())),
            None,
            None,
            StatePanelGeometry {
                padding: Padding::ZERO,
                spacing: 0.0,
                title_size: f32::from(FONT_13),
                ..StatePanelGeometry::default()
            },
            |_| container::Style::default(),
        ))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| theme::panel_style(theme_snapshot))
        .into()
    } else {
        let key = applications_virtual_table_key(projection_generation, &row_context, window);
        let open_menu = row_context.open_menu_identity.is_some()
            || row_context.tooltip_capture_marker.is_some();
        let model = ApplicationsTableModel {
            projection: Rc::clone(&projection),
            render: row_context,
            window,
        };
        if open_menu {
            // Lazy's overlay adapter drops nested widget operations. Keep the
            // same bounded virtual window mounted directly while a process
            // menu is open so focus and action operations reach its Popover.
            render_table_body(&model)
        } else {
            iced::widget::lazy(key, move |_| render_table_body(&model)).into()
        }
    };
    let table = virtual_table(
        app.applications_scroll_id(),
        header_row,
        table_body,
        Length::Fixed(table_extent),
        Direction::Both {
            vertical: crate::theme::scrollbar(),
            horizontal: crate::theme::scrollbar(),
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
        let control = shell.process_control_availability();
        let tree_selected = matches!(control.scope(), Some(ProcessControlScope::Tree));
        let selected_target_count = control.target_count();
        // The "End task" action wears the destructive accent only when the
        // selection holds a process in the platform's "not responding"
        // state (the core's uninterruptible wait): that is the case where
        // the user has to reach for the destructive path.
        let end_task_destructive = emphasis::selection_is_not_responding(shell);
        let mut actions: Vec<Element<'_, Message, Theme, Renderer>> = Vec::new();
        if !tree_selected {
            actions.push(focus::button_enabled(
                theme_snapshot,
                FocusTarget::EndTask,
                t("proc.end_task"),
                Message::RequestEndTask,
                end_task_destructive,
                control.is_ready(),
            ));
        }
        actions.push(focus::button_enabled(
            theme_snapshot,
            FocusTarget::KillProcess,
            t("proc.kill"),
            Message::RequestProcessBatch(ProcessBatchAction::Kill),
            true,
            control.is_ready(),
        ));
        if !tree_selected {
            actions.push(focus::button_enabled(
                theme_snapshot,
                FocusTarget::OpenProcessLocation,
                t("proc.open_location"),
                Message::OpenProcessLocation,
                false,
                control.is_single_process(),
            ));
            actions.push(focus::button_enabled(
                theme_snapshot,
                FocusTarget::SearchProcessOnline,
                t("proc.search_online"),
                Message::SearchProcessOnline,
                false,
                control.is_single_process(),
            ));
        }
        actions.push(focus::button_enabled(
            theme_snapshot,
            FocusTarget::SuspendProcess,
            t("proc.suspend"),
            Message::RequestProcessBatch(ProcessBatchAction::Suspend),
            false,
            control.is_ready(),
        ));
        actions.push(focus::button_enabled(
            theme_snapshot,
            FocusTarget::ResumeProcess,
            t("proc.resume"),
            Message::RequestProcessBatch(ProcessBatchAction::Resume),
            false,
            control.is_ready(),
        ));
        if !tree_selected {
            actions.push(focus::button_enabled(
                theme_snapshot,
                FocusTarget::ProcessAffinityOpen,
                t("proc.affinity"),
                Message::OpenProcessAffinity,
                false,
                control.is_single_process(),
            ));
        }
        actions.push(focus::button_enabled(
            theme_snapshot,
            FocusTarget::ProcessEfficiencyMode,
            t("proc.efficiency_mode"),
            Message::RequestProcessBatch(ProcessBatchAction::SetEfficiencyMode(true)),
            false,
            control.is_ready(),
        ));
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
        let priority_picker: Element<'_, Message, Theme, Renderer> = if control.is_ready() {
            // The shared select owns the trigger paint (the ONE glass
            // material), the `ControlRole::Input` geometry and the
            // keyboard stop; this adapter only supplies the typed
            // options and the typed batch message.
            crate::ui::components::select(
                theme_snapshot,
                FocusTarget::ProcessPrioritySelect,
                &PRIORITY_OPTIONS[..],
                None::<&PriorityOption>,
                t("proc.choose_priority"),
                |choice: &PriorityOption| Message::RequestProcessBatch(choice.action()),
            )
        } else {
            text(t("proc.choose_priority")).into()
        };
        let priority_group: Element<'_, Message, Theme, Renderer> = iced::widget::container(
            row![
                text(t("proc.priority"))
                    .size(f32::from(FONT_CAPTION))
                    .color(theme::muted_text_color(theme_snapshot)),
                priority_picker,
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .width(Length::Fixed(priority_group_width(theme_snapshot)))
        .into();
        let hint: Element<'_, Message, Theme, Renderer> = iced::widget::container(
            text(selection_hint(selected_target_count))
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .width(Length::Shrink)
        .into();
        // The action strip shows COMPLETE groups only. The priority preset
        // and the selection hint live in the anchored overflow menu, so
        // the right edge never paints half a group (the GPUI action bar
        // keeps the same vocabulary in its anchored overflow menu).
        let overflow_trigger = focus::ghost_button(
            theme_snapshot,
            FocusTarget::ProcessActionsTrigger,
            t("common.actions"),
            Message::OpenProcessActionsMenu,
        );
        let overflow_panel = column![
            text(t("common.actions"))
                .size(f32::from(FONT_13))
                .color(theme::muted_text_color(theme_snapshot)),
            priority_group,
            hint,
        ]
        .spacing(6);
        let overflow: Element<'_, Message, Theme, Renderer> = if app.process_actions_menu_open() {
            let panel: Element<'_, Message, Theme, Renderer> =
                iced::widget::container(overflow_panel)
                    .style(move |_| theme::panel_style(theme_snapshot))
                    .padding(8)
                    .into();
            crate::ui::components::Popover::new(
                overflow_trigger,
                panel,
                Message::CloseProcessActionsMenu,
            )
            .into()
        } else {
            overflow_trigger
        };
        actions.push(overflow);
        // The full action vocabulary is one bounded horizontal group.
        // A plain Fill row squeezed the last groups into half-painted
        // fragments at the right edge; the shared strip keeps every
        // control's intrinsic width and gives the overflow its own
        // scroll owner without spending another table row.
        crate::ui::components::bounded_toolbar(theme_snapshot, actions)
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

    if app.compact_layout() {
        let top_strip = row![search, Space::new().width(Length::Fill), view_selector,]
            .spacing(8)
            .align_y(Alignment::Center);

        column![top_strip, status_selector, ribbon, table, action_bar,]
            .spacing(4)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
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
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/applications_priority_tests.rs"]
mod applications_priority_tests;
