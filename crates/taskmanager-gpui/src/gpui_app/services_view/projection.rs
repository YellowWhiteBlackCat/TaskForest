//! Pure Services filter/sort projection and its render cache key.

use taskmanager_shell::{InfoSortCol, SortDir};
use taskmanager_ui_contract::IconId;

use crate::gpui_app::list_view::FilterSpec;
use taskmanager_application::i18n;
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};

/// Status filter for the Services list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ServiceFilter {
    All,
    Active,
    Inactive,
    Failed,
}

impl ServiceFilter {
    fn matches(self, status: ServiceStatus) -> bool {
        match self {
            Self::All => true,
            Self::Active => status == ServiceStatus::Active,
            Self::Inactive => status == ServiceStatus::Inactive,
            Self::Failed => status == ServiceStatus::Failed,
        }
    }

    pub(super) const ALL: [Self; 4] = [Self::All, Self::Active, Self::Inactive, Self::Failed];
}

impl FilterSpec for ServiceFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => i18n::t("common.all"),
            Self::Active => i18n::t("svc.active"),
            Self::Inactive => i18n::t("svc.inactive"),
            Self::Failed => i18n::t("svc.failed"),
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::All => "svc-filter-all",
            Self::Active => "svc-filter-active",
            Self::Inactive => "svc-filter-inactive",
            Self::Failed => "svc-filter-failed",
        }
    }

    fn icon(self) -> Option<IconId> {
        match self {
            Self::All => None,
            Self::Active => Some(IconId::CircleCheck),
            Self::Inactive => Some(IconId::CircleX),
            Self::Failed => Some(IconId::TriangleAlert),
        }
    }
}

pub fn sorted_services(
    items: &[ServiceItem],
    filter: ServiceFilter,
    query: &str,
    sort: Option<(InfoSortCol, SortDir)>,
) -> Vec<ServiceItem> {
    let mut filtered = filter_services(items, filter, query);
    taskmanager_shell::order_service_rows(&mut filtered, sort);
    filtered
}

/// Filter by typed status and case-insensitive name or description.
pub fn filter_services(
    items: &[ServiceItem],
    filter: ServiceFilter,
    query: &str,
) -> Vec<ServiceItem> {
    let query = query.trim();
    items
        .iter()
        .filter(|service| {
            filter.matches(service.status)
                && (query.is_empty()
                    || taskmanager_core::core::text::contains_ascii_ci(&service.name, query)
                    || taskmanager_core::core::text::contains_ascii_ci(&service.description, query))
        })
        .cloned()
        .collect()
}
