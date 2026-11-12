//! The shared card-row width admission: whole groups inside one capped lane.
//!
//! [`super::card_flow`] answers how many COMPLETE group BLOCKS fit in a
//! measured slot and how tall one card may grow. It does not answer how many
//! cards sit side by side in one group. That width verdict is the same
//! elastic-layout law seen from the other axis: a card is admitted only when
//! its WHOLE lower content keeps its readable floor, the measured content
//! width never exceeds the shape's own card/content cap, and the group
//! header's overflow action is a separate whole group that must fit beside
//! the bucket label before any card column is spent.
//!
//! The shapes measure their own slot, their own card width floor and ceiling,
//! their own gap and (when a bucket is truncated) their own header overflow
//! width. This module owns the ARITHMETIC and the typed verdict, so no
//! renderer carries a second "how many columns fit" ladder, a second maximum
//! content width, or silently clips the last column.
//!
//! TWO row shapes live here, and both spend ONE share arithmetic
//! ([`card_width_in_row`]):
//!
//! - the BUCKETED lane ([`CardRowHeader::Bucketed`]): a group header, and an
//!   overflow action beside the bucket label when a bucket is truncated, sit
//!   above the cards, so the header's whole fact group is charged inside the
//!   bounded content width;
//! - the PLAIN grid ([`CardRowHeader::Plain`]): a grid of equal cards with a
//!   gap and no header at all. A plain grid used to be unable to bind this
//!   module, because the bucketed spec demanded positive header facts; the
//!   header is now a typed fact with an ABSENT variant, so a plain grid
//!   spends the same arithmetic instead of keeping its own one-line formula.

/// The width one card takes when `columns` cards share `content_width`.
///
/// This is the ONE share arithmetic of the card row: the bucketed lane and
/// the plain grid both spend it, so a page's row and its card can never
/// disagree about how wide one card is.
#[must_use]
pub const fn card_width_in_row(gap: f32, columns: usize, content_width: f32) -> f32 {
    if columns == 0 {
        return 0.0;
    }
    (content_width - gap * (columns - 1) as f32) / columns as f32
}

/// The group-header facts a card row may carry.
///
/// A BUCKETED lane paints a group header above its cards, and an overflow
/// action beside the bucket label when a bucket is truncated. A PLAIN grid
/// carries neither: every card is the same kind of card and the row's whole
/// content is the cards themselves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CardRowHeader {
    /// The lane paints a group header, and an overflow action beside the
    /// bucket label when a bucket is truncated.
    Bucketed {
        /// The narrowest the group header's bucket label may be.
        label_min_width: f32,
        /// The width the overflow action needs when a bucket is truncated.
        overflow_action_min_width: f32,
    },
    /// The row paints no header: a plain grid of equal cards.
    Plain,
}

impl CardRowHeader {
    /// The width the header's whole fact group needs.
    #[must_use]
    pub const fn width(self, has_overflow: bool) -> f32 {
        match self {
            Self::Bucketed {
                label_min_width,
                overflow_action_min_width,
            } => {
                if has_overflow {
                    label_min_width + overflow_action_min_width
                } else {
                    label_min_width
                }
            }
            Self::Plain => 0.0,
        }
    }

    /// Whether the header facts are measurable.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        match self {
            Self::Bucketed {
                label_min_width,
                overflow_action_min_width,
            } => {
                label_min_width.is_finite()
                    && label_min_width > 0.0
                    && overflow_action_min_width.is_finite()
                    && overflow_action_min_width > 0.0
            }
            Self::Plain => true,
        }
    }
}

/// The measured lane after whole columns and the cap have been admitted.
///
/// Reason row: a very wide slot must not inflate a capped number of cards
/// into empty banners. The lower content is therefore one bounded whole
/// group; its residual slot stays OUTSIDE that group as lane-owned space.
/// The same bounded width is the only space the card row and, when a bucket
/// is truncated, its overflow header may spend.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardRowWidthBudget {
    /// Complete card columns admitted by the measured slot and the caps.
    pub columns: usize,
    /// Bounded width the whole card row paints, in the shape's logical unit.
    pub content_width: f32,
    /// Largest width one card may paint inside the bounded content group.
    pub card_width: f32,
    /// Lane-owned residual space outside the bounded content group.
    pub residual_width: f32,
}

impl CardRowWidthBudget {
    /// A zero-width budget for a lane that admits no whole card.
    #[must_use]
    pub const fn degraded() -> Self {
        Self {
            columns: 0,
            content_width: 0.0,
            card_width: 0.0,
            residual_width: 0.0,
        }
    }

    /// Whether the lane has admitted at least one whole card column.
    #[must_use]
    pub const fn is_admitted(&self) -> bool {
        self.columns > 0
    }
}

/// The measured width facts one card row hands to the shared admission.
///
/// All length values are in the shape's own logical length unit. The
/// contract never reads a theme token: a shape lifts its own measured
/// numbers at its `presentation_binding.rs` seam (ADR-063 keeps the
/// numbers with the theme). `card_max_width` is a measured CONTENT cap,
/// not a second theme ladder and not a window breakpoint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardRowWidthSpec {
    /// The narrowest a whole card may be without clipping its fact line.
    pub card_min_width: f32,
    /// The widest one card may become before it reads as an empty banner.
    pub card_max_width: f32,
    /// The gap between two cards, also charged between header facts.
    pub gap: f32,
    /// The header facts the row carries: a bucketed lane's group header, or
    /// the absence of one for a plain grid.
    pub header: CardRowHeader,
    /// Largest column count the product paints in one card row.
    pub max_columns: usize,
    /// Largest width the whole lower card content may paint in one row.
    pub content_width_cap: f32,
}

impl CardRowWidthSpec {
    /// Build a spec from measured floors, ceilings, gaps and product caps.
    ///
    /// A degenerate floor, ceiling or gap collapses the whole row to the
    /// typed degraded verdict below, so no arithmetic divides by zero or
    /// admits a negative column.
    #[must_use]
    pub const fn new(
        card_min_width: f32,
        card_max_width: f32,
        gap: f32,
        header_label_min_width: f32,
        overflow_action_min_width: f32,
        max_columns: usize,
        content_width_cap: f32,
    ) -> Self {
        Self {
            card_min_width,
            card_max_width,
            gap,
            header: CardRowHeader::Bucketed {
                label_min_width: header_label_min_width,
                overflow_action_min_width,
            },
            max_columns,
            content_width_cap,
        }
    }

    /// Build the spec of a PLAIN grid of equal cards.
    ///
    /// A plain grid has no group header and no overflow action, so the row's
    /// whole content is the cards themselves. Its content cap is the measured
    /// card ceiling over the product's column count: the grid never inflates
    /// its cards past that ceiling into empty banners.
    #[must_use]
    pub const fn plain_grid(
        card_min_width: f32,
        card_max_width: f32,
        gap: f32,
        max_columns: usize,
    ) -> Self {
        let columns = if max_columns > 0 { max_columns } else { 1 };
        let content_width_cap =
            card_max_width * columns as f32 + gap * (columns.saturating_sub(1)) as f32;
        Self {
            card_min_width,
            card_max_width,
            gap,
            header: CardRowHeader::Plain,
            max_columns,
            content_width_cap,
        }
    }

    /// Derive the whole-row content cap from the shape's measured card cap.
    ///
    /// Reason row: the widest useful lower content is `max_columns` cards,
    /// each at the measured card ceiling, plus their gaps. A shape supplies
    /// measured lengths and product limits; the fold, not the renderer,
    /// owns the cap arithmetic. Slot space beyond this width stays with the
    /// lane and cannot enlarge a card into an empty banner.
    #[must_use]
    pub const fn from_measured_card_cap(
        card_min_width: f32,
        card_max_width: f32,
        gap: f32,
        header_label_min_width: f32,
        overflow_action_min_width: f32,
        max_columns: usize,
    ) -> Self {
        let columns = if max_columns > 0 { max_columns } else { 1 };
        let content_width_cap =
            card_max_width * columns as f32 + gap * (columns.saturating_sub(1)) as f32;
        Self::new(
            card_min_width,
            card_max_width,
            gap,
            header_label_min_width,
            overflow_action_min_width,
            max_columns,
            content_width_cap,
        )
    }

    /// Whether every measured fact is finite and strictly positive.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.card_min_width.is_finite()
            && self.card_min_width > 0.0
            && self.card_max_width.is_finite()
            && self.card_max_width >= self.card_min_width
            && self.gap.is_finite()
            && self.gap > 0.0
            && self.header.is_valid()
            && self.max_columns > 0
            && self.content_width_cap.is_finite()
            && self.content_width_cap >= self.card_min_width
            && self.content_width_cap >= self.header_width(true)
            && self.content_width_cap >= self.row_width(self.max_columns)
    }

    /// Width of a whole `columns`-card row, charging every gap between cards.
    #[must_use]
    pub const fn row_width(&self, columns: usize) -> f32 {
        if columns == 0 {
            return 0.0;
        }
        self.card_min_width * columns as f32 + self.gap * (columns - 1) as f32
    }

    /// The largest whole row width `columns` complete cards may spend.
    ///
    /// Reason row: each card keeps its measured floor, never exceeds its
    /// measured card ceiling, and the whole row never exceeds the measured
    /// content cap. Gaps between cards are charged once, never stretched.
    #[must_use]
    pub fn row_width_cap(&self, columns: usize) -> f32 {
        if columns == 0 {
            return 0.0;
        }
        let card_cap = self.card_max_width * columns as f32;
        let gaps = self.gap * (columns - 1) as f32;
        (card_cap + gaps).min(self.content_width_cap)
    }

    /// The row width a measured slot actually spends for `columns` cards.
    ///
    /// The content cap and the slot both bound the row; neither the card
    /// floor nor the card ceiling is reinterpreted by this clamp.
    #[must_use]
    pub fn row_width_bounded(&self, columns: usize, slot_width: f32) -> f32 {
        let slot = if slot_width.is_finite() && slot_width > 0.0 {
            slot_width
        } else {
            0.0
        };
        self.row_width_cap(columns).min(slot)
    }

    /// Width one card paints when `columns` cards share `content_width`.
    #[must_use]
    pub const fn card_width(&self, columns: usize, content_width: f32) -> f32 {
        card_width_in_row(self.gap, columns, content_width)
    }

    /// Width the group header needs with and without its overflow action.
    #[must_use]
    pub const fn header_width(&self, has_overflow: bool) -> f32 {
        match self.header {
            CardRowHeader::Bucketed {
                label_min_width,
                overflow_action_min_width,
            } => {
                if has_overflow {
                    label_min_width + self.gap + overflow_action_min_width
                } else {
                    label_min_width
                }
            }
            CardRowHeader::Plain => 0.0,
        }
    }
}

/// The typed width verdict for ONE admitted group row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CardRowWidthAdmission {
    /// Every requested whole card column fits; paint the bounded full row.
    Full {
        columns: usize,
        content_width: f32,
        card_width: f32,
        residual_width: f32,
    },
    /// Only ONE card keeps its whole lower content in this slot.
    SingleColumn {
        content_width: f32,
        card_width: f32,
        residual_width: f32,
    },
    /// Even one card, or the header's whole fact group, cannot keep its floor.
    /// The caller degrades the WHOLE lane instead of clipping a column.
    Degrade,
}

impl CardRowWidthAdmission {
    /// The number of cards the admitted row paints; zero means degraded.
    #[must_use]
    pub const fn columns(&self) -> usize {
        match self {
            Self::Full { columns, .. } => *columns,
            Self::SingleColumn { .. } => 1,
            Self::Degrade => 0,
        }
    }

    /// Whether the lane paints any card at all.
    #[must_use]
    pub const fn is_admitted(&self) -> bool {
        !matches!(self, Self::Degrade)
    }

    /// The bounded lane a renderer may paint. A degraded lane owns no width.
    #[must_use]
    pub const fn budget(&self) -> CardRowWidthBudget {
        match self {
            Self::Full {
                columns,
                content_width,
                card_width,
                residual_width,
            } => CardRowWidthBudget {
                columns: *columns,
                content_width: *content_width,
                card_width: *card_width,
                residual_width: *residual_width,
            },
            Self::SingleColumn {
                content_width,
                card_width,
                residual_width,
            } => CardRowWidthBudget {
                columns: 1,
                content_width: *content_width,
                card_width: *card_width,
                residual_width: *residual_width,
            },
            Self::Degrade => CardRowWidthBudget::degraded(),
        }
    }
}

/// How many whole card columns a measured slot admits, and how many columns
/// the shape may request in the first place.
///
/// `requested_columns` is the shape's own measured width ceiling; this fold
/// clamps it into the product column cap and the measured content cap. The
/// admission then spends the smaller of that and the width the slot can
/// actually hold, whole column at a time.
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
    let spendable = slot_width.min(spec.content_width_cap);
    // The header is a separate whole group ABOVE the card row. When the
    // bucket is truncated, its label and overflow action must fit together;
    // a bounded content group that clips either fact cannot spend a card
    // column at all.
    let header_width = spec.header_width(has_overflow);
    if header_width > spendable {
        return CardRowWidthAdmission::Degrade;
    }
    if spec.row_width(1) > spendable {
        return CardRowWidthAdmission::Degrade;
    }
    let cap = spec.max_columns.max(1);
    let requested = requested_columns.clamp(1, cap);
    let mut columns = 1usize;
    while columns < requested && spec.row_width(columns + 1) <= spendable {
        let bounded = spec.row_width_bounded(columns + 1, spendable);
        if bounded < spec.row_width(columns + 1) || header_width > bounded {
            break;
        }
        columns += 1;
    }
    let content_width = spec.row_width_bounded(columns, spendable);
    if content_width < spec.row_width(columns) || header_width > content_width {
        return CardRowWidthAdmission::Degrade;
    }
    let card_width = spec.card_width(columns, content_width);
    if card_width < spec.card_min_width || card_width > spec.card_max_width {
        return CardRowWidthAdmission::Degrade;
    }
    let residual_width = (slot_width - content_width).max(0.0);
    if columns == 1 {
        CardRowWidthAdmission::SingleColumn {
            content_width,
            card_width,
            residual_width,
        }
    } else {
        CardRowWidthAdmission::Full {
            columns,
            content_width,
            card_width,
            residual_width,
        }
    }
}

/// The largest whole column count a measured slot and the content cap admit.
///
/// This is pure geometry over the same spec, so the shape's own column
/// request and the admission verdict can never disagree about the card
/// floor or the measured maximum content width.
#[must_use]
pub fn card_row_columns(slot_width: f32, spec: CardRowWidthSpec) -> usize {
    if !slot_width.is_finite() || slot_width <= 0.0 || !spec.is_valid() {
        return 1;
    }
    let spendable = slot_width.min(spec.content_width_cap);
    let numerator = spendable + spec.gap;
    let denominator = spec.card_min_width + spec.gap;
    let columns = (numerator / denominator).floor() as usize;
    columns.clamp(1, spec.max_columns.max(1))
}

/// Admit one bounded card row and return its paint-ready width budget.
///
/// The header's whole overflow group is charged INSIDE the bounded content
/// width; it can never be paid for by the residual lane space. A truncated
/// bucket therefore cannot bypass the cap by widening its header.
#[must_use]
pub fn admit_card_row_width_budget(
    slot_width: f32,
    requested_columns: usize,
    spec: CardRowWidthSpec,
    has_overflow: bool,
) -> CardRowWidthBudget {
    admit_card_row_width(slot_width, requested_columns, spec, has_overflow).budget()
}

/// The measured facts one PLAIN grid of equal cards spends.
///
/// A plain grid is a set of equal cards separated by one gap. It has no
/// group header and no overflow action, so the only facts it measures are
/// the gap and the product's column cap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardGridWidthSpec {
    /// The gap between two cards.
    pub gap: f32,
    /// Largest column count the product paints in one grid row.
    pub max_columns: usize,
}

impl CardGridWidthSpec {
    /// Build a plain-grid spec from its gap and the product's column cap.
    #[must_use]
    pub const fn new(gap: f32, max_columns: usize) -> Self {
        Self { gap, max_columns }
    }

    /// Whether every measured fact is finite and strictly positive.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.gap.is_finite() && self.gap > 0.0 && self.max_columns > 0
    }
}

/// The paint-ready width budget of one plain grid row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardGridWidthBudget {
    /// The columns the grid paints.
    pub columns: usize,
    /// The width one card takes.
    pub card_width: f32,
    /// The width the grid's row spans.
    pub content_width: f32,
}

/// Admit one plain grid row into a measured slot.
///
/// The requested columns share the measured slot minus the gaps between
/// them, and the row fills the slot exactly: no dead band survives at its
/// right edge. The share arithmetic is the SAME one the bucketed lane
/// spends ([`card_width_in_row`]), so a page's grid row and its bucketed
/// lane can never disagree about how wide one card is.
///
/// A degenerate slot or spec admits no card: the verdict is the typed
/// zero-width budget, never a negative card.
#[must_use]
pub fn admit_card_grid_width(
    slot_width: f32,
    requested_columns: usize,
    spec: CardGridWidthSpec,
) -> CardGridWidthBudget {
    let slot = if slot_width.is_finite() && slot_width > 0.0 {
        slot_width
    } else {
        0.0
    };
    if !spec.is_valid() {
        return CardGridWidthBudget {
            columns: 0,
            card_width: 0.0,
            content_width: 0.0,
        };
    }
    let columns = requested_columns.clamp(1, spec.max_columns);
    CardGridWidthBudget {
        columns,
        card_width: card_width_in_row(spec.gap, columns, slot),
        content_width: slot,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/ui_presentation_card_width.rs"]
mod tests;
