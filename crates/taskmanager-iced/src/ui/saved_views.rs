//! Independent saved-view review with fixed transfers around complete, scrollable rows.
//!
//! The module also owns the WHEEL GUARD (ICED-051): the compact presets
//! ribbon is a horizontal scroller that sits directly above the process
//! table, and a wheel gesture it cannot consume used to fall through to
//! that table and scroll it. The guard consumes the gesture at the ribbon's
//! own bounds, so the table's view stays still while the pointer is over
//! the ribbon.

use super::components::IcedElement;
use super::overlays::bounded_modal_overlay;
use crate::app::{FocusTarget, Message};
use crate::focus;
use iced::Length;
use iced::advanced::widget::{self, Tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout};
use iced::widget::{column, row, scrollable, text};
use iced::{Element, Event, Rectangle, Size};
use taskmanager_application::i18n::t;
use taskmanager_shell::saved_views::{feedback_text, review_rows};

/// The wheel guard: a wrapper that captures the wheel events over its own
/// bounds so they never reach the widgets underneath.
///
/// The content is updated FIRST (so a horizontal gesture still scrolls the
/// ribbon itself) and the gesture is captured afterwards: a vertical
/// gesture, or a horizontal one at the end of the track, no longer falls
/// through to the list below.
pub(crate) struct WheelGuard<'a> {
    content: Element<'a, Message, iced::Theme, iced::Renderer>,
}

impl<'a> WheelGuard<'a> {
    /// Wrap one widget in the wheel guard.
    pub(crate) fn new(content: Element<'a, Message, iced::Theme, iced::Renderer>) -> Self {
        Self { content }
    }
}

impl<'a> Widget<Message, iced::Theme, iced::Renderer> for WheelGuard<'a> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<()>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if matches!(
            event,
            Event::Mouse(iced::mouse::Event::WheelScrolled { .. })
        ) && cursor.is_over(layout.bounds())
        {
            // The gesture belongs to this band: capture it so the widgets
            // underneath (the process table) never see it.
            shell.capture_event();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
}

/// Wrap one widget in the wheel guard (ICED-051).
pub(crate) fn wheel_guard<'a>(
    content: Element<'a, Message, iced::Theme, iced::Renderer>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    Element::new(WheelGuard::new(content))
}

pub(super) fn render(app: &crate::IcedApp) -> IcedElement<'_> {
    let mut body = column![text(t("saved_views.help"))]
        .spacing(12)
        .width(Length::Fill);
    for preset in review_rows(&app.saved_views) {
        let mut actions = row![focus::dynamic_button(
            app.theme(),
            FocusTarget::SavedViewPreset(preset.id),
            t("common.apply").into(),
            Message::ApplySavedView(preset.id),
            false
        )]
        .spacing(6);
        if preset.is_user_saved() {
            actions = actions.push(focus::dynamic_button(
                app.theme(),
                FocusTarget::SavedViewRemove(preset.id),
                t("common.remove").into(),
                Message::DeleteSavedView(preset.id),
                false,
            ));
        }
        body = body.push(
            column![
                text(preset.display_name()),
                text(format!(
                    "{} · {} · {}",
                    preset.filter.label(),
                    preset.sort_col.label(),
                    if preset.sort_asc { "↑" } else { "↓" }
                )),
                actions.wrap()
            ]
            .spacing(4),
        );
    }
    if let Some(feedback) = app.saved_view_feedback {
        body = body.push(text(feedback_text(feedback)));
    }
    let actions = vec![
        focus::dynamic_button(
            app.theme(),
            FocusTarget::SavedViewSaveCurrent,
            t("saved_views.save_current").into(),
            Message::SaveCurrentProcessView,
            false,
        ),
        focus::dynamic_button(
            app.theme(),
            FocusTarget::SavedViewExport,
            t("common.export").into(),
            Message::ExportSavedViews,
            false,
        ),
        focus::dynamic_button(
            app.theme(),
            FocusTarget::SavedViewImport,
            t("common.import").into(),
            Message::ImportSavedViews,
            false,
        ),
    ];
    bounded_modal_overlay(
        app,
        t("saved_views.title"),
        // The modal's own scroll region owns the wheel gestures over it: a
        // gesture the region cannot consume (at its end) never scrolls the
        // page behind the modal.
        wheel_guard(
            scrollable(body)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
        ),
        actions,
    )
}

#[cfg(test)]
#[path = "../../tests/gui/ui/saved_views_wheel_guard_tests.rs"]
mod wheel_guard_tests;
