//! The shared card-row width admission: whole columns only, one arithmetic.
//!
//! [`super::card_flow`] answers how many COMPLETE group BLOCKS fit in a
//! measured slot and how tall one card may grow. It does not answer how many
//! cards sit side by side in one group. That width verdict is the same
//! elastic-layout law seen from the other axis: a card is admitted only when
//! its WHOLE lower content keeps its readable floor, and the group header's
//! overflow action is a separate whole group that must fit beside the bucket
//! label before any card column is spent.
//!
//! The shapes measure their own slot, their own card width floor, their own
//! gap and (when a bucket is truncated) their own header overflow width. This
//! module owns the ARITHMETIC and the typed verdict, so no renderer carries a
//! second "how many columns fit" ladder or silently clips the last column.

/// The measured width facts one card row hands to the shared admission.
///
/// All four values are in the shape's own logical length unit. The contract
/// never reads a theme token: a shape lifts its own measured numbers at its
/// `presentation_binding.rs` seam (ADR-063 keeps the numbers with the theme).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardRowWidthSpec {
    /// The narrowest a whole card may be without clipping its fact line.
    pub card_min_width: f32,
    /// The gap between two cards, also charged between header facts.
    pub gap: f32,
    /// The narrowest the group header's bucket label may be.
    pub header_label_min_width: f32,
    /// The width the overflow action needs when a bucket is truncated.
    pub overflow_action_min_width: f32,
    /// Largest column count the product paints in one card row.
    pub max_columns: usize,
}

impl CardRowWidthSpec {
    /// Build a spec from a card floor, a gap, the two header floors and a cap.
    ///
    /// A degenerate floor or gap collapses the whole row to the typed
    /// degraded verdict below, so no arithmetic divides by zero or admits a
    /// negative column.
    #[must_use]
    pub const fn new(
        card_min_width: f32,
        gap: f32,
        header_label_min_width: f32,
        overflow_action_min_width: f32,
        max_columns: usize,
    ) -> Self {
        Self {
            card_min_width,
            gap,
            header_label_min_width,
            overflow_action_min_width,
            max_columns,
        }
    }

    /// Whether every measured fact is finite and strictly positive.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.card_min_width.is_finite()
            && self.card_min_width > 0.0
            && self.gap.is_finite()
            && self.gap > 0.0
            && self.header_label_min_width.is_finite()
            && self.header_label_min_width > 0.0
            && self.overflow_action_min_width.is_finite()
            && self.overflow_action_min_width > 0.0
            && self.max_columns > 0
    }

    /// Width of a whole `columns`-card row, charging every gap between cards.
    #[must_use]
    pub const fn row_width(&self, columns: usize) -> f32 {
        if columns == 0 {
            return 0.0;
        }
        self.card_min_width * columns as f32 + self.gap * (columns - 1) as f32
    }

    /// Width the group header needs with and without its overflow action.
    #[must_use]
    pub const fn header_width(&self, has_overflow: bool) -> f32 {
        if has_overflow {
            self.header_label_min_width + self.gap + self.overflow_action_min_width
        } else {
            self.header_label_min_width
        }
    }
}

/// The typed width verdict for ONE admitted group row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardRowWidthAdmission {
    /// Every requested whole card column fits; paint the full row.
    Full { columns: usize },
    /// Only ONE card keeps its whole lower content in this slot.
    SingleColumn,
    /// Even one card, or the header's whole fact group, cannot keep its floor.
    /// The caller degrades the WHOLE lane instead of clipping a column.
    Degrade,
}

impl CardRowWidthAdmission {
    /// The number of cards the admitted row paints; zero means degraded.
    #[must_use]
    pub const fn columns(&self) -> usize {
        match self {
            Self::Full { columns } => *columns,
            Self::SingleColumn => 1,
            Self::Degrade => 0,
        }
    }

    /// Whether the lane paints any card at all.
    #[must_use]
    pub const fn is_admitted(&self) -> bool {
        !matches!(self, Self::Degrade)
    }
}

/// How many whole card columns a measured slot admits, and how many columns
/// the shape may request in the first place.
///
/// `requested_columns` is the shape's own measured width ceiling (for example
/// the slot ÷ minimum card width); this fold only clamps it into the product
/// cap. The admission then spends the smaller of that and the width the slot
/// can actually hold, whole column at a time.
#[must_use]
pub fn admit_card_row_width(
    slot_width: f32,
    requested_columns: usize,
    spec: CardRowWidthSpec,
    has_overflow: bool,
) -> CardRowWidthAdmission {
    if !slot_width.is_finite() || slot_width <= 0.0 || !spec.is_valid() {
        return CardRowWidthAdmission::Degrade;
    }
    // The header is a separate whole group ABOVE the card row. When the
    // bucket is truncated, its label and overflow action must fit together;
    // a slot that clips either fact cannot spend a card column at all.
    if spec.header_width(has_overflow) > slot_width {
        return CardRowWidthAdmission::Degrade;
    }
    if spec.row_width(1) > slot_width {
        return CardRowWidthAdmission::Degrade;
    }
    let cap = spec.max_columns.max(1);
    let requested = requested_columns.clamp(1, cap);
    let mut columns = 1usize;
    while columns < requested && spec.row_width(columns + 1) <= slot_width {
        columns += 1;
    }
    if columns == 1 {
        CardRowWidthAdmission::SingleColumn
    } else {
        CardRowWidthAdmission::Full { columns }
    }
}

/// The largest whole column count a measured slot can request before gaps.
///
/// This is pure geometry over the same spec, so the shape's own column
/// request and the admission verdict can never disagree about the card floor.
#[must_use]
pub fn card_row_columns(slot_width: f32, spec: CardRowWidthSpec) -> usize {
    if !slot_width.is_finite() || slot_width <= 0.0 || !spec.is_valid() {
        return 1;
    }
    let numerator = slot_width + spec.gap;
    let denominator = spec.card_min_width + spec.gap;
    let columns = (numerator / denominator).floor() as usize;
    columns.clamp(1, spec.max_columns.max(1))
}

#[cfg(test)]
#[path = "../../tests/headless/ui_presentation_card_width.rs"]
mod tests;
