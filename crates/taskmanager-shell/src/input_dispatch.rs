//! Explicit result of routing one frontend input through the shell.

use taskmanager_application::PlatformEffect;

/// Distinguishes an unrecognized input from a consumed input that happens not
/// to require platform work. `Option<PlatformEffect>` cannot represent that
/// distinction and previously forced frontends to infer it from surrounding
/// booleans and duplicate key lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputDispatch {
    /// No shell binding or active input owner recognized the input.
    Unhandled,
    /// The shell consumed the input through a pure state transition.
    Consumed,
    /// The shell consumed the input and requested platform work.
    Effect(Box<PlatformEffect>),
}

impl InputDispatch {
    /// Convert a consumed reducer result into the explicit routing outcome.
    #[must_use]
    pub fn consumed(effect: Option<PlatformEffect>) -> Self {
        match effect {
            Some(effect) => Self::Effect(Box::new(effect)),
            None => Self::Consumed,
        }
    }

    /// Recover platform work, if this consumed input produced any.
    #[must_use]
    pub fn into_effect(self) -> Option<PlatformEffect> {
        match self {
            Self::Effect(effect) => Some(*effect),
            Self::Unhandled | Self::Consumed => None,
        }
    }

    /// Whether an input owner or binding consumed the input.
    #[must_use]
    pub const fn is_consumed(&self) -> bool {
        !matches!(self, Self::Unhandled)
    }
}
