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

/// The two page families of the composition doctrine (ADR-042).
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

    /// Whether this DATA page additionally composes through the shared
    /// `ListPageScaffold` inner header+body shell (ADR-042). Declared next
    /// to the family mapping so the render guard reads one typed source
    /// instead of mirroring a second list in the test.
    pub const fn uses_list_scaffold(self) -> bool {
        match self {
            Self::Services | Self::Users | Self::Startup => true,
            Self::Performance | Self::Apps | Self::System | Self::AppHistory | Self::Containers => {
                false
            }
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

impl super::RootView {
    /// Request only the inventory owned by a newly visible detail page.
    pub(crate) fn request_page_data(&mut self, page: TopPage) {
        match page {
            TopPage::System => {
                self.request_refresh(taskmanager_application::RefreshRequest::HardwareInventory)
            }
            TopPage::Containers => {
                self.request_refresh(taskmanager_application::RefreshRequest::Containers)
            }
            // The CPU page's pinned details (base clock, sockets, the P/E/LP
            // core-class breakdown) consume the hardware inventory. Under the
            // Dashboard automatic profile hardware is not background work, so
            // the page requests its own copy when it becomes visible — one
            // static fetch, never a schedule.
            TopPage::Performance => {
                self.request_refresh(taskmanager_application::RefreshRequest::HardwareInventory)
            }
            TopPage::Apps
            | TopPage::Services
            | TopPage::Startup
            | TopPage::Users
            | TopPage::AppHistory => {}
        }
    }

    /// Select a page and eagerly request only the inventory owned by that
    /// page. High-rate dashboard facts remain on the shared automatic
    /// schedule; static/detail inventories become live only while a user
    /// visits the corresponding surface.
    pub(crate) fn select_page(&mut self, page: TopPage) {
        if self.page == page {
            return;
        }
        self.dismiss_current_surface(super::WindowSurfaceDismissReason::PageChanged);
        self.page = page;
        self.request_page_data(page);
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
