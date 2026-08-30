//! Test-side page surface: the full route list the page-signature
//! reservation gate walks. The rendered strip shows only
//! [`crate::app::NAV_TABS`]; this list keeps every route (including
//! Alerts/Settings) under the shared-order law.

use crate::app::Page;

impl Page {
    /// The full route surface, in the shared order.
    pub(crate) const ALL: &'static [Page] = &[
        Page::Processes,
        Page::Performance,
        Page::Services,
        Page::System,
        Page::Startup,
        Page::Sessions,
        Page::Alerts,
        Page::Settings,
        Page::AppHistory,
    ];
}
