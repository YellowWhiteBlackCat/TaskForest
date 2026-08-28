//! Platform-neutral identities for root navigation and device selection.

use taskmanager_application::AppPage;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TopPage {
    Performance,
    Apps,
    Services,
    System,
    Startup,
    Users,
    /// Per-application usage history (Mission Center 应用检测). Mirrors the
    /// shared `AppPage::AppHistory` route; the body is rendered by
    /// [`crate::gpui_app::app_history_view`].
    AppHistory,
    Containers,
}

/// The two page families of the composition doctrine (ADR-041).
///
/// Every top-level page declares its family, and each family owns exactly
/// ONE composition root: chart pages (the Performance surface) compose
/// through `perf_page` with the chart-tier grammar (ADR-039); data pages
/// compose through the shared `PageScaffold` shell — the list-style ones
/// share the inner `ListPageScaffold` header+body split. The family split
/// is what makes a layout adjustment propagate everywhere at once: one
/// root per family, proven by the page-family render guard.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageFamily {
    /// Chart-first telemetry surface; one `perf_page` composition root.
    Chart,
    /// Text/table-first inventory surface; one `PageScaffold` shell root.
    Data,
}

impl TopPage {
    /// The composition family this page belongs to. New pages must declare
    /// into one of the two roots — a third family is an ADR-level decision.
    pub const fn family(self) -> PageFamily {
        match self {
            Self::Performance => PageFamily::Chart,
            Self::Apps
            | Self::Services
            | Self::System
            | Self::Startup
            | Self::Users
            | Self::AppHistory
            | Self::Containers => PageFamily::Data,
        }
    }

    /// Every page, for exhaustive iteration (tests, nav construction).
    pub const ALL: [TopPage; 8] = [
        TopPage::Performance,
        TopPage::Apps,
        TopPage::Services,
        TopPage::System,
        TopPage::Startup,
        TopPage::Users,
        TopPage::AppHistory,
        TopPage::Containers,
    ];

    /// Adapt one application-owned shared route into the GPUI page identity.
    /// The GPUI-only `Containers` page deliberately has no `AppPage` source;
    /// keeping that fact explicit prevents a renderer extension from leaking
    /// into the shared shell contract.
    pub const fn from_app_page(page: AppPage) -> Self {
        match page {
            AppPage::Performance => Self::Performance,
            AppPage::Applications => Self::Apps,
            AppPage::Services => Self::Services,
            AppPage::System => Self::System,
            AppPage::Startup => Self::Startup,
            AppPage::Users => Self::Users,
            AppPage::AppHistory => Self::AppHistory,
        }
    }

    /// Return the shared route represented by this GPUI page, if any.
    #[must_use]
    pub const fn app_page(self) -> Option<AppPage> {
        match self {
            Self::Performance => Some(AppPage::Performance),
            Self::Apps => Some(AppPage::Applications),
            Self::Services => Some(AppPage::Services),
            Self::System => Some(AppPage::System),
            Self::Startup => Some(AppPage::Startup),
            Self::Users => Some(AppPage::Users),
            Self::AppHistory => Some(AppPage::AppHistory),
            Self::Containers => None,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_navigation_tests.rs"]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StableDeviceKind {
    Disk,
    Network,
    Gpu,
    Battery,
    Fan,
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_app/page_family_contract_tests.rs"]
mod page_family_contract_tests;
