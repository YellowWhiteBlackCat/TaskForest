// test-intent: behavior
//! Column-contract and sticky-shell behavior for the Iced tables: the
//! Applications table's widths/alignment/hideability must be
//! `PROCESS_COLUMNS` contract truth, the local inventory specs share one
//! flexible remainder column, and the sticky-header body window keeps the
//! clamped, prefix-free geometry.

use super::{ColumnWidth, VirtualWindow, virtual_table};
use crate::app::Message;
use iced::Element;
use taskmanager_shell::SortCol;

use crate::ui::applications::rows::visible_column_elements;
use crate::ui::applications::{
    apps_columns, column_alignment, column_hideable, sort_col_contract_id,
};
use crate::ui::tables::services_columns;
use crate::ui::users::users_columns;

/// Every renderable Applications column (except the shell-superset `Pss`)
/// resolves to a `PROCESS_COLUMNS` row and carries exactly its default width,
/// and every contract column is projected by the table (Swap only on a host
/// with swap). This is the Iced-side contract gate mirroring the GPUI one.
#[test]
fn process_column_widths_and_tokens_are_contract_truth() {
    let with_swap = apps_columns(true);
    for (column, width) in &with_swap {
        if *column == SortCol::Pss {
            continue; // shell superset: no contract row by design
        }
        let spec = taskmanager_ui_contract::find(sort_col_contract_id(*column))
            .unwrap_or_else(|| panic!("{column:?} must map onto PROCESS_COLUMNS"));
        assert_eq!(
            *width, spec.default_width,
            "{column:?} width must come from the contract"
        );
    }

    let tokens: std::collections::HashSet<&str> = with_swap
        .iter()
        .map(|(column, _)| sort_col_contract_id(*column))
        .collect();
    for spec in taskmanager_ui_contract::PROCESS_COLUMNS {
        assert!(
            tokens.contains(spec.id),
            "contract column {} must have a table column",
            spec.id
        );
    }

    let without_swap: Vec<SortCol> = apps_columns(false).into_iter().map(|(c, _)| c).collect();
    assert!(!without_swap.contains(&SortCol::Swap));
    assert!(with_swap.iter().any(|(c, _)| *c == SortCol::Swap));
}

/// Numeric columns right-align exactly where the contract says `numeric`;
/// the shell-superset `Pss` column renders byte values and stays right-aligned.
#[test]
fn numeric_alignment_follows_the_contract_numeric_flag() {
    use iced::alignment::Horizontal;
    for (column, _) in apps_columns(true) {
        if column == SortCol::Pss {
            continue;
        }
        let spec = taskmanager_ui_contract::find(sort_col_contract_id(column))
            .unwrap_or_else(|| panic!("{column:?} must map onto PROCESS_COLUMNS"));
        let expected = if spec.numeric {
            Horizontal::Right
        } else {
            Horizontal::Left
        };
        assert_eq!(column_alignment(column), expected, "{column:?} alignment");
    }
    assert_eq!(column_alignment(SortCol::Pss), Horizontal::Right);
}

/// Hideability is contract truth: the identity column (`Name`) is never
/// hideable and the toggle message only hides hideable columns.
#[test]
fn column_menu_toggles_only_contract_hideable_columns() {
    for (column, _) in apps_columns(true) {
        if column == SortCol::Pss {
            continue;
        }
        let spec = taskmanager_ui_contract::find(sort_col_contract_id(column))
            .unwrap_or_else(|| panic!("{column:?} must map onto PROCESS_COLUMNS"));
        assert_eq!(
            column_hideable(column),
            spec.hideable,
            "{column:?} hideability"
        );
    }
    assert!(
        !column_hideable(SortCol::Name),
        "the identity column never hides"
    );

    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::ToggleProcessColumn(SortCol::Name));
    assert!(
        !app.process_presentation
            .hidden_columns
            .contains(&SortCol::Name),
        "toggling Name must be a no-op, not a hide"
    );
    let _ = app.update(Message::ToggleProcessColumn(SortCol::Cpu));
    assert!(
        app.process_presentation
            .hidden_columns
            .contains(&SortCol::Cpu)
    );
    let visible = crate::ui::applications::visible_apps_columns(
        true,
        &app.process_presentation.hidden_columns,
    );
    assert!(visible.iter().any(|(c, _)| *c == SortCol::Name));
    assert!(!visible.iter().any(|(c, _)| *c == SortCol::Cpu));
}

/// The sticky-header body window is exactly the unprefixed row window: the
/// header lives outside the scrollable, so the same offset materializes a
/// different (smaller) range than the old in-flow-header geometry did.
#[test]
fn sticky_body_window_has_no_header_prefix() {
    let sticky = VirtualWindow::for_sticky_rows(1_000, 4_530.0, 240.0, 30.0);
    let unprefixed = VirtualWindow::for_rows(1_000, 4_530.0, 240.0, 30.0, 0.0);
    assert_eq!(sticky, unprefixed);

    let in_flow_header = VirtualWindow::for_rows(1_000, 4_530.0, 240.0, 30.0, 32.0);
    assert_ne!(
        sticky.key(),
        in_flow_header.key(),
        "the sticky body window must not carry the header prefix geometry"
    );
}

/// Hostile/stale offsets and invalid extents stay clamped on the sticky path.
#[test]
fn sticky_window_clamps_hostile_and_invalid_offsets() {
    let clamped = VirtualWindow::for_sticky_rows(8, f32::MAX, 200.0, 24.0);
    assert_eq!(clamped.end, 8);
    assert_eq!(clamped.bottom, 0.0);

    let invalid = VirtualWindow::for_sticky_rows(8, f32::NAN, 0.0, 0.0);
    assert!(invalid.top.is_finite() && invalid.bottom.is_finite());
}

/// The sticky composition shell constructs for every scroll direction it
/// dispatches on (vertical stack, nested both-axis, horizontal fallback)
/// without panicking or dropping the tracked body scrollable wiring.
#[test]
fn sticky_shell_composes_for_every_scroll_direction() {
    use iced::Length;
    use iced::widget::scrollable::{Direction, Scrollbar};

    for direction in [
        Direction::Vertical(Scrollbar::default()),
        Direction::Horizontal(Scrollbar::default()),
        Direction::Both {
            vertical: Scrollbar::default(),
            horizontal: Scrollbar::default(),
        },
    ] {
        let header: Element<'static, Message, iced::Theme, iced::Renderer> =
            iced::widget::text("header").into();
        let body: Element<'static, Message, iced::Theme, iced::Renderer> =
            iced::widget::text("body").into();
        let _shell: Element<'static, Message, iced::Theme, iced::Renderer> = virtual_table(
            iced::widget::Id::unique(),
            header,
            body,
            Length::Fixed(1_488.0),
            direction,
            Message::ApplicationsScrolled,
        );
    }
}

/// Grouped aggregate rows fuse Pid+Name into one leading identity cell, so
/// their visibility tags start at `Name`: hiding `StartTime` must drop the
/// start-clock cell itself. (The pre-fix shared tag list was off by one —
/// every trailing cell was governed by its left neighbor's hidden state, and
/// hiding StartTime dropped nothing.)
#[test]
fn grouped_rows_hide_cells_by_their_own_column() {
    fn dummy_cells(count: usize) -> Vec<Element<'static, Message, iced::Theme, iced::Renderer>> {
        (0..count)
            .map(|index| iced::widget::text(index.to_string()).into())
            .collect()
    }
    // Grouped rows with swap carry 15 cells (the fused identity cell replaces
    // the separate Pid + Name cells).
    let hidden = std::collections::HashSet::from([SortCol::StartTime]);
    let survivors = visible_column_elements(dummy_cells(15), true, &hidden, true).len();
    assert_eq!(
        survivors, 14,
        "hiding StartTime must drop exactly the start-clock cell"
    );

    // Flat rows keep one cell per column (16 with swap): hiding Cpu drops only
    // its own cell and never the Trend sparkline.
    let flat_survivors = visible_column_elements(
        dummy_cells(16),
        true,
        &std::collections::HashSet::from([SortCol::Cpu]),
        false,
    )
    .len();
    assert_eq!(flat_survivors, 15);
}

/// The local inventory specs (Services/Users/Startup) share the column
/// vocabulary: every full-layout table keeps exactly one flexible remainder
/// column (compact Services is all-fixed by design — the description column
/// is dropped at the minimum viewport), and the reserved drag-hook flag never
/// marks a Fill column resizable.
#[test]
fn inventory_column_specs_keep_one_flexible_remainder() {
    let services_full = services_columns(false);
    assert_eq!(
        services_full.description.map(|spec| spec.width),
        Some(ColumnWidth::Fill)
    );
    let services_compact = services_columns(true);
    assert!(services_compact.description.is_none());
    assert_eq!(services_compact.actions.width, ColumnWidth::Fixed(300.0));
    assert_eq!(services_full.actions.width, ColumnWidth::Fixed(450.0));

    let users = users_columns();
    let startup = crate::ui::startup_table::startup_columns();
    // (table, expected flexible columns): compact Services drops its
    // description column entirely, so its layout is all-fixed.
    for (name, expected_fills) in [
        ("Users", 1),
        ("Startup", 1),
        ("Services (compact)", 0),
        ("Services (full)", 1),
    ] {
        let specs: Vec<&super::TableColumn> = match name {
            "Users" => [
                &users.session,
                &users.name,
                &users.seat,
                &users.tty,
                &users.remote,
                &users.logon,
            ]
            .into_iter()
            .collect(),
            "Startup" => [
                &startup.status,
                &startup.name,
                &startup.impact,
                &startup.source,
                &startup.control,
                &startup.command,
            ]
            .into_iter()
            .collect(),
            "Services (compact)" => vec![
                &services_compact.name,
                &services_compact.status,
                &services_compact.actions,
            ],
            _ => vec![
                &services_full.name,
                &services_full.status,
                services_full.description.as_ref().unwrap(),
                &services_full.actions,
            ],
        };
        let fills = specs
            .iter()
            .filter(|spec| spec.width == ColumnWidth::Fill)
            .count();
        assert_eq!(
            fills, expected_fills,
            "{name} flexible-column count drifted"
        );
        for spec in specs {
            assert_eq!(
                spec.resizable,
                spec.width != ColumnWidth::Fill,
                "{name}/{} drag hook must not cover the flexible column",
                spec.id
            );
            assert!(
                spec.label.starts_with("common.")
                    || spec.label.starts_with("users.")
                    || spec.label.starts_with("startup."),
                "{name}/{} label must stay a shared-catalog key",
                spec.id
            );
        }
    }
}
