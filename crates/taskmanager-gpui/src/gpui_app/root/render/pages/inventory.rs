//! Inventory-page projections materialized from the shared shell store.

use gpui::{Context, Div, Entity, Window, px};
use taskmanager_core::core::services::ServiceStatus;
use taskmanager_theme::Theme;
use taskmanager_ui::{layout::PageScaffold, primitives::button::ButtonState};

use super::{Hover, RootView, elements, i18n};
use crate::gpui_app::{
    root::responsive::PageLayoutBudget, services_view, startup_view, users_view,
};

impl RootView {
    pub(super) fn render_services_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        theme: &Theme,
        hovered: Option<&Hover>,
        retry_button: Entity<ButtonState>,
        page_padding: f32,
    ) -> Div {
        let selected = self.selected_service.clone();
        let services = self.services_rc().clone();
        let sources = self.service_sources_rc().clone();
        let table = self
            .services_table
            .get_or_insert_with(|| services_view::init_table_entity(*theme, cx))
            .clone();
        PageScaffold::new(
            services_view::render_services(
                services_view::ServicesViewProps {
                    theme,
                    items: &services,
                    sources: &sources,
                    selected: selected.as_ref(),
                    hovered: hovered.cloned(),
                    filter: self.services_state.filter,
                    query: &self.services_state.query,
                    rows: self.services_rows(),
                    feedback: self.services_feedback(),
                    search_input: self
                        .services_search
                        .get_or_insert_with(|| services_view::init_search_entity(cx))
                        .clone(),
                    table_entity: table,
                    retry_button,
                },
                window,
                cx,
            ),
            px(page_padding),
        )
        .footer(elements::status_bar(
            theme,
            &[
                format!("{}: {}", i18n::t("svc.total"), services.len()),
                format!(
                    "{}: {}",
                    i18n::t("svc.running"),
                    services
                        .iter()
                        .filter(|service| service.status == ServiceStatus::Active)
                        .count()
                ),
            ],
            &[],
        ))
        .render()
    }

    pub(super) fn render_startup_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        theme: &Theme,
        hovered: Option<&Hover>,
        retry_button: Entity<ButtonState>,
        page_layout: PageLayoutBudget,
    ) -> Div {
        let selected = self.selected_startup.clone();
        let entries = self.startup_entries_rc().clone();
        let sources = self.startup_sources_rc().clone();
        let table = self
            .startup_table
            .get_or_insert_with(|| startup_view::init_table_entity(*theme, cx))
            .clone();
        PageScaffold::new(
            startup_view::render_startup(
                startup_view::StartupViewProps {
                    theme,
                    entries: &entries,
                    sources: &sources,
                    selected: selected.as_ref(),
                    hovered: hovered.cloned(),
                    filter: self.startup_state.filter,
                    query: &self.startup_state.query,
                    rows: self.startup_rows(),
                    boot_baseline: self.capture_evidence.startup_boot_baseline(),
                    feedback: self.startup_feedback(),
                    search_input: self
                        .startup_search
                        .get_or_insert_with(|| startup_view::init_search_entity(cx))
                        .clone(),
                    table_entity: table,
                    evidence: self.startup_boot_evidence(),
                    retry_button,
                    layout: startup_view::StartupPageBudget::from_page_layout(page_layout),
                },
                window,
                cx,
            ),
            px(page_layout.page_padding),
        )
        .render()
    }

    pub(super) fn render_users_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        theme: &Theme,
        hovered: Option<&Hover>,
        retry_button: Entity<ButtonState>,
        page_padding: f32,
    ) -> Div {
        let selected = self.selected_session.clone();
        let sources = self.session_sources_rc().clone();
        let table = self
            .users_table
            .get_or_insert_with(|| users_view::init_table_entity(*theme, cx))
            .clone();
        PageScaffold::new(
            users_view::render_users(
                users_view::UsersViewProps {
                    theme,
                    rows: self.sessions_rows(),
                    sources: &sources,
                    selected: selected.as_ref(),
                    feedback: self.session_feedback(),
                    hovered: hovered.cloned(),
                    search_query: self.process_query(),
                    table_entity: &table,
                    retry_button,
                },
                window,
                cx,
            ),
            px(page_padding),
        )
        .render()
    }
}
