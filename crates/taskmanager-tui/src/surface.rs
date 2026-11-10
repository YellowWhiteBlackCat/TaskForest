//! Typed TUI-local surface and input-ownership machine.
//!
//! Shared dangerous confirmations and Process Properties are owned by the
//! application `InteractionState`. This module owns the one terminal-local
//! modal/menu. Its private slot replaces the former collection of booleans
//! and independent `Option` fields, making the modal precedence chain
//! unrepresentable.

use crate::diagnostic_bundle::DiagnosticBundleTargetView;
use taskmanager_application::SurfaceKind;
use taskmanager_ui_contract::{OverlayEvent, OverlayKindOf, OverlaySlot, OverlayTransition};

use crate::{
    BatchMenuTarget, CommandPalette, ProcessMenuTarget, ServiceMenuTarget, SessionMenuTarget,
    StartupMenuTarget, TuiApp,
};
use taskforest_contract::RequestId;
use taskforest_core::core::failure::FailureKind;
use taskforest_core::core::process::FrozenProcessIdentity;
use taskforest_core::core::target::ServiceId;
use zhuge_ui_core::row_page::page_of;

/// Target for the interactive service-dependencies modal.
#[derive(Clone, Debug)]
pub struct ServiceDependenciesTarget {
    pub service_id: ServiceId,
    pub service_name: String,
    pub scroll: usize,
}

/// Number of columns in the CPU affinity core grid.
pub const AFFINITY_GRID_COLS: usize = 4;

/// Target and interactive state for the CPU affinity editor modal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffinityModalState {
    pub target: FrozenProcessIdentity,
    pub selected_cpu: usize,
    pub selected_mask: Vec<u32>,
    pub logical_cpu_count: usize,
    pub scroll: usize,
    /// Whether a correlated read has ever seeded `selected_mask`. Before the
    /// first authoritative read, the grid shows every CPU unchecked and the
    /// toggles and apply stay inert: an unobserved mask is never fabricated
    /// into an editable default (the Iced editor fails closed the same way).
    pub mask_observed: bool,
    /// Which authoritative read `selected_mask` was last seeded from. A
    /// *fresh* read for the same identity rewrites in-progress edits (the
    /// Iced editor's same-wave sync rule), while re-projecting the same read
    /// every cycle must not — so the absorbed request id is what makes the
    /// per-cycle projection idempotent.
    pub observed_read: Option<RequestId>,
    /// The shared reason a correlated read failed for `target`, if the latest
    /// outcome was a failure. The header paints it through the owner's
    /// resolver; it never becomes a local sentence or a `Debug` dump.
    pub failure: Option<FailureKind>,
}

impl AffinityModalState {
    #[must_use]
    pub fn new(
        target: FrozenProcessIdentity,
        current: Option<(RequestId, Vec<u32>)>,
        logical_cpu_count: usize,
    ) -> Self {
        let logical_cpu_count = logical_cpu_count.max(1);
        let (observed_read, selected_mask) = match current {
            Some((request_id, cpus)) => (Some(request_id), cpus),
            None => (None, Vec::new()),
        };
        let mask_observed = observed_read.is_some();
        Self {
            target,
            selected_cpu: 0,
            selected_mask,
            logical_cpu_count,
            scroll: 0,
            mask_observed,
            observed_read,
            failure: None,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_cpu >= AFFINITY_GRID_COLS {
            self.selected_cpu -= AFFINITY_GRID_COLS;
        }
        self.adjust_scroll();
    }

    pub fn move_down(&mut self) {
        if self.selected_cpu + AFFINITY_GRID_COLS < self.logical_cpu_count {
            self.selected_cpu += AFFINITY_GRID_COLS;
        } else if self.logical_cpu_count > 0 {
            self.selected_cpu = self.logical_cpu_count - 1;
        }
        self.adjust_scroll();
    }

    pub fn move_left(&mut self) {
        self.selected_cpu = self.selected_cpu.saturating_sub(1);
        self.adjust_scroll();
    }

    pub fn move_right(&mut self) {
        if self.selected_cpu + 1 < self.logical_cpu_count {
            self.selected_cpu += 1;
        }
        self.adjust_scroll();
    }

    pub fn toggle_selected(&mut self) {
        if !self.is_editable() {
            return;
        }
        if let Ok(cpu) = u32::try_from(self.selected_cpu) {
            if let Some(pos) = self.selected_mask.iter().position(|&c| c == cpu) {
                self.selected_mask.remove(pos);
            } else {
                self.selected_mask.push(cpu);
                self.selected_mask.sort_unstable();
            }
        }
    }

    pub fn toggle_all(&mut self) {
        if !self.is_editable() {
            return;
        }
        if self.selected_mask.len() >= self.logical_cpu_count {
            self.selected_mask.clear();
        } else {
            self.selected_mask = (0..self.logical_cpu_count as u32).collect();
        }
    }

    /// Copy an authoritative read into the editor. Only a snapshot whose
    /// frozen target matches the modal's captured identity may rewrite the
    /// visible mask, so a recycled PID can never retarget the open editor.
    pub fn observe_mask(&mut self, request_id: RequestId, mask: &[u32]) {
        let bounded = mask
            .iter()
            .copied()
            .filter(|cpu| usize::try_from(*cpu).is_ok_and(|index| index < self.logical_cpu_count))
            .collect::<Vec<_>>();
        self.selected_mask = bounded;
        self.observed_read = Some(request_id);
        self.mask_observed = true;
    }

    /// Record the latest read outcome for the modal's captured identity: the
    /// shared failure reason when the read failed, cleared when it did not.
    /// The grid's fail-closed rule is unchanged — a failed read never seeds a
    /// mask; only `observe_mask` on an authoritative snapshot does.
    pub fn observe_failure(&mut self, failure: Option<FailureKind>) {
        self.failure = failure;
    }

    /// Drop the displayed mask back to "never observed". A failed read means
    /// the current mask is unknown, so the editor shows no mask and stays
    /// read-only until an authoritative read lands (Iced and GPUI agree).
    pub fn unobserve_mask(&mut self) {
        self.selected_mask.clear();
        self.selected_cpu = 0;
        self.scroll = 0;
        self.mask_observed = false;
        self.observed_read = None;
    }

    /// Whether the displayed mask may be edited or submitted: only an
    /// observed mask whose latest read did not fail. While a read failed the
    /// editor is read-only and says why, exactly like Iced and GPUI — a stale
    /// last-good mask is shown for context, never submitted as current.
    #[must_use]
    pub fn is_editable(&self) -> bool {
        self.mask_observed && self.failure.is_none()
    }

    fn adjust_scroll(&mut self) {
        let current_row = page_of(self.selected_cpu, AFFINITY_GRID_COLS);
        if current_row < self.scroll {
            self.scroll = current_row;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TuiSurfaceKind {
    Settings,
    SidebarEditor,
    SavedViews,
    About,
    SystemInformation,
    Health,
    Containers,
    ServiceMenu,
    ProcessMenu,
    BatchMenu,
    SessionMenu,
    StartupMenu,
    ColumnMenu,
    CommandPalette,
    ServiceDependencies,
    ProcessAffinity,
    DiagnosticBundle,
    FirstRun,
}

#[derive(Clone, Debug)]
pub(crate) enum TuiSurface {
    Settings,
    SidebarEditor {
        selected: Option<String>,
    },
    SavedViews {
        selected: Option<u64>,
        input: crate::saved_views::SavedViewInput,
    },
    About(crate::information::AboutTargetView),
    SystemInformation(crate::information::SystemInformationTargetView),
    Health,
    Containers,
    ServiceMenu(Box<ServiceMenuTarget>),
    ProcessMenu(Box<ProcessMenuTarget>),
    BatchMenu(BatchMenuTarget),
    SessionMenu(SessionMenuTarget),
    StartupMenu(StartupMenuTarget),
    ColumnMenu {
        selection: usize,
    },
    CommandPalette(CommandPalette),
    ServiceDependencies(ServiceDependenciesTarget),
    ProcessAffinity(AffinityModalState),
    DiagnosticBundle(DiagnosticBundleTargetView),
    FirstRun(crate::first_run::FirstRunTargetView),
}

impl TuiSurface {
    pub(crate) fn kind(&self) -> TuiSurfaceKind {
        match self {
            Self::Settings => TuiSurfaceKind::Settings,
            Self::SidebarEditor { .. } => TuiSurfaceKind::SidebarEditor,
            Self::SavedViews { .. } => TuiSurfaceKind::SavedViews,
            Self::About(_) => TuiSurfaceKind::About,
            Self::SystemInformation(_) => TuiSurfaceKind::SystemInformation,
            Self::Health => TuiSurfaceKind::Health,
            Self::Containers => TuiSurfaceKind::Containers,
            Self::ServiceMenu(_) => TuiSurfaceKind::ServiceMenu,
            Self::ProcessMenu(_) => TuiSurfaceKind::ProcessMenu,
            Self::BatchMenu(_) => TuiSurfaceKind::BatchMenu,
            Self::SessionMenu(_) => TuiSurfaceKind::SessionMenu,
            Self::StartupMenu(_) => TuiSurfaceKind::StartupMenu,
            Self::ColumnMenu { .. } => TuiSurfaceKind::ColumnMenu,
            Self::CommandPalette(_) => TuiSurfaceKind::CommandPalette,
            Self::ServiceDependencies(_) => TuiSurfaceKind::ServiceDependencies,
            Self::ProcessAffinity(_) => TuiSurfaceKind::ProcessAffinity,
            Self::DiagnosticBundle(_) => TuiSurfaceKind::DiagnosticBundle,
            Self::FirstRun(_) => TuiSurfaceKind::FirstRun,
        }
    }
}

impl OverlayKindOf for TuiSurface {
    type Kind = TuiSurfaceKind;

    fn kind(&self) -> Self::Kind {
        TuiSurface::kind(self)
    }
}

pub(crate) type TuiSurfaceEvent = OverlayEvent<TuiSurface, TuiSurfaceKind, ()>;
pub(crate) type TuiSurfaceTransition = OverlayTransition<TuiSurfaceKind, ()>;

#[derive(Debug, Default)]
pub(crate) struct TuiSurfaceState(OverlaySlot<TuiSurface>);

impl std::ops::Deref for TuiSurfaceState {
    type Target = OverlaySlot<TuiSurface>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for TuiSurfaceState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl TuiSurfaceState {
    pub(crate) fn reduce(&mut self, event: TuiSurfaceEvent) -> TuiSurfaceTransition {
        self.0.apply(event)
    }

    pub(crate) fn take(&mut self, expected: TuiSurfaceKind) -> Option<TuiSurface> {
        self.dismiss_expected(expected)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiInputScope {
    SharedSurface(SurfaceKind),
    LocalSurface(TuiSurfaceKind),
    ServiceLog,
    Help,
    Suggestions,
    Search,
    DetailsPanel,
    Content,
}

impl TuiInputScope {
    pub(crate) fn blocks_pointer(self) -> bool {
        !matches!(self, Self::Content)
    }
}

impl TuiApp {
    pub(crate) fn local_surface(&self) -> Option<&TuiSurface> {
        OverlaySlot::active(&self.local_surface)
    }
    pub(crate) fn local_surface_mut(&mut self) -> Option<&mut TuiSurface> {
        OverlaySlot::active_mut(&mut self.local_surface)
    }

    pub(crate) fn local_surface_kind(&self) -> Option<TuiSurfaceKind> {
        self.local_surface.kind()
    }

    #[must_use]
    pub fn settings_open(&self) -> bool {
        matches!(self.local_surface(), Some(TuiSurface::Settings))
    }

    #[must_use]
    pub fn about_open(&self) -> bool {
        matches!(self.local_surface(), Some(TuiSurface::About(_)))
    }

    #[must_use]
    pub fn health_open(&self) -> bool {
        matches!(self.local_surface(), Some(TuiSurface::Health))
    }

    #[must_use]
    pub fn containers_open(&self) -> bool {
        matches!(self.local_surface(), Some(TuiSurface::Containers))
    }

    pub(crate) fn service_menu_mut(&mut self) -> Option<&mut ServiceMenuTarget> {
        match self.local_surface_mut() {
            Some(TuiSurface::ServiceMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) fn process_menu_mut(&mut self) -> Option<&mut ProcessMenuTarget> {
        match self.local_surface_mut() {
            Some(TuiSurface::ProcessMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) fn batch_menu_mut(&mut self) -> Option<&mut BatchMenuTarget> {
        match self.local_surface_mut() {
            Some(TuiSurface::BatchMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) fn session_menu_mut(&mut self) -> Option<&mut SessionMenuTarget> {
        match self.local_surface_mut() {
            Some(TuiSurface::SessionMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) fn startup_menu_mut(&mut self) -> Option<&mut StartupMenuTarget> {
        match self.local_surface_mut() {
            Some(TuiSurface::StartupMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(crate) fn service_dependencies_mut(&mut self) -> Option<&mut ServiceDependenciesTarget> {
        match self.local_surface_mut() {
            Some(TuiSurface::ServiceDependencies(target)) => Some(target),
            _ => None,
        }
    }

    #[must_use]
    pub fn process_affinity(&self) -> Option<&AffinityModalState> {
        match self.local_surface() {
            Some(TuiSurface::ProcessAffinity(state)) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn process_affinity_mut(&mut self) -> Option<&mut AffinityModalState> {
        match self.local_surface_mut() {
            Some(TuiSurface::ProcessAffinity(state)) => Some(state),
            _ => None,
        }
    }

    #[must_use]
    pub fn affinity_open(&self) -> bool {
        matches!(self.local_surface(), Some(TuiSurface::ProcessAffinity(_)))
    }

    pub(crate) fn column_menu_selection(&self) -> Option<usize> {
        match self.local_surface() {
            Some(TuiSurface::ColumnMenu { selection }) => Some(*selection),
            _ => None,
        }
    }

    pub(crate) fn column_menu_selection_mut(&mut self) -> Option<&mut usize> {
        match self.local_surface_mut() {
            Some(TuiSurface::ColumnMenu { selection }) => Some(selection),
            _ => None,
        }
    }

    pub(crate) fn command_palette(&self) -> Option<&CommandPalette> {
        match self.local_surface() {
            Some(TuiSurface::CommandPalette(palette)) => Some(palette),
            _ => None,
        }
    }

    pub(crate) fn command_palette_mut(&mut self) -> Option<&mut CommandPalette> {
        match self.local_surface_mut() {
            Some(TuiSurface::CommandPalette(palette)) => Some(palette),
            _ => None,
        }
    }

    pub(crate) fn process_properties(&self) -> Option<&crate::ProcessPropertiesTarget> {
        self.shell
            .process_properties_target()
            .and(self.process_properties_view.as_ref())
    }

    pub(crate) fn process_properties_mut(&mut self) -> Option<&mut crate::ProcessPropertiesTarget> {
        self.shell
            .process_properties_target()
            .and(self.process_properties_view.as_mut())
    }

    pub(crate) fn open_local_surface(&mut self, surface: TuiSurface) {
        if self.local_surface_kind() == Some(TuiSurfaceKind::DiagnosticBundle) {
            self.diagnostics.close();
        }
        self.shell.dismiss_overlay();
        self.shell.close_service_log();
        self.shell.dismiss_informational_overlay();
        self.shell.close_search();
        self.focus_panel = crate::FocusPanel::Table;
        let _ = self.local_surface.reduce(TuiSurfaceEvent::Open(surface));
    }

    pub(crate) fn dismiss_local_surface(&mut self) {
        if self.local_surface_kind() == Some(TuiSurfaceKind::DiagnosticBundle) {
            self.diagnostics.close();
        }
        let _ = self
            .local_surface
            .reduce(TuiSurfaceEvent::DismissCurrent(()));
    }

    pub(crate) fn dismiss_local_surface_kind(&mut self, expected: TuiSurfaceKind) {
        if expected == TuiSurfaceKind::DiagnosticBundle
            && self.local_surface_kind() == Some(expected)
        {
            self.diagnostics.close();
        }
        let _ = self
            .local_surface
            .reduce(TuiSurfaceEvent::Dismiss(expected, ()));
    }

    pub(crate) fn take_local_surface(&mut self, expected: TuiSurfaceKind) -> Option<TuiSurface> {
        self.local_surface.take(expected)
    }

    pub(crate) fn input_scope(&self) -> TuiInputScope {
        if let Some(surface) = self.shell.interaction_surface() {
            TuiInputScope::SharedSurface(surface)
        } else if let Some(surface) = self.local_surface_kind() {
            TuiInputScope::LocalSurface(surface)
        } else if self.shell.service_log.is_some() {
            TuiInputScope::ServiceLog
        } else if self.shell.help_open() {
            TuiInputScope::Help
        } else if self.shell.suggestions_open() {
            TuiInputScope::Suggestions
        } else if self.shell.search_active() {
            TuiInputScope::Search
        } else if self.focus_panel == crate::FocusPanel::Details {
            TuiInputScope::DetailsPanel
        } else {
            TuiInputScope::Content
        }
    }

    pub(crate) fn assert_surface_invariants(&self) {
        let owner_count = usize::from(self.shell.interaction_surface().is_some())
            + usize::from(self.local_surface_kind().is_some())
            + usize::from(self.shell.service_log.is_some())
            + usize::from(self.shell.help_open())
            + usize::from(self.shell.suggestions_open())
            + usize::from(self.shell.search_active())
            + usize::from(self.focus_panel == crate::FocusPanel::Details);
        debug_assert!(owner_count <= 1, "TUI input surfaces must be exclusive");
    }
}

#[cfg(test)]
#[path = "../tests/gui/surface_state_tests.rs"]
mod tests;
