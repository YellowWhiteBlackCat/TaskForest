//! Semantic icons. Frontends decide which glyph or asset represents each value.

/// Stable semantic icon identity, independent of a particular UI toolkit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconId {
    Cpu,
    Memory,
    Disk,
    Network,
    Gpu,
    Process,
    Service,
    Startup,
    User,
    Health,
    Alert,
    Export,
    Settings,
    Search,
    /// Secondary actions or overflow-menu trigger.
    More,
    NavigateUp,
    NavigateDown,
    Focus,
    Performance,
    Applications,
    Services,
    System,
    Users,
    Refresh,
    EndTask,
    Properties,
    Close,
    Pause,
    Sidebar,
    /// Circled check (status-filter "Active" / "Enabled" pills).
    CircleCheck,
    /// Circled cross (status-filter "Inactive" / "Disabled" pills).
    CircleX,
    /// Triangle alert (status-filter "Failed" pills).
    TriangleAlert,
    /// Per-application usage history (App history page).
    History,
}

impl IconId {
    /// Every semantic icon in the contract, in stable declaration order.
    ///
    /// Frontend registries and tests iterate this source of truth instead of
    /// carrying drifting hand-written variant lists.
    pub const ALL: [Self; 33] = [
        Self::Cpu,
        Self::Memory,
        Self::Disk,
        Self::Network,
        Self::Gpu,
        Self::Process,
        Self::Service,
        Self::Startup,
        Self::User,
        Self::Health,
        Self::Alert,
        Self::Export,
        Self::Settings,
        Self::Search,
        Self::More,
        Self::NavigateUp,
        Self::NavigateDown,
        Self::Focus,
        Self::Performance,
        Self::Applications,
        Self::Services,
        Self::System,
        Self::Users,
        Self::Refresh,
        Self::EndTask,
        Self::Properties,
        Self::Close,
        Self::Pause,
        Self::Sidebar,
        Self::CircleCheck,
        Self::CircleX,
        Self::TriangleAlert,
        Self::History,
    ];
}
