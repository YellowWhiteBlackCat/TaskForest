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
//! - when even one group does not fit, a caller may admit one complete
//!   focus card if its own floor fits; otherwise the answer degrades to the
//!   dense/table form instead of squeezing a card.

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
    /// The height one admitted card KEEPS.
    ///
    /// The admitted groups share the measured slot equally, so this is the
    /// height a card takes when the groups fill the slot exactly. It is a
    /// FLOOR, not a ceiling: a renderer whose cards sit in elastic lines
    /// spends it as the card's minimum height, and the line grows the card
    /// into whatever the slot still holds. A slot larger than the
    /// measurement — a conservative chrome budget, a theme difference — then
    /// fills the card instead of leaving a dead band under the last row.
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

/// The typed degradation rung for a card presentation.
///
/// A focus card is one complete selected item without a group header. It is
/// admitted only after whole groups fail and only when its complete measured
/// card floor fits. `Dense` means even that card must yield.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CardFlowAdmission {
    /// One or more complete group blocks fit.
    Groups(CardFlowBudget),
    /// One complete focus card fits after optional leading chrome is removed.
    FocusCard {
        /// The card's complete height, already at or above its measured floor.
        card_height: f32,
    },
    /// Neither a complete group nor one complete focus card fits.
    Dense,
}

/// Admit whole groups first, then one whole focus card, then the dense rung.
///
/// `available_height` is the complete adaptive lane below its fixed mode row.
/// `leading_height` is optional chrome shown with the group presentation
/// (for example, a dashboard); the caller removes that chrome when the
/// `FocusCard` rung is selected. The focus card is therefore checked against
/// the full adaptive lane, while groups are checked after reserving the
/// leading chrome. A non-finite or negative measurement admits no card.
#[must_use]
pub fn admit_card_flow(
    available_height: f32,
    leading_height: f32,
    columns: usize,
    spec: CardFlowSpec,
    focus_card_floor: f32,
) -> CardFlowAdmission {
    if columns == 0 {
        return CardFlowAdmission::Dense;
    }
    let available = if available_height.is_finite() && available_height > 0.0 {
        available_height
    } else {
        0.0
    };
    let leading = if leading_height.is_finite() && leading_height > 0.0 {
        leading_height
    } else {
        0.0
    };
    let groups = admit_card_groups((available - leading).max(0.0), columns, spec);
    if groups.groups > 0 {
        return CardFlowAdmission::Groups(groups);
    }
    if focus_card_floor.is_finite() && focus_card_floor > 0.0 && available >= focus_card_floor {
        return CardFlowAdmission::FocusCard {
            card_height: focus_card_floor,
        };
    }
    CardFlowAdmission::Dense
}

/// Whether a focus card already represents the selected member of a group.
///
/// Group presentation may show one complete selected item above its status
/// bucket. The bucket then renders only the remaining members, and its
/// overflow count treats the focus card as one represented member instead
/// of painting the same identity twice.
#[must_use]
pub fn focus_represents_group<G: PartialEq>(
    focus_group: Option<G>,
    group: G,
    first_member: Option<usize>,
    selected: usize,
) -> bool {
    focus_group == Some(group) && first_member == Some(selected)
}

/// Remove the selected member from a group represented by a focus card.
///
/// This is the one typed de-duplication rule for card flows. It preserves
/// provider-issued member indices and their order; callers still apply their
/// own width and count budgets to the returned vector.
#[must_use]
pub fn group_members_without_focus(
    members: &[usize],
    selected: usize,
    focus_is_first: bool,
) -> Vec<usize> {
    if !focus_is_first {
        return members.to_vec();
    }
    members
        .iter()
        .copied()
        .filter(|index| *index != selected)
        .collect()
}

/// Count group members not painted in the bounded card row.
///
/// The focus card counts as a represented member in `total`, so the overflow
/// affordance never claims that an already visible identity is hidden.
#[must_use]
pub fn hidden_group_members(total: usize, painted: usize, focus_is_first: bool) -> usize {
    total.saturating_sub(painted.saturating_add(usize::from(focus_is_first)))
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
