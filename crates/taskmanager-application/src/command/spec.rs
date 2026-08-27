//! Single declarative source for per-command metadata — the command spec
//! table ("one fact, one authority" for the command vocabulary).
//!
//! Every derived surface reads this one table: [`CommandId::ALL`]'s
//! canonical order, [`CommandId::action`], the router's default bindings
//! and enable rules, and the i18n keys behind the shared label/description
//! copy the shell resolves through the application catalog. Presentation
//! data the application layer cannot own (the semantic icon vocabulary)
//! stays in `taskmanager-ui-contract`, keyed by the same [`CommandId`].
//! Adding a command is: one enum variant in `command.rs`, one row here,
//! one icon arm in `ui-contract`, and the locale keys in `locales/*.json`.

use crate::router::{CommandBinding, CommandContext, CommandScope};
use crate::{
    AppAction, AppPage, CommandId, FocusDirection, KeyChord, KeyCode, Modifiers, RefreshRequest,
    SelectionDirection,
};

/// When the router may fire a command, expressed against the runtime
/// [`CommandContext`]. The enable rule is per-command data carried by the
/// spec table, not a hand-maintained match arm in the router.
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

/// One row of per-command metadata: identity, typed action, default chord
/// and scope, enable rule, and the i18n keys for the shared label and
/// description copy.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CommandSpec {
    pub id: CommandId,
    pub action: AppAction,
    pub key: KeyCode,
    pub modifiers: Modifiers,
    pub scope: CommandScope,
    pub enable: CommandEnableRule,
    pub label_key: &'static str,
    pub description_key: &'static str,
}

impl CommandSpec {
    /// The router binding this row prescribes.
    #[must_use]
    pub(crate) const fn binding(self) -> CommandBinding {
        CommandBinding::new(self.id, KeyChord::new(self.key, self.modifiers), self.scope)
    }
}

/// The command vocabulary in canonical order. The slice length (never a
/// literal count) sizes every derived array: [`CommandId::ALL`] and the
/// router's `DEFAULT_BINDINGS`.
pub(crate) const COMMAND_SPECS: &[CommandSpec] = &[
    CommandSpec {
        id: CommandId::FocusSearch,
        action: AppAction::FocusSearch,
        key: KeyCode::F,
        modifiers: Modifiers::CONTROL,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.focus_search.label",
        description_key: "command.focus_search.description",
    },
    CommandSpec {
        id: CommandId::PageUp,
        action: AppAction::MoveSelection(SelectionDirection::PageUp),
        key: KeyCode::PageUp,
        modifiers: Modifiers::NONE,
        scope: CommandScope::ProcessList,
        enable: CommandEnableRule::NotTextInput,
        label_key: "command.page_up.label",
        description_key: "command.page_up.description",
    },
    CommandSpec {
        id: CommandId::PageDown,
        action: AppAction::MoveSelection(SelectionDirection::PageDown),
        key: KeyCode::PageDown,
        modifiers: Modifiers::NONE,
        scope: CommandScope::ProcessList,
        enable: CommandEnableRule::NotTextInput,
        label_key: "command.page_down.label",
        description_key: "command.page_down.description",
    },
    CommandSpec {
        id: CommandId::ArrowDown,
        action: AppAction::MoveSelection(SelectionDirection::Next),
        key: KeyCode::ArrowDown,
        modifiers: Modifiers::NONE,
        scope: CommandScope::ProcessList,
        enable: CommandEnableRule::NotTextInput,
        label_key: "command.arrow_down.label",
        description_key: "command.arrow_down.description",
    },
    CommandSpec {
        id: CommandId::ArrowUp,
        action: AppAction::MoveSelection(SelectionDirection::Previous),
        key: KeyCode::ArrowUp,
        modifiers: Modifiers::NONE,
        scope: CommandScope::ProcessList,
        enable: CommandEnableRule::NotTextInput,
        label_key: "command.arrow_up.label",
        description_key: "command.arrow_up.description",
    },
    CommandSpec {
        id: CommandId::FocusNext,
        action: AppAction::MoveFocus(FocusDirection::Next),
        key: KeyCode::Tab,
        modifiers: Modifiers::NONE,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.focus_next.label",
        description_key: "command.focus_next.description",
    },
    CommandSpec {
        id: CommandId::FocusPrevious,
        action: AppAction::MoveFocus(FocusDirection::Previous),
        key: KeyCode::Tab,
        modifiers: Modifiers::SHIFT,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.focus_previous.label",
        description_key: "command.focus_previous.description",
    },
    CommandSpec {
        id: CommandId::ShowPerformance,
        action: AppAction::SelectPage(AppPage::Performance),
        key: KeyCode::Digit1,
        modifiers: Modifiers::ALT,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.show_performance.label",
        description_key: "command.show_performance.description",
    },
    CommandSpec {
        id: CommandId::ShowApplications,
        action: AppAction::SelectPage(AppPage::Applications),
        key: KeyCode::Digit2,
        modifiers: Modifiers::ALT,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.show_applications.label",
        description_key: "command.show_applications.description",
    },
    CommandSpec {
        id: CommandId::ShowServices,
        action: AppAction::SelectPage(AppPage::Services),
        key: KeyCode::Digit3,
        modifiers: Modifiers::ALT,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.show_services.label",
        description_key: "command.show_services.description",
    },
    CommandSpec {
        id: CommandId::ShowSystem,
        action: AppAction::SelectPage(AppPage::System),
        key: KeyCode::Digit4,
        modifiers: Modifiers::ALT,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.show_system.label",
        description_key: "command.show_system.description",
    },
    CommandSpec {
        id: CommandId::ShowStartup,
        action: AppAction::SelectPage(AppPage::Startup),
        key: KeyCode::Digit5,
        modifiers: Modifiers::ALT,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.show_startup.label",
        description_key: "command.show_startup.description",
    },
    CommandSpec {
        id: CommandId::ShowUsers,
        action: AppAction::SelectPage(AppPage::Users),
        key: KeyCode::Digit6,
        modifiers: Modifiers::ALT,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.show_users.label",
        description_key: "command.show_users.description",
    },
    CommandSpec {
        id: CommandId::ShowAppHistory,
        action: AppAction::SelectPage(AppPage::AppHistory),
        key: KeyCode::Digit7,
        modifiers: Modifiers::ALT,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.show_app_history.label",
        description_key: "command.show_app_history.description",
    },
    CommandSpec {
        id: CommandId::ShowAlerts,
        action: AppAction::OpenAlerts,
        key: KeyCode::Digit8,
        modifiers: Modifiers::ALT,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.show_alerts.label",
        description_key: "command.show_alerts.description",
    },
    CommandSpec {
        id: CommandId::Refresh,
        action: AppAction::Refresh(RefreshRequest::Processes),
        key: KeyCode::F5,
        modifiers: Modifiers::NONE,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.refresh.label",
        description_key: "command.refresh.description",
    },
    CommandSpec {
        id: CommandId::EndTask,
        action: AppAction::RequestEndTask,
        key: KeyCode::Delete,
        modifiers: Modifiers::NONE,
        scope: CommandScope::ProcessList,
        enable: CommandEnableRule::RequiresSelection,
        label_key: "command.end_task.label",
        description_key: "command.end_task.description",
    },
    CommandSpec {
        id: CommandId::OpenProperties,
        action: AppAction::OpenProperties,
        key: KeyCode::Enter,
        modifiers: Modifiers::NONE,
        scope: CommandScope::ProcessList,
        enable: CommandEnableRule::RequiresSelection,
        label_key: "command.open_properties.label",
        description_key: "command.open_properties.description",
    },
    CommandSpec {
        id: CommandId::ShowSystemAbout,
        action: AppAction::OpenSystemAbout,
        key: KeyCode::A,
        modifiers: Modifiers::CONTROL,
        scope: CommandScope::Global,
        enable: CommandEnableRule::NoOverlay,
        label_key: "command.show_system_about.label",
        description_key: "command.show_system_about.description",
    },
    CommandSpec {
        id: CommandId::Dismiss,
        action: AppAction::DismissOverlay,
        key: KeyCode::Escape,
        modifiers: Modifiers::NONE,
        scope: CommandScope::Global,
        enable: CommandEnableRule::OverlayOnly,
        label_key: "command.dismiss.label",
        description_key: "command.dismiss.description",
    },
    CommandSpec {
        id: CommandId::Confirm,
        action: AppAction::ConfirmEndTask,
        key: KeyCode::Enter,
        modifiers: Modifiers::NONE,
        scope: CommandScope::Dialog,
        enable: CommandEnableRule::OverlayOnly,
        label_key: "command.confirm.label",
        description_key: "command.confirm.description",
    },
    CommandSpec {
        id: CommandId::TogglePause,
        action: AppAction::TogglePause,
        key: KeyCode::Space,
        modifiers: Modifiers::CONTROL,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.toggle_pause.label",
        description_key: "command.toggle_pause.description",
    },
    CommandSpec {
        id: CommandId::ToggleSidebar,
        action: AppAction::ToggleSidebar,
        key: KeyCode::F9,
        modifiers: Modifiers::NONE,
        scope: CommandScope::Global,
        enable: CommandEnableRule::Always,
        label_key: "command.toggle_sidebar.label",
        description_key: "command.toggle_sidebar.description",
    },
    CommandSpec {
        id: CommandId::MoveToFirst,
        action: AppAction::MoveSelection(SelectionDirection::First),
        key: KeyCode::Home,
        modifiers: Modifiers::NONE,
        scope: CommandScope::ProcessList,
        enable: CommandEnableRule::NotTextInput,
        label_key: "command.move_to_first.label",
        description_key: "command.move_to_first.description",
    },
    CommandSpec {
        id: CommandId::MoveToLast,
        action: AppAction::MoveSelection(SelectionDirection::Last),
        key: KeyCode::End,
        modifiers: Modifiers::NONE,
        scope: CommandScope::ProcessList,
        enable: CommandEnableRule::NotTextInput,
        label_key: "command.move_to_last.label",
        description_key: "command.move_to_last.description",
    },
    CommandSpec {
        id: CommandId::CopySelectedRow,
        action: AppAction::CopySelectedRow,
        key: KeyCode::C,
        modifiers: Modifiers::CONTROL,
        scope: CommandScope::Global,
        enable: CommandEnableRule::NoOverlay,
        label_key: "command.copy_selected_row.label",
        description_key: "command.copy_selected_row.description",
    },
];
