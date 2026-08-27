//! "Run New Task" modal overlay for launching processes with optional elevation.

use iced::widget::{checkbox, column, container, row, text, text_input};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_theme::tokens;

use super::modal_overlay;
use crate::app::{FocusTarget, Message};
use crate::focus;
use crate::theme;

/// State backing the Run New Task modal.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunTaskState {
    pub command: String,
    pub as_admin: bool,
    pub error_msg: Option<String>,
}

pub fn run_task_overlay<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    state: &'a RunTaskState,
    appear: f32,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let input = text_input(t("search.run_command"), &state.command)
        .on_input(Message::UpdateRunTaskCommand)
        .on_submit(Message::SubmitRunTask)
        .padding(8)
        .size(f32::from(tokens::FONT_13));

    let admin_checkbox = row![
        checkbox(state.as_admin)
            .on_toggle(|_| Message::ToggleRunTaskAdmin)
            .size(f32::from(tokens::FONT_14)),
        text(t("proc.run_as_admin")).size(f32::from(tokens::FONT_12)),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    let mut body_items: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> =
        vec![input.into(), admin_checkbox.into()];

    if let Some(err) = &state.error_msg {
        let danger_color = theme::color(theme_snapshot.palette().danger);
        body_items.push(
            text(err.clone())
                .size(f32::from(tokens::FONT_12))
                .style(move |_| text::Style {
                    color: Some(danger_color),
                })
                .into(),
        );
    }

    let buttons = row![
        focus::ghost_button(
            theme_snapshot,
            FocusTarget::RunTaskCancel,
            t("common.cancel"),
            Message::CloseRunTask,
        ),
        focus::button(
            theme_snapshot,
            FocusTarget::RunTaskSubmit,
            t("common.run"),
            Message::SubmitRunTask,
            false,
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    body_items.push(
        container(buttons)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .into(),
    );

    let content = column(body_items).spacing(12).width(Length::Fixed(420.0));

    modal_overlay(
        theme_snapshot,
        t("proc.run_new_task"),
        "Enter executable path or command",
        content.into(),
        appear,
    )
}
