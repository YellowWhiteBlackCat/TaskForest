//! Iced-local Services status filter and its visible-row projection rule.
//!
//! The four buckets mirror GPUI's Services status-filter pills. The shared
//! shell remains the owner of service facts and sort order; this renderer-local
//! selector only narrows the already projected rows by typed status.

use taskforest_core::core::services::ServiceStatus;
use taskmanager_application::i18n::t;

use crate::ui::tables::ServiceRow;

use super::{IcedApp, Message};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ServiceStatusFilter {
    #[default]
    All,
    Active,
    Inactive,
    Failed,
}

impl ServiceStatusFilter {
    pub const ALL: [Self; 4] = [Self::All, Self::Active, Self::Inactive, Self::Failed];

    const LABEL_KEYS: [&'static str; Self::ALL.len()] =
        ["common.all", "svc.active", "svc.inactive", "svc.failed"];
    const KEYS: [&'static str; Self::ALL.len()] = ["all", "active", "inactive", "failed"];
    const REQUIRED_STATUSES: [Option<ServiceStatus>; Self::ALL.len()] = [
        None,
        Some(ServiceStatus::Active),
        Some(ServiceStatus::Inactive),
        Some(ServiceStatus::Failed),
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        t(Self::LABEL_KEYS
            .get(self.index())
            .copied()
            .unwrap_or("common.all"))
    }

    #[must_use]
    pub fn key(self) -> &'static str {
        Self::KEYS[self.index()]
    }

    #[must_use]
    pub fn index(self) -> usize {
        let mut index = 0;
        while index < Self::ALL.len() {
            if Self::ALL[index] == self {
                return index;
            }
            index += 1;
        }
        0
    }

    #[must_use]
    pub(crate) fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    #[must_use]
    fn matches(self, status: ServiceStatus) -> bool {
        Self::REQUIRED_STATUSES
            .get(self.index())
            .copied()
            .flatten()
            .is_none_or(|required| status == required)
    }
}

impl ServiceStatusFilter {
    #[must_use]
    pub(crate) fn matches_row(self, service: &ServiceRow, query: &str) -> bool {
        self.matches(service.status) && crate::ui::tables::service_matches_lower(service, query)
    }
}

#[must_use]
pub(crate) fn service_filter_message(index: usize) -> Message {
    Message::SelectServiceStatusFilter(ServiceStatusFilter::from_index(index).unwrap_or_default())
}

impl IcedApp {
    pub(crate) fn set_services_status_filter(&mut self, filter: ServiceStatusFilter) {
        if self.services_status_filter == filter {
            return;
        }
        self.services_status_filter = filter;
        self.close_service_row_menu();
        let visible = self.services_projection(self.services_query()).1;
        self.shell
            .move_selection_to(self.shell.selected.min(visible.len().saturating_sub(1)));
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/services_status_tests.rs"]
mod tests;
