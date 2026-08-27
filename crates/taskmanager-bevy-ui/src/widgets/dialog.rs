#![allow(dead_code)]
// ^ Library component awaiting its first in-product call site (the
// confirmation-gate wiring arms dialogs from `ShellApp::pending_confirmation`
// when the table pages land). The interaction core and render adapter are
// production-shaped but not yet reachable from a page, so the allow mirrors
// the skeleton reservation the module was seeded with.

//! Confirmation dialog: typed interaction core + themed bsn! render adapter.
//!
//! Dangerous intents flow through the shell's shared confirmation gate
//! (`ShellApp::pending_confirmation`); this widget renders and resolves one
//! gated intent, never owns it. The core models the shell's
//! [`PendingConfirmation`] **double-echo** semantics: a dialog is armed with
//! a frozen `target_id`, the rendered body displays that id, and the confirm
//! outcome echoes the same id back — so the wiring can prove the authorized
//! action is exactly the one the user read, and a stale or retargeted dialog
//! can never authorize a different target.
//!
//! Split per the widget-layer contract: the core below is plain data with
//! zero bevy deps (the headless-test surface); [`dialog_scene`] is the bsn!
//! adapter themed exclusively through [`crate::palette`] tokens. Labels
//! arrive pre-localized from the caller (i18n stays a page/shell concern).
//!
//! [`PendingConfirmation`]: taskmanager_application::PendingConfirmation

use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, UiRect, Val,
    percent, px,
};
use bevy::ui::widget::Text;

use crate::palette::{UiPalette, space_8, space_24};
use crate::window::{Role, TextRole};

/// A dialog's full neutral description. The body must be composed through
/// [`ConfirmationDialog::echoed_body`] when the double-echo contract applies,
/// so the displayed text carries the target id the confirm outcome echoes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DialogSpec {
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) confirm_label: String,
    pub(crate) dismiss_label: String,
}

/// Which physical action activated the dialog. Keyboard mapping (Enter
/// confirms, Esc dismisses) is caller-side routing into this vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DialogInput {
    Confirm,
    Dismiss,
}

/// The outcome of one activation. [`DialogOutcome::Confirmed`] echoes the
/// `target_id` the dialog was armed with — the second half of the
/// double-echo contract (the first half is the rendered body).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DialogOutcome {
    Confirmed { target_id: String },
    Dismissed,
}

/// A confirmation dialog bound to the target it would act on. The binding is
/// frozen at construction; activation never re-reads any live target list,
/// so a refresh between arm and confirm cannot redirect the intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfirmationDialog {
    pub(crate) spec: DialogSpec,
    pub(crate) target_id: String,
}

impl ConfirmationDialog {
    /// Bind a pre-localized dialog description to the target id it acts on.
    pub(crate) fn new(spec: DialogSpec, target_id: impl Into<String>) -> Self {
        Self {
            spec,
            target_id: target_id.into(),
        }
    }

    /// The body line the user actually confirms: the caller's body with the
    /// frozen target id echoed in. Rendering this (not the bare `spec.body`)
    /// is what makes the confirm echo verifiable on screen.
    pub(crate) fn echoed_body(&self) -> String {
        format!("{} — {}", self.spec.body, self.target_id)
    }

    /// Pure activation: `Confirm` echoes the armed target id back, `Dismiss`
    /// discards without echo. No shell mutation happens here — the wiring
    /// resolves the echo against the shell's pending gate.
    pub(crate) fn activate(&self, input: DialogInput) -> DialogOutcome {
        match input {
            DialogInput::Confirm => DialogOutcome::Confirmed {
                target_id: self.target_id.clone(),
            },
            DialogInput::Dismiss => DialogOutcome::Dismissed,
        }
    }
}

/// Render a confirmation dialog with the target id echoed in the body (the
/// first half of the double-echo contract). Title, echoed body, and the two
/// action labels — exactly four text nodes.
pub(crate) fn confirmation_scene(
    dialog: &ConfirmationDialog,
    palette: &UiPalette,
) -> impl Scene + use<> {
    dialog_scene(
        &DialogSpec {
            body: dialog.echoed_body(),
            ..dialog.spec.clone()
        },
        palette,
    )
}

/// The neutral panel render: title (heading), body (body ink), confirm and
/// dismiss labels. Kept signature-stable for the skeleton's callers; the
/// double-echo path composes it through [`confirmation_scene`].
pub(crate) fn dialog_scene(spec: &DialogSpec, palette: &UiPalette) -> impl Scene + use<> {
    let title = spec.title.clone();
    let body = spec.body.clone();
    let confirm = spec.confirm_label.clone();
    let dismiss = spec.dismiss_label.clone();
    let radius = palette.panel_radius_px;
    bsn! {
        Node {
            width: px(420.0),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_24())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
            ( Text(title) TextRole(Role::Heading) ),
            ( Text(body) TextRole(Role::Body) ),
            (
                Node {
                    width: percent(100),
                    height: Val::Auto,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::End,
                    column_gap: Val::Px(space_8()),
                }
                Children [
                    ( Text(confirm) TextRole(Role::Body) ),
                    ( Text(dismiss) TextRole(Role::Caption) ),
                ]
            ),
        ]
    }
}

/// Reserved: the scrim behind a modal. Standalone so pages can layer it
/// independently of the panel.
pub(crate) fn scrim_scene(palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: UiRect::all(Val::Px(space_8())),
        }
        BackgroundColor({ palette.scrim })
    }
}
