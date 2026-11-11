//! Applications card lane: a current-focus dashboard and one bounded
//! atomic-card flow over the existing shell row projection.
//!
//! FlClash v0.9.1's public redesign is a layout reference only: dashboard
//! around current focus, proxy groups as cards, group tabs as chips, optional
//! two-tier Liquid Glass, and iOS-style surfaces. No asset or implementation
//! is copied. Cards are another presentation of the same shell-owned rows;
//! filtering, sorting, selection, identity, and virtual scrolling remain
//! shell-owned. The static hero may use the real optical tier, while the
//! scrolled cards use painted glass; nested optical glass is not claimed.
//!
//! The lane renders NO filter chips of its own: the page chrome's canonical
//! status-filter pills already classify by the very same provider state the
//! buckets use, so a second chip row would be the same fact painted twice.
//! The group headers carry the bucket identity and its size instead.

use bevy::ecs::component::Component;
use bevy::ecs::event::Event;
use bevy::ecs::resource::Resource;
use taskforest_core::core::process::ProcessItem;
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::{MISSING_VALUE, bytes};
use taskmanager_shell::saved_views::sort_token;
use taskmanager_shell::{ProcessStatusFilter, ShellApp, SortDir};
use taskmanager_ui_contract::presentation::card_flow::{CardFlowSpec, admit_card_groups};
use taskmanager_ui_contract::presentation::card_width::{
    CardRowWidthSpec, admit_card_row_width, card_row_columns,
};

use crate::palette::{UiPalette, space_4, space_8, space_16};

/// The two renderings of one shell projection. Cards are the focus lane;
/// Dense keeps the measured table and every cell/menu/resize seam unchanged.
/// This is a presentation resource, not business state.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ProcessViewMode {
    #[default]
    Cards,
    Dense,
}

/// The static hero card. It alone may ask for the optical tier.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessFocusDashboard;

/// Replaceable hero content. The root and optical marker survive rebuilds.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessFocusContent;

/// One dashboard fact's mutable value leaf. `field` names the fact; the
/// renderer never re-derives business state from this marker.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessFocusValue(pub(crate) ProcessFocusField);

/// Live direction marker for the dashboard's inline sort icon. The runtime
/// re-stamps the icon when the shell's sort direction changes.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessFocusSortDir(pub(crate) bool);

/// The dashboard facts the hero card renders from the shell projection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ProcessFocusField {
    #[default]
    Name,
    Status,
    Pid,
    Cpu,
    Memory,
    Count,
    Sort,
    Query,
}

/// Root of the bounded atomic-card viewport. Only complete groups are
/// admitted from a measured slot.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessCardFlow;

/// One complete state group.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessCardGroup(pub(crate) ProcessStatusFilter);

/// One atomic process card, carrying the shell visible-row coordinate.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessAtomicCard(pub(crate) usize);

/// The group header's overflow action: reveal every row of this bucket.
///
/// The bucket's admitted row is bounded by the measured column count, so a
/// bucket with more members than columns states the remainder. This action
/// is how the remainder becomes reachable: it reduces through the shell's
/// canonical status filter and hands the page to its dense table.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessGroupOverflow(pub(crate) ProcessStatusFilter);

/// The card's state tint overlay.
///
/// An optical card has no painted fill to tint, so the hover, pressed and
/// selected ink rides a dedicated overlay above the glass instead of
/// replacing the surface the material owns.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessCardStateOverlay;

/// Official Button marker for the two presentation modes.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessViewModeButton(pub(crate) ProcessViewMode);

/// Typed presentation-mode intent. A global Event (not an EntityEvent) is
/// correct here: the mode RESOURCE is the target, not one entity.
#[derive(Clone, Debug, Event)]
pub(crate) struct ProcessViewModeChanged(pub(crate) ProcessViewMode);

/// One selected task plus the visible-row facts already owned by the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FocusProjection {
    pub(crate) name: Option<String>,
    pub(crate) pid: Option<u32>,
    pub(crate) status: Option<String>,
    pub(crate) cpu: Option<String>,
    pub(crate) memory: Option<String>,
    pub(crate) visible_count: usize,
    pub(crate) query: String,
    pub(crate) sort_column: String,
    pub(crate) sort_descending: bool,
    pub(crate) active_group: ProcessStatusFilter,
}

impl FocusProjection {
    /// One fact's final display text. Unavailable facts use the shared dash,
    /// never a fabricated zero or success.
    #[must_use]
    pub(crate) fn value(&self, field: ProcessFocusField) -> String {
        match field {
            ProcessFocusField::Name => self
                .name
                .clone()
                .unwrap_or_else(|| t("empty.no_process_selected").to_owned()),
            ProcessFocusField::Status => self.status.clone().unwrap_or_else(missing_value),
            ProcessFocusField::Pid => self.pid.map_or_else(missing_value, |pid| pid.to_string()),
            ProcessFocusField::Cpu => self.cpu.clone().unwrap_or_else(missing_value),
            ProcessFocusField::Memory => self.memory.clone().unwrap_or_else(missing_value),
            ProcessFocusField::Count => self.visible_count.to_string(),
            ProcessFocusField::Sort => self.sort_column.clone(),
            ProcessFocusField::Query => {
                if self.query.trim().is_empty() {
                    t("common.all").to_owned()
                } else {
                    self.query.clone()
                }
            }
        }
    }
}

/// Bevy's own card metrics for the shared whole-group admission contract.
///
/// Only the toolkit-specific lengths live here: the measured atomic-card
/// height (the control ladder plus the card's own spacing), the group header
/// height, the gap between two group blocks, and the column count this
/// frontend paints. The ADMISSION arithmetic — how many COMPLETE groups fit,
/// the clamped card height, and the zero-group verdict — belongs to the
/// shared `taskmanager_ui-contract::presentation::card_flow` contract, so no
/// frontend carries a second ladder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CardFlowMetrics {
    /// The control height one card row line occupies, in logical pixels.
    pub(crate) control_height_px: f32,
    /// The measured atomic-card height, in logical pixels.
    pub(crate) card_height_px: f32,
    /// The narrowest an atomic card may be, in logical pixels.
    pub(crate) card_min_width_px: f32,
    /// The group header height, in logical pixels.
    pub(crate) header_height_px: f32,
    /// The gap between two group blocks, in logical pixels.
    pub(crate) group_gap_px: f32,
    /// The smallest height one atomic card may keep.
    pub(crate) card_floor_px: f32,
    /// The tallest height one atomic card may grow to.
    pub(crate) card_ceiling_px: f32,
    /// The group header's bucket label floor, measured from the caption role.
    pub(crate) header_label_min_width_px: f32,
    /// The header overflow action's own readable floor.
    pub(crate) overflow_action_min_width_px: f32,
    /// Largest card-column count the product paints in one row.
    pub(crate) max_columns: usize,
}

impl CardFlowMetrics {
    /// Resolve Bevy's own card metrics from the palette's control ladder.
    #[must_use]
    pub(crate) fn from_palette(palette: &UiPalette) -> Self {
        let control_height_px = palette.control_height_px();
        // A card's content is fixed: the name/status line, the fact line,
        // the gap between them and the card's own padding. It never grows
        // with the slot, so the card's floor and ceiling are the same
        // height and leftover space stays OUTSIDE the card instead of
        // inflating it into an empty panel.
        let card_height_px = control_height_px * 2.0 + space_4() + space_16();
        // The card's WIDTH floor is set by its fact line: PID, CPU and
        // memory each carry a label and a value, and below this width the
        // memory value clips instead of truncating. Eight control heights
        // is the smallest multiple that holds that line in the standard
        // type size, so a wide slot paints a card GRID instead of a row of
        // wide banners.
        let card_min_width_px = control_height_px * 8.0;
        Self {
            control_height_px,
            card_height_px,
            card_min_width_px,
            header_height_px: control_height_px,
            group_gap_px: space_8(),
            card_floor_px: card_height_px,
            card_ceiling_px: card_height_px,
            header_label_min_width_px: control_height_px * 3.0,
            overflow_action_min_width_px: control_height_px * 4.0,
            max_columns: 4,
        }
    }

    /// The measured width facts the shared card-row admission spends.
    #[must_use]
    pub(crate) const fn width_spec(self) -> CardRowWidthSpec {
        CardRowWidthSpec::new(
            self.card_min_width_px,
            self.group_gap_px,
            self.header_label_min_width_px,
            self.overflow_action_min_width_px,
            self.max_columns,
        )
    }

    /// The shared contract's spec for one group block: the header plus the
    /// card row it introduces.
    #[must_use]
    pub(crate) fn spec(self) -> CardFlowSpec {
        CardFlowSpec::new(
            self.header_height_px + self.card_floor_px,
            self.header_height_px + self.card_ceiling_px,
        )
        .with_gap(self.group_gap_px)
    }
}

/// The shared contract's verdict, projected into the page's typed budget.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub(crate) struct CardFlowBudget {
    /// Whether the budget has been resolved from a measured card slot. A
    /// non-measured resource must not hide the lane with a false zero-group
    /// verdict during the first frames of window composition.
    pub(crate) measured: bool,
    /// The number of COMPLETE group blocks admitted. Zero means the lane
    /// degrades to its dense form instead of squeezing a card.
    pub(crate) groups: usize,
    /// The clamped height of one admitted group block.
    pub(crate) group_height_px: f32,
    /// The number of cards one group row holds.
    pub(crate) columns: usize,
    /// The measured width the group rows were admitted against, in logical pixels.
    pub(crate) width_px: f32,
    /// The palette-derived card metrics behind that width admission.
    pub(crate) metrics: Option<CardFlowMetrics>,
}

impl Default for CardFlowBudget {
    fn default() -> Self {
        Self {
            measured: false,
            groups: 0,
            group_height_px: 0.0,
            columns: 1,
            width_px: 0.0,
            metrics: None,
        }
    }
}

impl CardFlowBudget {
    /// The presentation the budget actually admits.
    ///
    /// The shared contract's zero-group verdict is the product's degradation
    /// rule: when not even one COMPLETE group fits, the lane steps aside and
    /// the dense table answers — no squeezed card, no half group.
    #[must_use]
    pub(crate) fn effective_mode(self, mode: ProcessViewMode) -> ProcessViewMode {
        match mode {
            ProcessViewMode::Cards if self.measured && self.groups == 0 => ProcessViewMode::Dense,
            other => other,
        }
    }

    /// The height one atomic card may take inside an admitted group block.
    #[must_use]
    pub(crate) fn card_height_px(self, metrics: CardFlowMetrics) -> f32 {
        (self.group_height_px - metrics.header_height_px).max(metrics.card_floor_px)
    }

    /// Re-admit one bucket's lower content as WHOLE columns against the slot
    /// that was measured when this budget was resolved.
    ///
    /// A truncated bucket carries an overflow action in its header. The
    /// header is a separate whole group, so its label and action are charged
    /// together by the shared width contract before any card column is spent.
    #[must_use]
    pub(crate) fn width_columns(self, member_count: usize, has_overflow: bool) -> usize {
        let Some(metrics) = self.metrics else {
            return self.columns.min(member_count);
        };
        if !self.width_px.is_finite() || self.width_px <= 0.0 {
            return self.columns.min(member_count);
        }
        let admissible = admit_card_row_width(
            self.width_px,
            self.columns,
            metrics.width_spec(),
            has_overflow,
        )
        .columns();
        admissible.min(member_count)
    }
}

/// Derive Bevy's card metrics from the measured card-flow viewport and
/// spend the shared contract's verdict.
///
/// The measured slot is the flow viewport's own logical size. Bevy
/// contributes its card height, group header height, group gap, and the
/// column count derived from the viewport width; the shared contract answers
/// with the whole-group count, the clamped group height, and the zero-group
/// verdict.
#[must_use]
pub(crate) fn card_flow_budget(
    slot_width_px: f32,
    slot_height_px: f32,
    palette: &UiPalette,
) -> CardFlowBudget {
    let metrics = CardFlowMetrics::from_palette(palette);
    let width_spec = metrics.width_spec();
    let requested_columns = card_row_columns(slot_width_px, width_spec);
    let columns = admit_card_row_width(slot_width_px, requested_columns, width_spec, false)
        .columns()
        .max(1);
    // The measured slot IS the bounded card-flow viewport: the mode row,
    // dashboard, page title and toolbar already spent their height before
    // this node was laid out. Only the flow's own group gap remains.
    let available = (slot_height_px - metrics.group_gap_px).max(0.0);
    let verdict = admit_card_groups(available, columns, metrics.spec());
    CardFlowBudget {
        measured: true,
        groups: verdict.groups,
        group_height_px: verdict.card_height,
        columns: verdict.columns,
        width_px: slot_width_px,
        metrics: Some(metrics),
    }
}

/// The current focus resolves from the exact visible row the table cursor
/// addresses; no OS source and no second projection is read here.
#[must_use]
pub(crate) fn focus_projection(shell: &ShellApp) -> FocusProjection {
    let process = shell.visible_process_at(shell.selected);
    let (column, direction) = shell.process_sort;
    FocusProjection {
        name: process.map(|process| process.name.clone()),
        pid: process.map(|process| process.pid),
        status: process.map(|process| process.status.clone()),
        cpu: process
            .and_then(ProcessItem::current_cpu_percentage)
            .map(taskforest_core::core::percent::precise_percent_text),
        memory: process
            .and_then(ProcessItem::current_memory_bytes)
            .map(bytes),
        visible_count: shell.visible_process_count(),
        query: shell.query.clone(),
        sort_column: sort_token(column).to_owned(),
        sort_descending: direction == SortDir::Desc,
        active_group: shell.process_status_filter,
    }
}

/// The shared dash for an unavailable fact.
fn missing_value() -> String {
    MISSING_VALUE.to_owned()
}

/// Group identity for one provider-owned process state. `All` resets the
/// shell filter through a chip; it is never rendered as a card group.
pub(crate) type CardGroup = ProcessStatusFilter;

/// Classify a provider state without inventing a status.
#[must_use]
pub(crate) fn card_group(status: &str) -> CardGroup {
    ProcessStatusFilter::classify(status)
}

/// Pure plan for the bounded lower card flow. It consumes the already pinned
/// shell `RowPage` and admits only whole groups from the shared verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CardFlowPlan {
    /// The admitted groups, in shell order, with their visible members.
    pub(crate) groups: Vec<CardGroupPlan>,
    /// Whether the visible projection is empty.
    pub(crate) empty: bool,
}

/// One status bucket's admitted card row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CardGroupPlan {
    /// The provider-owned status bucket this group paints.
    pub(crate) group: CardGroup,
    /// The visible-set indices of the admitted cards, in shell order.
    pub(crate) members: Vec<usize>,
    /// How many visible rows the bucket owns in total. The admitted row can
    /// be narrower than the bucket, so the header states the whole count and
    /// never lets a truncated row read as the whole bucket.
    pub(crate) total: usize,
}

/// The card lane's group window.
///
/// The bucket list is folded over the WHOLE visible set, so a bucket is a
/// page-wide fact and never changes identity while the lane scrolls. The
/// window only says which bucket the lane starts at, so every bucket stays
/// reachable and the group headers keep their whole counts.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CardFlowWindow {
    /// The index of the first admitted bucket.
    pub(crate) top: usize,
}

impl CardFlowWindow {
    /// The window clamped to the buckets that actually exist and to the
    /// number of whole groups the measured slot admits.
    #[must_use]
    pub(crate) fn clamped(self, buckets: usize, groups: usize) -> Self {
        let last = buckets.saturating_sub(groups.min(buckets));
        Self {
            top: self.top.min(last),
        }
    }
}

/// Fold ALREADY PROJECTED shell rows into state groups and admit whole
/// groups.
///
/// The card lane reads the shell's VISIBLE rows, not the dense table's
/// viewport window: the two presentations are mutually exclusive, and the
/// dense table's measured window collapses to zero rows while the card lane
/// is on screen. The shell's visible set is the one authority for which
/// processes exist; the dense window is only that presentation's paging.
#[must_use]
pub(crate) fn card_flow_plan(
    shell: &ShellApp,
    budget: CardFlowBudget,
    window: CardFlowWindow,
) -> CardFlowPlan {
    let visible = shell.visible_processes();
    if visible.is_empty() {
        return CardFlowPlan {
            groups: Vec::new(),
            empty: true,
        };
    }
    // One bucket per provider status, in first-appearance order, with the
    // members in shell order. A bucket is a whole group: it is never split
    // across two blocks and never counted twice.
    let mut folded: Vec<(CardGroup, Vec<usize>)> = Vec::new();
    for (index, process) in visible.iter().enumerate() {
        let group = card_group(&process.status);
        match folded.iter_mut().find(|(bucket, _)| *bucket == group) {
            Some((_, members)) => members.push(index),
            None => folded.push((group, vec![index])),
        }
    }
    let start = window.clamped(folded.len(), budget.groups).top;
    let mut groups: Vec<CardGroupPlan> = Vec::new();
    for (group, members) in folded.into_iter().skip(start) {
        if groups.len() >= budget.groups {
            break;
        }
        let has_overflow = members.len() > budget.columns;
        let admitted = members
            .len()
            .min(budget.width_columns(members.len(), has_overflow));
        if admitted == 0 {
            break;
        }
        groups.push(CardGroupPlan {
            group,
            members: members[..admitted].to_vec(),
            total: members.len(),
        });
    }
    CardFlowPlan {
        groups,
        empty: false,
    }
}

/// The bucket index the shell's current selection sits in.
///
/// The lane follows the shell cursor: a selected row that lives in a bucket
/// below the window must still be visible, so the window moves to that
/// bucket instead of leaving the selection behind.
#[must_use]
pub(crate) fn card_bucket_of_selection(shell: &ShellApp) -> Option<usize> {
    let selected = shell.selected;
    let mut folded: Vec<(CardGroup, Vec<usize>)> = Vec::new();
    for (index, process) in shell.visible_processes().iter().enumerate() {
        let group = card_group(&process.status);
        match folded.iter_mut().find(|(bucket, _)| *bucket == group) {
            Some((_, members)) => members.push(index),
            None => folded.push((group, vec![index])),
        }
    }
    folded
        .iter()
        .position(|(_, members)| members.contains(&selected))
}

mod scenes;
pub(crate) use scenes::{card_lane_scene, empty_card_scene, group_scene};
mod runtime;
pub(crate) use runtime::register;
