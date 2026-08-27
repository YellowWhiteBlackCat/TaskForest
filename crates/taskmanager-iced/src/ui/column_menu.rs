//! Iced Applications column-visibility menu.
//!
//! The table owns its layout, while this small menu owns only the renderer
//! local visibility set. The Name column is mandatory, matching the GPUI
//! identity-column rule. The menu is also the keyboard-accessible sizing
//! path: every visible resizable column gets a stepper pair (narrow/widen)
//! that publishes the shared `ResizeProcessColumn` transition in fixed
//! steps, so a keyboard user can reach the same widths a header-edge drag
//! produces without pointer input.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_shell::SortCol;
use taskmanager_theme::{Theme, tokens};

use crate::app::{FocusTarget, Message, keyboard_resize_width};
use crate::{IcedApp, focus, theme};

pub(super) fn render<'a>(
    app: &IcedApp,
    theme_snapshot: &'a Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    if !app.process_columns_menu_open() {
        return column![].into();
    }
    let hidden = &app.process_presentation.hidden_columns;
    let entries: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> =
        crate::ui::applications::apps_columns(true)
            .into_iter()
            .map(|(column, _)| {
                let checked = !hidden.contains(&column);
                let label = format!("{} {}", if checked { "✓" } else { "·" }, column.label());
                focus::dynamic_button(
                    theme_snapshot,
                    FocusTarget::ProcessColumnToggle(column),
                    label,
                    Message::ToggleProcessColumn(column),
                    false,
                )
            })
            .chain([
                focus::dynamic_button(
                    theme_snapshot,
                    FocusTarget::ProcessColumnsClose,
                    t("common.reset").to_owned(),
                    Message::ResetProcessColumns,
                    false,
                ),
                focus::dynamic_button(
                    theme_snapshot,
                    FocusTarget::ProcessColumnsClose,
                    t("common.cancel").to_owned(),
                    Message::CloseProcessColumnsMenu,
                    false,
                ),
            ])
            .collect();
    // The keyboard sizing path: one stepper pair per VISIBLE resizable column
    // (a hidden column has no header to drag, so it gets no stepper either;
    // the identity column is never resizable). Each press publishes the shared
    // resize transition at the column's current rendered width ± one step;
    // the readout gives a keyboard user the feedback a drag gets visually.
    let steppers: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> =
        crate::ui::applications::apps_columns(true)
            .into_iter()
            .filter(|(column, _)| {
                crate::ui::applications::column_resizable(*column) && !hidden.contains(column)
            })
            .map(|(column, _)| {
                stepper_group(theme_snapshot, column, app.process_column_width(column))
            })
            .collect();
    let mut menu = column![
        row![
            text(t("proc.choose_columns")).size(f32::from(tokens::FONT_12)),
            text(t("common.actions")).size(f32::from(tokens::FONT_12))
        ]
        .spacing(8),
        row(entries).spacing(6),
    ];
    if !steppers.is_empty() {
        menu = menu.push(row(steppers).spacing(6));
    }
    container(menu.spacing(6).padding(8))
        .style(move |_| theme::panel_style(theme_snapshot))
        .width(Length::Fill)
        .into()
}

/// One column's stepper pair: a muted `label widthpx` readout plus focusable
/// narrow/widen buttons. The button payloads are absolute widths resolved
/// from the app's current width rule, so each activation steps exactly once
/// and the rebuilt view re-derives the next step's target.
fn stepper_group<'a>(
    theme_snapshot: &'a Theme,
    column: SortCol,
    current: f32,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    row![
        text(format!("{} {:.0}px", column.label(), current))
            .size(f32::from(tokens::FONT_12))
            .color(theme::muted_text_color(theme_snapshot)),
        focus::dynamic_button(
            theme_snapshot,
            FocusTarget::ProcessColumnNarrow(column),
            "−".to_owned(),
            Message::ResizeProcessColumn {
                column,
                width: keyboard_resize_width(current, false),
            },
            false,
        ),
        focus::dynamic_button(
            theme_snapshot,
            FocusTarget::ProcessColumnWiden(column),
            "+".to_owned(),
            Message::ResizeProcessColumn {
                column,
                width: keyboard_resize_width(current, true),
            },
            false,
        ),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center)
    .into()
}
