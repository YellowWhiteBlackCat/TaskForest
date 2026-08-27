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

impl TopPage {
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
    ///
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
