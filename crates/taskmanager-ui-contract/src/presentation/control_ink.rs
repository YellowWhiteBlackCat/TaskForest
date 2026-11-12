//! The one interaction-ink precedence fold for every frontend control.
//!
//! Toolkit signals arrive in different shapes — GPUI element state, Iced
//! widget `Status`, Bevy `PickingInteraction`/`Pressed`, TUI cursor rows —
//! but a surface can paint only one ink. Each binding projects its native
//! signals into the booleans below and consumes one typed verdict. A state
//! that changes paint never changes geometry: focus rings and disabled ink
//! ride the same measured control slot as idle.
//!
//! The precedence is deliberately total and stable:
//! `Disabled > Pressed > Focused > Hovered > Selected > Idle`. Disabled wins
//! because a control that cannot act must not advertise interaction; press
//! wins over hover because the pointer is already on the control; visible
//! keyboard focus wins over hover so the modality ring remains discoverable;
//! selection is the durable page state and yields to every transient signal.

/// The typed ink a control surface resolves before choosing theme tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControlInkState {
    /// No interaction or page selection.
    Idle,
    /// The pointer rests on an enabled control.
    Hovered,
    /// An enabled control is being activated.
    Pressed,
    /// Keyboard-visible focus owns the ring.
    Focused,
    /// The control is unavailable and keeps its measured target.
    Disabled,
    /// The page selected this control without a transient interaction.
    Selected,
}

impl ControlInkState {
    /// Every ink state, in precedence order from strongest to weakest.
    pub const ALL: [Self; 6] = [
        Self::Disabled,
        Self::Pressed,
        Self::Focused,
        Self::Hovered,
        Self::Selected,
        Self::Idle,
    ];

    /// Whether this ink may change only paint, never the control's extent.
    #[must_use]
    pub const fn changes_measurement(self) -> bool {
        false
    }

    /// Whether this state represents an unavailable control.
    #[must_use]
    pub const fn is_disabled(self) -> bool {
        matches!(self, Self::Disabled)
    }
}

/// Resolve one control's ink from independent native interaction signals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ControlInkPriority {
    /// The pointer rests on the control.
    pub hovered: bool,
    /// The control is being activated.
    pub pressed: bool,
    /// Keyboard-visible focus owns the control.
    pub focused: bool,
    /// The control cannot be activated.
    pub disabled: bool,
    /// The page selected the control.
    pub selected: bool,
}

impl ControlInkPriority {
    /// The resting interaction verdict.
    pub const IDLE: Self = Self {
        hovered: false,
        pressed: false,
        focused: false,
        disabled: false,
        selected: false,
    };

    /// The one precedence fold. Conflicting bits resolve to exactly one ink.
    #[must_use]
    pub const fn resolve(
        hovered: bool,
        pressed: bool,
        focused: bool,
        disabled: bool,
        selected: bool,
    ) -> ControlInkState {
        if disabled {
            ControlInkState::Disabled
        } else if pressed {
            ControlInkState::Pressed
        } else if focused {
            ControlInkState::Focused
        } else if hovered {
            ControlInkState::Hovered
        } else if selected {
            ControlInkState::Selected
        } else {
            ControlInkState::Idle
        }
    }

    /// Resolve the signals carried by this value.
    #[must_use]
    pub const fn state(self) -> ControlInkState {
        Self::resolve(
            self.hovered,
            self.pressed,
            self.focused,
            self.disabled,
            self.selected,
        )
    }
}

#[cfg(test)]
#[path = "../../tests/headless/ui_presentation_control_ink.rs"]
mod tests;
