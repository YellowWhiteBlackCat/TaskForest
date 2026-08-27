//! Iced Services-row context-menu state and dispatch.
//!
//! The menu is renderer-local, while the selected provider identity and the
//! confirmation/effect path remain shared with the Services action bar.

use super::*;

impl IcedApp {
    pub(super) fn open_service_row_menu(&mut self, visual_index: usize, source_index: usize) {
        let service = self
            .shell
            .projection()
            .services
            .as_deref()
            .and_then(|services| services.get(source_index))
            .cloned();
        let Some(service) = service else {
            return;
        };
        let _ = self.shell.select_row(visual_index);
        self.open_context_menu(ContextMenu::Service {
            source_index,
            service,
        });
    }

    pub(super) fn close_service_row_menu(&mut self) {
        if self.context_menu_kind() == Some(ContextMenuKind::Service) {
            self.dismiss_context_menu();
        }
    }

    pub(super) fn request_service_action_at(
        &mut self,
        index: usize,
        action: ServiceAction,
    ) -> Option<PlatformEffect> {
        let service = self
            .shell
            .projection()
            .services
            .as_deref()
            .and_then(|services| services.get(index))
            .cloned();
        service.and_then(|service| self.request_service_action_for(service, action))
    }

    pub(super) fn request_service_action_for(
        &mut self,
        service: ServiceItem,
        action: ServiceAction,
    ) -> Option<PlatformEffect> {
        let captured =
            !service.id.as_str().is_empty() && self.shell.select_service_control(&service, action);
        if captured {
            self.shell.apply_action(AppAction::RequestServiceControl)
        } else {
            self.shell.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                "No service selected for control",
            );
            None
        }
    }
}
