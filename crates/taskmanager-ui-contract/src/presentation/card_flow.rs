//! The shared card-flow admission: whole groups only, one clamp.
//!
//! Every graphical shape that presents a grid of cards must answer the same
//! two questions with the same arithmetic: how many COMPLETE groups fit in
//! the measured slot, and how tall may one card become inside that slot.
//! Answering them per frontend produced three near-identical ladders that
//! drifted, so the arithmetic lives here once: the shape measures its own
//! slot and its own card footprint, and spends this verdict.
//!
//! The rules are the elastic-layout playbook's rules, made arithmetic:
//!
//! - a group is admitted only when the COMPLETE group fits — no half card,
//!   no clipped row;
//! - an admitted group's height is the leftover space shared out, clamped
//!   into the card's own floor and ceiling, so a card never shrinks below
//!   its readable floor and never grows past its ceiling;
//! - when even one group does not fit, the answer is ZERO groups: the
//!   caller degrades to its dense/table form instead of squeezing a card.

/// One card's own footprint budget: the height floor it must keep, the
/// ceiling it may grow to, and the gap between two cards in a group.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardFlowSpec {
    /// The minimum height one card may take.
    pub floor: f32,
    /// The maximum height one card may take.
    pub ceiling: f32,
    /// The gap between two cards.
    pub gap: f32,
}

impl CardFlowSpec {
    /// Build a spec from a floor and a ceiling.
    ///
    /// An inverted or degenerate pair collapses to a single height, so the
    /// clamp below can never divide by zero or admit a negative group.
    #[must_use]
    pub fn new(floor: f32, ceiling: f32) -> Self {
        let floor = if floor.is_finite() && floor > 0.0 {
            floor
        } else {
            0.0
        };
        let ceiling = if ceiling.is_finite() && ceiling >= floor {
            ceiling
        } else {
            floor
        };
        Self {
            floor,
            ceiling,
            gap: 0.0,
        }
    }

    /// Set the gap between two cards.
    #[must_use]
    pub fn with_gap(mut self, gap: f32) -> Self {
        self.gap = if gap.is_finite() && gap > 0.0 {
            gap
        } else {
            0.0
        };
        self
    }

    /// The height one card takes when it cannot grow.
    #[must_use]
    pub fn floor_height(&self) -> f32 {
        self.floor
    }
}

/// The shared verdict for one card flow inside a measured slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardFlowBudget {
    /// The number of columns the slot admits.
    pub columns: usize,
    /// The number of COMPLETE groups admitted. Zero means the caller must
    /// degrade to its dense form.
    pub groups: usize,
    /// The height one card takes inside the admitted groups.
    pub card_height: f32,
}

impl CardFlowBudget {
    /// The number of cards the admitted groups hold.
    #[must_use]
    pub fn cards(&self) -> usize {
        self.groups.saturating_mul(self.columns)
    }
}

/// Admit whole card groups into a measured slot.
///
/// `available_height` is the height the flow may spend (the slot minus the
/// page's own fixed chrome, already net of the bottom safety band).
/// `columns` is how many cards sit side by side; `spec` is the card's own
/// floor/ceiling/gap. The leftover space is shared out over the admitted
/// groups and clamped into the card's floor and ceiling.
#[must_use]
pub fn admit_card_groups(
    available_height: f32,
    columns: usize,
    spec: CardFlowSpec,
) -> CardFlowBudget {
    let columns = columns.max(1);
    let available = if available_height.is_finite() && available_height > 0.0 {
        available_height
    } else {
        0.0
    };
    let floor = spec.floor_height();
    if floor <= 0.0 {
        return CardFlowBudget {
            columns,
            groups: 0,
            card_height: 0.0,
        };
    }
    let group_cost = floor + spec.gap;
    let groups = if available + spec.gap < group_cost {
        return CardFlowBudget {
            columns,
            groups: 0,
            card_height: 0.0,
        };
    } else {
        (((available + spec.gap) / group_cost).floor() as usize).max(1)
    };
    let gaps = spec.gap * (groups.saturating_sub(1)) as f32;
    let share = ((available - gaps) / groups as f32).clamp(floor, spec.ceiling);
    CardFlowBudget {
        columns,
        groups,
        card_height: share,
    }
}

/// Admit whole card groups and cap the group count.
///
/// The cap is the product's own content limit (a page that shows at most
/// four metrics), not a layout fact: the caller supplies it.
#[must_use]
pub fn admit_card_groups_capped(
    available_height: f32,
    columns: usize,
    spec: CardFlowSpec,
    max_groups: usize,
) -> CardFlowBudget {
    let mut budget = admit_card_groups(available_height, columns, spec);
    if budget.groups > max_groups {
        budget.groups = max_groups;
        if budget.groups == 0 {
            budget.card_height = 0.0;
            return budget;
        }
        let gaps = spec.gap * (budget.groups.saturating_sub(1)) as f32;
        let available = if available_height.is_finite() && available_height > 0.0 {
            available_height
        } else {
            0.0
        };
        let share =
            ((available - gaps) / budget.groups as f32).clamp(spec.floor_height(), spec.ceiling);
        budget.card_height = share;
    }
    budget
}

#[cfg(test)]
#[path = "../../tests/headless/ui_presentation_card_flow.rs"]
mod tests;
