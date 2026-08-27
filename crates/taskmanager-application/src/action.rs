//! Typed application actions produced by commands or explicit UI confirmation.

/// Top-level application destination.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AppPage {
    #[default]
    Performance,
    Applications,
    Services,
    System,
    Startup,
    Users,
    /// Durable per-application usage history, projected from persistent
    /// frontend-session replay and rendered by every frontend.
    AppHistory,
}

impl AppPage {
    /// Canonical order for the shared application shell.
    ///
    /// Renderer-specific extensions such as GPUI's Containers page stay
    /// outside this core application route set instead of silently widening
    /// the shell contract for every frontend.
    pub const ALL: [Self; 7] = [
        Self::Performance,
        Self::Applications,
        Self::Services,
        Self::System,
        Self::Startup,
        Self::Users,
        Self::AppHistory,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FocusDirection {
    Next,
    Previous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SelectionDirection {
    PageUp,
    PageDown,
    /// Single-row advance toward the end of the list.
    Next,
    /// Single-row advance toward the start of the list.
    Previous,
    /// Jump to the first visible row (Home).
    First,
    /// Jump to the last visible row (End).
    Last,
}

/// Every state transition accepted by the pure application reducer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AppAction {
    FocusSearch,
    MoveFocus(FocusDirection),
    MoveSelection(SelectionDirection),
    SelectPage(AppPage),
    Refresh(crate::RefreshRequest),
    RequestEndTask,
    ConfirmEndTask,
    OpenProperties,
    OpenSystemAbout,
    DismissOverlay,
    TogglePause,
    /// Copy the current selected row's summary to the clipboard (Ctrl+C).
    CopySelectedRow,
    /// Toggle the frontend-owned Performance device navigator.
    ///
    /// This is deliberately a UI action: visibility is per-window presentation
    /// state and must not enter telemetry, provider, or persistence contracts.
    ToggleSidebar,
    /// Open the shared confirmation overlay for a gated service-control action
    /// (Stop / Restart / Disable). Merely sets pending overlay state; the
    /// platform request is only emitted by [`Self::ConfirmServiceControl`].
    /// Mirrors [`Self::RequestEndTask`]'s request→confirm ordering.
    RequestServiceControl,
    ConfirmServiceControl,
    /// Open the alerts-management surface. The route itself is frontend-owned
    /// (each shape renders its own page/overlay), so — exactly like
    /// [`Self::CopySelectedRow`] — the reducer only acknowledges the action
    /// and the owning frontend presents its surface.
    OpenAlerts,
}
