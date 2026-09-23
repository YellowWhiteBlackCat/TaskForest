//! Per-page body rendering for the root application shell (line split).
//!
//! This root owns the frame-local render context and dispatch; each page body
//! lives in a sibling submodule.

use super::super::{Hover, PresentationSnapshot, RootView, SelectedDevice, TopPage};
use super::init_search_entity;
use crate::gpui_app::root::{containers_view, elements, i18n, responsive};
use gpui::{AppContext, Context, Div, Pixels, Window, px};
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_telemetry_store::TelemetryStore;
use taskmanager_theme::Theme;
use taskmanager_ui::layout::PageScaffold;
use taskmanager_ui::primitives::button::ButtonState;

mod apps;
mod inventory;
mod performance;
mod system;
mod vm;

pub(crate) struct PageBodyFrame<'a> {
    pub theme: &'a Theme,
    pub snapshot: &'a SystemSnapshot,
    pub telemetry: &'a TelemetryStore,
    pub hovered: Option<&'a Hover>,
    pub selected: SelectedDevice,
    pub frame: responsive::FrameBudget,
    pub corner_radius_factor: f32,
    pub selected_identity: Option<ProcessLiveKey>,
}

/// Frame-local render inputs shared by the per-page body helpers: the borrowed
/// projection plus the slot budgets already derived from the frame. Bundling
/// them keeps every helper's argument list small and keeps the page arms from
/// recomputing the same shell geometry.
struct PageRenderContext<'a> {
    theme: &'a Theme,
    snapshot: &'a SystemSnapshot,
    telemetry: &'a TelemetryStore,
    hovered: Option<&'a Hover>,
    selected: SelectedDevice,
    frame: responsive::FrameBudget,
    corner_radius_factor: f32,
    selected_identity: Option<ProcessLiveKey>,
    layout: responsive::PageLayoutBudget,
    page_padding: f32,
    content_width: Pixels,
    presentation: PresentationSnapshot,
}

impl RootView {
    /// Render the active page from one immutable, frame-local projection.
    /// `PageBodyFrame` is stack-owned; it is neither an allocation nor cache.
    pub(crate) fn render_page_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        frame: PageBodyFrame<'_>,
    ) -> Div {
        let layout = frame.frame.page_layout();
        let page_padding = frame.frame.content.page_padding;
        let content_width = px(f32::from(frame.frame.content.size.width));
        let source_retry_button = self
            .source_retry_button
            .get_or_insert_with(|| cx.new(|cx| ButtonState::new(cx)))
            .clone();
        let context = PageRenderContext {
            theme: frame.theme,
            snapshot: frame.snapshot,
            telemetry: frame.telemetry,
            hovered: frame.hovered,
            selected: frame.selected,
            frame: frame.frame,
            corner_radius_factor: frame.corner_radius_factor,
            selected_identity: frame.selected_identity,
            layout,
            page_padding,
            content_width,
            presentation: self.presentation_snapshot(),
        };
        match self.page {
            TopPage::Performance => self.render_performance_page(cx, context),
            TopPage::Apps => self.render_apps_page(window, cx, context),
            TopPage::Services => self.render_services_page(
                window,
                cx,
                context.theme,
                context.hovered,
                source_retry_button.clone(),
                context.page_padding,
            ),
            TopPage::System => self.render_system_page(cx, context),
            TopPage::Startup => self.render_startup_page(
                window,
                cx,
                context.theme,
                context.hovered,
                source_retry_button.clone(),
                context.layout,
            ),
            TopPage::Users => self.render_users_page(
                window,
                cx,
                context.theme,
                context.hovered,
                source_retry_button,
                context.page_padding,
            ),
            TopPage::AppHistory => self.render_app_history_page(cx, context),
            TopPage::Containers => self.render_containers_page(context),
        }
    }

    /// Render the Containers page body.
    fn render_containers_page(&mut self, context: PageRenderContext<'_>) -> Div {
        let PageRenderContext {
            theme: t,
            page_padding,
            ..
        } = context;
        PageScaffold::new(
            containers_view::render_containers(t, self.containers(), self.display_units()),
            px(page_padding),
        )
        .render()
    }
}
