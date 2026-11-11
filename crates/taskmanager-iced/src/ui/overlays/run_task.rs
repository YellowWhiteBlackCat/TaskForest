//! "Run New Task" modal overlay for launching an unprivileged command.

use iced::widget::{column, container, row, text, text_editor};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_theme::tokens;

use super::modal_overlay;
use crate::app::{FocusTarget, Message};
use crate::focus;
use taskmanager_theme::Theme;

/// State backing the Run New Task modal.
#[derive(Clone, Debug)]
pub struct RunTaskState {
    pub command: text_editor::Content,
    pub error_msg: Option<String>,
}

impl Default for RunTaskState {
    fn default() -> Self {
        Self {
            command: text_editor::Content::new(),
            error_msg: None,
        }
    }
}

impl RunTaskState {
    /// The current command draft as one owned string. The dialog's native
    /// editor owns caret/selection; this projection is the sole reducer input.
    #[must_use]
    pub fn command(&self) -> String {
        self.command.text()
    }

    pub fn perform(&mut self, action: text_editor::Action) {
        self.command.perform(action);
    }

    pub fn clear_command(&mut self) {
        self.command = text_editor::Content::new();
    }
}

pub fn run_task_overlay<'a>(
    theme_snapshot: &'a Theme,
    state: &'a RunTaskState,
    appear: f32,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let input = crate::ui::components::multi_line_text_input(
        theme_snapshot,
        Some(FocusTarget::RunTaskCommandInput),
        &state.command,
        t("search.run_command"),
        Message::UpdateRunTaskEditor,
        Some(Message::SubmitRunTask),
        false,
    );

    let mut body_items: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = vec![input];

    if let Some(err) = &state.error_msg {
        let danger_color = crate::theme_binding::color(theme_snapshot.palette().danger);
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
        t("search.run_command"),
        content.into(),
        appear,
    )
}
