//! The single application-owned command registry.
//!
//! The registry macro declares each identity and its neutral behavior metadata
//! together. It generates the public command enum, the canonical ID array, and
//! the router/help/palette metadata rows from the same entries. Frontend
//! executors, icons, and shape-local actions remain outside this crate.

use super::{KeyChord, KeyCode, Modifiers};
use crate::router::{CommandBinding, CommandContext, CommandScope};
use crate::{AppAction, AppPage, FocusDirection, RefreshRequest, SelectionDirection};

/// Whether a shared command is suitable for a generic command-palette entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandPaletteDisposition {
    /// A palette may offer the command; a frontend still owns its executor.
    Available,
    /// Keep the command on its contextual keyboard or frontend-local route.
    Excluded(CommandPaletteExclusionReason),
}

impl CommandPaletteDisposition {
    /// Whether the shared command is available to a generic palette.
    #[must_use]
    pub const fn is_available(self) -> bool {
        matches!(self, Self::Available)
    }

    /// The typed reason when this command is excluded from a generic palette.
    #[must_use]
    pub const fn exclusion_reason(self) -> Option<CommandPaletteExclusionReason> {
        match self {
            Self::Available => None,
            Self::Excluded(reason) => Some(reason),
        }
    }
}

/// Why a shared command stays out of a generic command palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandPaletteExclusionReason {
    /// The command moves a cursor or selection within the active list.
    ContextualNavigation,
    /// The command moves focus between controls.
    FocusTraversal,
    /// The command is destructive and must use its confirmation route.
    ConfirmationRequired,
    /// The command only has meaning while an overlay is active.
    OverlayLifecycle,
    /// The route depends on a frontend-owned surface.
    FrontendOwnedSurface,
    /// The command operates on a currently selected row and clipboard.
    SelectionContext,
}

/// When the router may fire a command, expressed against the runtime
/// CommandContext. The enable rule belongs to the same row as the command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandEnableRule {
    /// Always enabled inside its scope.
    Always,
    /// Not while a text input owns the keyboard (list navigation keys).
    NotTextInput,
    /// Only with no text input, no overlay, and a selected process row.
    RequiresSelection,
    /// Not while a text input owns the keyboard or an overlay is open.
    NoOverlay,
    /// Only while a confirmation overlay is open.
    OverlayOnly,
}

impl CommandEnableRule {
    #[must_use]
    pub(crate) const fn allows(self, context: CommandContext) -> bool {
        match self {
            Self::Always => true,
            Self::NotTextInput => !context.text_input_focused,
            Self::RequiresSelection => {
                !context.text_input_focused && !context.overlay_present && context.process_selected
            }
            Self::NoOverlay => !context.text_input_focused && !context.overlay_present,
            Self::OverlayOnly => context.overlay_present,
        }
    }
}

/// One command's neutral routing and presentation metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandSpec {
    pub id: CommandId,
    pub action: AppAction,
    pub key: KeyCode,
    pub modifiers: Modifiers,
    pub scope: CommandScope,
    pub enable: CommandEnableRule,
    pub label_key: &'static str,
    pub description_key: &'static str,
    pub palette: CommandPaletteDisposition,
}

impl CommandSpec {
    /// The router binding prescribed by this row.
    #[must_use]
    pub(crate) const fn binding(self) -> CommandBinding {
        CommandBinding::new(self.id, KeyChord::new(self.key, self.modifiers), self.scope)
    }
}

macro_rules! define_command_registry {
    (
        $(
            $(#[$variant_meta:meta])*
            $variant:ident: (
                $action:expr,
                $key:expr,
                $modifiers:expr,
                $scope:expr,
                $enable:expr,
                $label_key:literal,
                $description_key:literal,
                $palette:expr
            ),
        )+
    ) => {
        /// Stable command identities shared by every frontend.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(usize)]
        pub enum CommandId {
            $(
                $(#[$variant_meta])*
                $variant,
            )+
        }

        /// Command metadata in the same order as CommandId::ALL.
        pub(crate) const COMMAND_SPECS: &[CommandSpec] = &[
            $(
                CommandSpec {
                    id: CommandId::$variant,
                    action: $action,
                    key: $key,
                    modifiers: $modifiers,
                    scope: $scope,
                    enable: $enable,
                    label_key: $label_key,
                    description_key: $description_key,
                    palette: $palette,
                },
            )+
        ];

        impl CommandId {
            /// Every registered command in canonical order.
            pub const ALL: [Self; COMMAND_SPECS.len()] = [
                $(Self::$variant,)+
            ];

            /// The shared application action associated with this command.
            #[must_use]
            pub fn action(self) -> AppAction {
                self.spec().action
            }

            /// i18n key of the shared command label.
            #[must_use]
            pub fn label_key(self) -> &'static str {
                self.spec().label_key
            }

            /// i18n key of the shared command description.
            #[must_use]
            pub fn description_key(self) -> &'static str {
                self.spec().description_key
            }

            /// Shared generic-palette disposition for this command.
            #[must_use]
            pub fn palette_disposition(self) -> CommandPaletteDisposition {
                self.spec().palette
            }

            /// The spec row is indexed by the registry-generated enum
            /// discriminant. The enum and this table come from this one macro
            /// invocation, so every valid ID has exactly one row.
            #[must_use]
            pub(crate) fn spec(self) -> &'static CommandSpec {
                &COMMAND_SPECS[self as usize]
            }
        }
    };
}

define_command_registry! {
    FocusSearch: (
        AppAction::FocusSearch,
        KeyCode::F,
        Modifiers::CONTROL,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.focus_search.label",
        "command.focus_search.description",
        CommandPaletteDisposition::Available
    ),
    PageUp: (
        AppAction::MoveSelection(SelectionDirection::PageUp),
        KeyCode::PageUp,
        Modifiers::NONE,
        CommandScope::ProcessList,
        CommandEnableRule::NotTextInput,
        "command.page_up.label",
        "command.page_up.description",
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::ContextualNavigation)
    ),
    PageDown: (
        AppAction::MoveSelection(SelectionDirection::PageDown),
        KeyCode::PageDown,
        Modifiers::NONE,
        CommandScope::ProcessList,
        CommandEnableRule::NotTextInput,
        "command.page_down.label",
        "command.page_down.description",
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::ContextualNavigation)
    ),
    /// ArrowDown: advance the process selection by a single row.
    ArrowDown: (
        AppAction::MoveSelection(SelectionDirection::Next),
        KeyCode::ArrowDown,
        Modifiers::NONE,
        CommandScope::ProcessList,
        CommandEnableRule::NotTextInput,
        "command.arrow_down.label",
        "command.arrow_down.description",
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::ContextualNavigation)
    ),
    /// ArrowUp: retreat the process selection by a single row.
    ArrowUp: (
        AppAction::MoveSelection(SelectionDirection::Previous),
        KeyCode::ArrowUp,
        Modifiers::NONE,
        CommandScope::ProcessList,
        CommandEnableRule::NotTextInput,
        "command.arrow_up.label",
        "command.arrow_up.description",
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::ContextualNavigation)
    ),
    FocusNext: (
        AppAction::MoveFocus(FocusDirection::Next),
        KeyCode::Tab,
        Modifiers::NONE,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.focus_next.label",
        "command.focus_next.description",
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::FocusTraversal)
    ),
    FocusPrevious: (
        AppAction::MoveFocus(FocusDirection::Previous),
        KeyCode::Tab,
        Modifiers::SHIFT,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.focus_previous.label",
        "command.focus_previous.description",
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::FocusTraversal)
    ),
    ShowPerformance: (
        AppAction::SelectPage(AppPage::Performance),
        KeyCode::Digit1,
        Modifiers::ALT,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.show_performance.label",
        "command.show_performance.description",
        CommandPaletteDisposition::Available
    ),
    ShowApplications: (
        AppAction::SelectPage(AppPage::Applications),
        KeyCode::Digit2,
        Modifiers::ALT,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.show_applications.label",
        "command.show_applications.description",
        CommandPaletteDisposition::Available
    ),
    ShowServices: (
        AppAction::SelectPage(AppPage::Services),
        KeyCode::Digit3,
        Modifiers::ALT,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.show_services.label",
        "command.show_services.description",
        CommandPaletteDisposition::Available
    ),
    ShowSystem: (
        AppAction::SelectPage(AppPage::System),
        KeyCode::Digit4,
        Modifiers::ALT,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.show_system.label",
        "command.show_system.description",
        CommandPaletteDisposition::Available
    ),
    ShowStartup: (
        AppAction::SelectPage(AppPage::Startup),
        KeyCode::Digit5,
        Modifiers::ALT,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.show_startup.label",
        "command.show_startup.description",
        CommandPaletteDisposition::Available
    ),
    ShowUsers: (
        AppAction::SelectPage(AppPage::Users),
        KeyCode::Digit6,
        Modifiers::ALT,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.show_users.label",
        "command.show_users.description",
        CommandPaletteDisposition::Available
    ),
    ShowAppHistory: (
        AppAction::SelectPage(AppPage::AppHistory),
        KeyCode::Digit7,
        Modifiers::ALT,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.show_app_history.label",
        "command.show_app_history.description",
        CommandPaletteDisposition::Available
    ),
    /// Alt+8 opens a frontend-owned alerts-management surface.
    ShowAlerts: (
        AppAction::OpenAlerts,
        KeyCode::Digit8,
        Modifiers::ALT,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.show_alerts.label",
        "command.show_alerts.description",
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::FrontendOwnedSurface)
    ),
    Refresh: (
        AppAction::Refresh(RefreshRequest::Processes),
        KeyCode::F5,
        Modifiers::NONE,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.refresh.label",
        "command.refresh.description",
        CommandPaletteDisposition::Available
    ),
    EndTask: (
        AppAction::RequestEndTask,
        KeyCode::Delete,
        Modifiers::NONE,
        CommandScope::ProcessList,
        CommandEnableRule::RequiresSelection,
        "command.end_task.label",
        "command.end_task.description",
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::ConfirmationRequired)
    ),
    OpenProperties: (
        AppAction::OpenProperties,
        KeyCode::Enter,
        Modifiers::NONE,
        CommandScope::ProcessList,
        CommandEnableRule::RequiresSelection,
        "command.open_properties.label",
        "command.open_properties.description",
        CommandPaletteDisposition::Available
    ),
    ShowSystemAbout: (
        AppAction::OpenSystemAbout,
        KeyCode::A,
        Modifiers::CONTROL,
        CommandScope::Global,
        CommandEnableRule::NoOverlay,
        "command.show_system_about.label",
        "command.show_system_about.description",
        CommandPaletteDisposition::Available
    ),
    Dismiss: (
        AppAction::DismissOverlay,
        KeyCode::Escape,
        Modifiers::NONE,
        CommandScope::Global,
        CommandEnableRule::OverlayOnly,
        "command.dismiss.label",
        "command.dismiss.description",
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::OverlayLifecycle)
    ),
    /// Enter confirms the active confirmation dialog.
    Confirm: (
        AppAction::ConfirmEndTask,
        KeyCode::Enter,
        Modifiers::NONE,
        CommandScope::Dialog,
        CommandEnableRule::OverlayOnly,
        "command.confirm.label",
        "command.confirm.description",
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::OverlayLifecycle)
    ),
    TogglePause: (
        AppAction::TogglePause,
        KeyCode::Space,
        Modifiers::CONTROL,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.toggle_pause.label",
        "command.toggle_pause.description",
        CommandPaletteDisposition::Available
    ),
    ToggleSidebar: (
        AppAction::ToggleSidebar,
        KeyCode::F9,
        Modifiers::NONE,
        CommandScope::Global,
        CommandEnableRule::Always,
        "command.toggle_sidebar.label",
        "command.toggle_sidebar.description",
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::FrontendOwnedSurface)
    ),
    /// Home jumps to the first visible process row.
    MoveToFirst: (
        AppAction::MoveSelection(SelectionDirection::First),
        KeyCode::Home,
        Modifiers::NONE,
        CommandScope::ProcessList,
        CommandEnableRule::NotTextInput,
        "command.move_to_first.label",
        "command.move_to_first.description",
        CommandPaletteDisposition::Available
    ),
    /// End jumps to the last visible process row.
    MoveToLast: (
        AppAction::MoveSelection(SelectionDirection::Last),
        KeyCode::End,
        Modifiers::NONE,
        CommandScope::ProcessList,
        CommandEnableRule::NotTextInput,
        "command.move_to_last.label",
        "command.move_to_last.description",
        CommandPaletteDisposition::Available
    ),
    /// Ctrl+C copies the selected row through a frontend-owned clipboard seam.
    CopySelectedRow: (
        AppAction::CopySelectedRow,
        KeyCode::C,
        Modifiers::CONTROL,
        CommandScope::Global,
        CommandEnableRule::NoOverlay,
        "command.copy_selected_row.label",
        "command.copy_selected_row.description",
        CommandPaletteDisposition::Excluded(CommandPaletteExclusionReason::SelectionContext)
    ),
}
