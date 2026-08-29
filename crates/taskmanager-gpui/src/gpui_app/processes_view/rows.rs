//! Processes view table model: canonical category-tree projection, sortable
//! columns, and the per-row element for the virtualized list.

use crate::gpui_app::elements::{self};
use crate::gpui_app::icons;
use crate::gpui_app::root::{Hover, RootView};
use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement,
    Pixels, StatefulInteractiveElement, Styled, StyledImage, div, px,
};
use std::collections::{HashMap, HashSet};
use taskmanager_application::i18n;
use taskmanager_application::process_category_projection::category_expansion_key;
use taskmanager_shell::ProcessRowId;
use taskmanager_theme::Color;
use taskmanager_ui_contract::{IconId, ProcessColumnSpec};

mod cells;
mod formatting;
mod groups;
use cells::append_body_cells;
use projection::{StructuralArrow, structural_arrow_action};
use taskmanager_shell::SortCol;
use taskmanager_theme::tokens;
pub(crate) mod projection;
pub use projection::{
    ProcRowProps, ProjectionCache, RowCellText, Toggle, VisibleRow, VisibleRowsProps,
    application_root_count, category_tree_rows, default_category_expansions,
    effective_process_hidden_cols, effective_process_sort_col, sort_col_step, sort_id,
    visible_rows, visible_rows_with_local_time, visible_sort_cols,
};

// ── GPUI column kit over the shell `SortCol` ─────────────────────────────────
//
// The shell enum carries the three-frontend superset (including `Pss`).
// GPUI does not surface a PSS column (out of scope for this view), so the
// kit below enumerates only the renderable GPUI columns; the `Pss` arms in
// the exhaustive matches exist for compiler totality and are unreachable
// through any GPUI interaction (the header never offers the column and the
// persistence token parser refuses a `PSS` token).

/// The renderable GPUI columns in canonical header order (matches the Win11
/// TM layout + the `sort_header_row` cell order: Name → User → PID → Threads
/// → Start → Status → CPU → Memory → Swap → Disk read → Disk write → CPU
/// time → FDs → Nice; Swap follows Memory so the two independent memory
/// resources stay adjacent). The "Choose columns" picker and the header
/// arrow-key navigation iterate this order, and it mirrors
/// [`taskmanager_ui_contract::PROCESS_COLUMNS`] position for position — the
/// contract gate test in this module fails if either side drifts.
pub fn columns() -> &'static [SortCol] {
    &[
        SortCol::Name,
        SortCol::User,
        SortCol::Pid,
        SortCol::Threads,
        SortCol::StartTime,
        SortCol::State,
        SortCol::Cpu,
        SortCol::Memory,
        SortCol::Swap,
        SortCol::DiskRead,
        SortCol::DiskWrite,
        SortCol::CpuTime,
        SortCol::Fds,
        SortCol::Nice,
    ]
}

/// Stable token identifying this column in the toolkit-neutral
/// [`taskmanager_ui_contract::PROCESS_COLUMNS`] inventory — spelled exactly
/// like the persisted sort / hidden-columns tokens (`sort_token` /
/// `hidden_tokens`), so persistence, the contract, and the shell enum agree
/// on one string per column. The match is compiler-exhaustive: adding a
/// variant without an arm here is a build error, and the contract gate test
/// below then forces the matching contract row.
pub fn contract_id(col: SortCol) -> &'static str {
    match col {
        SortCol::Name => "Name",
        SortCol::User => "User",
        SortCol::Pid => "PID",
        SortCol::Threads => "Threads",
        SortCol::StartTime => "StartTime",
        SortCol::State => "Status",
        SortCol::Cpu => "CPU",
        SortCol::Memory => "Memory",
        SortCol::Swap => "Swap",
        SortCol::DiskRead => "DiskRead",
        SortCol::DiskWrite => "DiskWrite",
        SortCol::CpuTime => "CPUTime",
        SortCol::Fds => "FDs",
        SortCol::Nice => "Nice",
        // Not a renderable GPUI column (see the module docs); the token keeps
        // the arm exhaustive without ever entering a persisted layout.
        SortCol::Pss => "PSS",
    }
}

/// The neutral contract row for this column. A miss is a programming error
/// (a variant without a contract row); the delegating accessors keep
/// panic-free fallbacks so rendering survives, while the contract gate test
/// fails the suite in CI.
fn contract_spec(col: SortCol) -> Option<&'static ProcessColumnSpec> {
    let spec = taskmanager_ui_contract::find(contract_id(col));
    debug_assert!(
        spec.is_some(),
        "SortCol {col:?} ({}) is missing from PROCESS_COLUMNS",
        contract_id(col)
    );
    spec
}

/// Stable positional index of this column in [`columns`] canonical order
/// (0..14). Used as the per-column element-id suffix for the resize handles
/// so each handle's gpui element state (focus/hitbox) keys uniquely without
/// holding a string table. Derived from the canonical order itself (which
/// the contract gate test pins to `PROCESS_COLUMNS`), so the index can never
/// disagree with [`columns`]; the discriminant fallback is unique per
/// variant and only reachable if [`columns`] is missing a variant.
pub fn column_index(col: SortCol) -> usize {
    let position = columns().iter().position(|&candidate| candidate == col);
    debug_assert!(
        position.is_some(),
        "SortCol {col:?} is missing from columns()"
    );
    position.unwrap_or(col as usize)
}

/// Header display label (matches the `sort_header_row` cell labels exactly, so
/// the picker row reads the same string the user sees above the column).
pub fn header_label(col: SortCol) -> &'static str {
    match col {
        SortCol::Name => i18n::t("common.name"),
        SortCol::User => i18n::t("common.user"),
        SortCol::Pid => i18n::t("proc.pid"),
        SortCol::Threads => i18n::t("common.threads"),
        SortCol::StartTime => i18n::t("proc.start"),
        SortCol::State => i18n::t("common.status"),
        SortCol::Cpu => i18n::t("common.cpu"),
        SortCol::Memory => i18n::t("common.memory"),
        SortCol::Swap => i18n::t("proc.swap"),
        SortCol::DiskRead => i18n::t("proc.disk_read"),
        SortCol::DiskWrite => i18n::t("proc.disk_write"),
        SortCol::CpuTime => i18n::t("proc.cpu_time"),
        SortCol::Fds => i18n::t("proc.fds"),
        SortCol::Nice => i18n::t("proc.nice"),
        // Unreachable (see the module docs): the shell's static label.
        SortCol::Pss => "PSS",
    }
}

/// Whether the "Choose columns" picker may hide this column. Delegates to
/// the neutral `contract_spec` row: `Name` is the identity
/// column (always visible) so it is NOT toggleable — its picker row renders
/// checked + inert, matching Win11 TM / Mission Center. A contract miss
/// (programming error, gate-tested away) falls back to "not hideable" so the
/// column stays visible.
pub fn is_hideable(col: SortCol) -> bool {
    contract_spec(col).is_some_and(|spec| spec.hideable)
}

/// Default pixel width of this column's header + body cell — delegated to
/// the neutral `contract_spec` row, the single toolkit-neutral
/// table of defaults (each value matches the `sort_header_row` /
/// `append_body_cells` `.w(px(X))` so the default render is byte-identical
/// to the pre-resize layout). `Name` is the flexible growable identity
/// column (sized via `flex_grow` between 120–260 px, never `.w(..)`); its
/// default here is its `min_w` floor, returned only so
/// [`crate::gpui_app::root::RootView::proc_col_width`] has a total fallback
/// — it is never applied as a fixed width on the Name cell. A contract miss
/// (programming error, gate-tested away) falls back to the shared 100px
/// default.
pub fn default_width(col: SortCol) -> Pixels {
    contract_spec(col).map_or(px(100.0), |spec| px(spec.default_width))
}

/// Whether this column exposes a drag-resize handle. `Name` is the flexible
/// growable identity column (sized via `flex_grow`, not `.w(..)`), so it is
/// non-resizable; every other column has a fixed default width and a
/// right-edge resize handle. (The non-sortable Trend/sparkline header is not
/// a `SortCol` at all, so it never reaches this gate — it simply gets no
/// handle mounted.)
pub fn is_resizable(col: SortCol) -> bool {
    col != SortCol::Name
}

/// Whether this column holds numeric data and should render RIGHT-aligned in
/// the theme's monospace stack so digits line up vertically tick-to-tick
/// (Win11 TM / Mission Center parity) — delegated to the neutral contract
/// `contract_spec` row. Used by the header (`sort_cell`) and body
/// (`proc_row` via `numeric_cell`) render paths; the text columns (Name /
/// User / Start / Status) stay left-aligned in the UI font. A contract miss
/// (programming error, gate-tested away) falls back to text alignment.
///
/// This is an ALIGNMENT concern only — it is independent of the
/// sort-DIRECTION classification in the shell's `click_sort_column` reducer,
/// where `Pid` is treated as text-like (initial ascending) even though it is
/// numeric here (a PID is a number that right-aligns, but its natural
/// reading is lexicographic so it sorts asc).
pub fn is_numeric(col: SortCol) -> bool {
    contract_spec(col).is_some_and(|spec| spec.numeric)
}

/// Width reserved for the fixed process identity column in column-navigation
/// mode. It scales with the viewport so the name remains useful on a wide
/// window without stealing the entire compact table.
#[must_use]
pub fn process_name_band_width(viewport: Pixels) -> Pixels {
    (viewport * 0.24).clamp(px(180.0), px(280.0))
}

/// Intrinsic width of the process table's complete enabled column set. The
/// name column remains the identity anchor, while CPU owns an adjacent trend
/// band; every other enabled column contributes its live resized width. The
/// returned width is never smaller than the viewport, so a compact window
/// gets a real horizontal range instead of silently shrinking numeric cells.
#[must_use]
pub fn process_table_content_width(
    hidden_cols: &HashSet<SortCol>,
    viewport: Pixels,
    col_widths: &HashMap<SortCol, Pixels>,
) -> Pixels {
    let name = process_name_band_width(viewport);
    let right = visible_sort_cols(hidden_cols)
        .into_iter()
        .filter(|col| *col != SortCol::Name)
        .map(|col| process_band_column_width(col, col_widths))
        .fold(px(0.0), |sum, width| sum + width);
    (name + right + px(tokens::SPACE_16.0)).max(viewport)
}

fn process_band_column_width(col: SortCol, col_widths: &HashMap<SortCol, Pixels>) -> Pixels {
    let base = col_widths
        .get(&col)
        .copied()
        .unwrap_or_else(|| default_width(col));
    if col == SortCol::Cpu {
        base + px(56.0)
    } else {
        base
    }
}

/// Return the fixed-name plus visible-right-column band for the Apps table.
/// The active column keeps up to two preceding columns as context; when the
/// whole right side fits, no columns are hidden. This is a pure layout rule so
/// render and headless keyboard tests use the same projection.
#[must_use]
pub fn process_column_band(
    hidden_cols: &HashSet<SortCol>,
    cursor: SortCol,
    viewport: Pixels,
    col_widths: &HashMap<SortCol, Pixels>,
) -> Vec<SortCol> {
    let visible = visible_sort_cols(hidden_cols);
    if visible.len() <= 1 {
        return visible;
    }

    let name_width = process_name_band_width(viewport);
    let right_width = (viewport - name_width).max(px(160.0));
    let right_total = visible
        .iter()
        .skip(1)
        .map(|col| process_band_column_width(*col, col_widths))
        .fold(px(0.0), |sum, width| sum + width);
    if right_total <= right_width {
        return visible;
    }

    let cursor_ix = visible.iter().position(|col| *col == cursor).unwrap_or(0);
    let mut start = cursor_ix.saturating_sub(2).max(1);
    let mut end = start;
    let mut used = px(0.0);
    while end < visible.len() {
        let width = process_band_column_width(visible[end], col_widths);
        if end > start && used + width > right_width {
            break;
        }
        used += width;
        end += 1;
    }

    // A very wide active column must still be visible even when it is wider
    // than the available right-hand viewport.
    if cursor_ix >= end {
        start = cursor_ix.max(1);
        end = (start + 1).min(visible.len());
    }

    let mut band = Vec::with_capacity(end.saturating_sub(start) + 1);
    band.push(SortCol::Name);
    band.extend(visible[start..end].iter().copied());
    band
}

/// Move the Apps column cursor without wrapping. Wrapping would make a
/// compact table jump from the far-right metric back to the first metric and
/// hide the fact that the fixed name column is the navigation anchor.
#[must_use]
pub fn process_column_step(
    cursor: SortCol,
    right: bool,
    hidden_cols: &HashSet<SortCol>,
) -> SortCol {
    let visible = visible_sort_cols(hidden_cols);
    let Some(current) = visible.iter().position(|col| *col == cursor) else {
        return visible.first().copied().unwrap_or(SortCol::Name);
    };
    let next = if right {
        (current + 1).min(visible.len().saturating_sub(1))
    } else {
        current.saturating_sub(1)
    };
    visible.get(next).copied().unwrap_or(SortCol::Name)
}

/// Apply the single structural expansion policy used by the chevron, pointer double-click,
/// and keyboard directional paths. Group membership means "expanded" while tree membership
/// means "collapsed", so keeping this translation here prevents the three entry points from
/// drifting as the row projection evolves.
fn set_expansion(view: &mut RootView, toggle: &Toggle, expanded: bool) {
    match toggle {
        Toggle::TreePid(pid) => {
            if expanded {
                view.processes_state.collapsed.remove(pid);
            } else {
                view.processes_state.collapsed.insert(*pid);
            }
        }
        Toggle::GroupApp(name) => {
            if expanded {
                view.processes_state.expanded_apps.insert(name.clone());
            } else {
                view.processes_state.expanded_apps.remove(name);
            }
        }
        // Category headers share the hierarchy expansion set with application
        // roots. Their prefixed stable key can never collide with an app root.
        Toggle::GroupCategory(category) => {
            let key = category_expansion_key(*category);
            if expanded {
                view.processes_state.expanded_apps.insert(key);
            } else {
                view.processes_state.expanded_apps.remove(&key);
            }
        }
        Toggle::None => {}
    }
}

fn toggle_expansion(view: &mut RootView, toggle: &Toggle) {
    let expanded = match toggle {
        Toggle::TreePid(pid) => !view.processes_state.collapsed.contains(pid),
        Toggle::GroupApp(name) => view.processes_state.expanded_apps.contains(name),
        Toggle::GroupCategory(category) => view
            .processes_state
            .expanded_apps
            .contains(&category_expansion_key(*category)),
        Toggle::None => return,
    };
    set_expansion(view, toggle, !expanded);
}

/// Header-click semantics: clicking the already-active column toggles asc/desc; clicking
/// a new column makes it active with the conventional initial direction (asc for text,
/// desc for numerics). Pure function of the current `(col, asc)` — the caller writes the
/// returned pair back into the `RootView` fields (`processes.sort_col` /
/// `processes.sort_asc`).
pub fn proc_row(
    props: ProcRowProps<'_>,
    hidden_cols: &HashSet<SortCol>,
    col_widths: &HashMap<SortCol, Pixels>,
) -> AnyElement {
    proc_row_with_layout(
        props,
        hidden_cols,
        col_widths,
        process_name_band_width(px(640.0)),
    )
}

/// Render one process row with the shared live-width column layout. The row
/// itself is laid out once inside the table's intrinsic content surface; the
/// surrounding header/body viewport performs horizontal translation without
/// asking this row projection to rebuild for every pointer position.
pub fn proc_row_with_layout(
    props: ProcRowProps<'_>,
    // Columns the user has hidden via the "Choose columns" picker. Membership =
    // "skip this column's cell". `Name` is never in here (the picker refuses to
    // toggle it — see `SortCol::is_hideable`), so the identity cell always renders.
    hidden_cols: &HashSet<SortCol>,
    // Live column widths (user overrides merged with defaults by the caller).
    // Body cells read the SAME width as the header so header + body stay
    // pixel-aligned after a drag (see `live_col_width`).
    col_widths: &HashMap<SortCol, Pixels>,
    name_width: Pixels,
) -> AnyElement {
    let ProcRowProps {
        theme,
        row,
        row_idx,
        is_sel,
        is_hov,
        entity,
        pids,
        row_keys,
        rows,
        gray_zero_values,
        density,
        ui_size,
    } = props;
    // Row background — subtle + theme-aware (Win11 TM parity): selected rows get
    // a soft accent tint, hovered (non-selected) rows a fainter accent tint, and
    // odd visible rows get a barely-visible neutral zebra for readable scanning.
    // All three come from the theme's derived surface tokens (`selection_bg` /
    // `hover_bg` / `zebra_bg`) so they adapt to every skin / light-dark /
    // high-contrast variant and stay in lockstep with the rest of the app. The
    // keyboard focus ring (`elements::focus_ring` below) paints additively on
    // top of this bg (an outset box-shadow — no layout perturbation), so the
    // ring stays visible over a selected/hovered row.
    let bg = if is_sel {
        theme.selection_bg()
    } else if is_hov {
        theme.hover_bg()
    } else if row_idx % 2 == 1 {
        theme.zebra_bg()
    } else {
        Color::TRANSPARENT
    };
    let process_pid = row.process_pid;
    let selection_key = row.selection_key;
    let ent_click = entity.clone();
    let ent_hover = entity.clone();
    let ent_toggle = entity.clone();
    let ent_key = entity.clone();
    // Keyboard-nav capture: the ordered PID list (ArrowUp/Down moves selection by
    // index) + this row's tree/group affordance (bare ArrowLeft/Right run the
    // iced-parity tree matrix: collapse / expand / climb to the parent; leaf
    // rows and Alt/Shift keep the column-cursor stepping).
    // Each handler reads the LIVE `selected_process_row` from RootView, so
    // consecutive arrow presses keep advancing even though gpui focus stays on
    // the originally-clicked row.
    let key_pids = pids;
    let click_pids = key_pids.clone();
    let nav_rows = row_keys;
    // Full projection snapshot for the bare Left/Right structural resolver:
    // the acted-on row is the LIVE selection (focus and selection diverge
    // after Home/End/PageUp), so the handler needs to find it among all rows,
    // not just this focused one. Owned `Rc` — the same per-row clone cost the
    // `pids`/`nav_rows` captures already pay.
    let all_rows = rows;
    let key_has_children = row.has_children;
    let key_collapsed = row.collapsed;
    let key_parent_key = row.parent_key;
    let key_toggle = row.toggle.clone();
    let double_click_toggle = row.toggle.clone();
    let double_click_enabled = row.has_children;

    // Build the Name cell: depth indent + (chevron | spacer) + name + optional badge.
    // Width mirrors the Name header cell (`sort_header_row`): flex_grow between 120–260
    // so the column contracts at narrow windows instead of overflowing into User.
    let mut name_cell = div()
        .min_w(px(120.0))
        .flex()
        .items_center()
        // Same inner gutter every other column carries (row `.px(SPACE_8)` +
        // cell `.pl(SPACE_8)`), so the identity column's text does not sit
        // flush against the table edge while User/PID sit 16px in.
        .pl(tokens::SPACE_8)
        .text_size(ui_size.body_font_size())
        .text_color(theme.fg)
        // Indentation by depth (tree children / group instances). flex_shrink_0
        // holds the indent fixed — without it gpui's default flex_shrink=1
        // collapses the spacer when the name cell overflows (deep nesting / a
        // narrow window), so children of different parents end up at the same
        // indent and the tree hierarchy visually flattens.
        .child(div().w(px(row.depth as f32 * 14.0)).flex_shrink_0());
    name_cell = name_cell.w(name_width).flex_shrink_0();

    // Chevron for expandable rows; a fixed-width spacer for non-expandable rows in
    // tree/group modes so sibling names stay column-aligned.
    if row.has_children {
        let glyph = if row.collapsed {
            "\u{25B8}"
        } else {
            "\u{25BE}"
        }; // ▸ / ▾
        let toggle = row.toggle.clone();
        name_cell = name_cell.child(
            div()
                .id(("expand", row_idx))
                .w(px(18.0))
                .flex_shrink_0()
                .cursor_pointer()
                .text_color(theme.fg_dim)
                // stop_propagation keeps the parent row's selection on_click from firing
                // when the chevron is the click target.
                .on_mouse_down(MouseButton::Left, move |_ev, _win, cx: &mut App| {
                    cx.stop_propagation();
                    ent_toggle.update(cx, |v, cx| {
                        toggle_expansion(v, &toggle);
                        cx.notify();
                    });
                })
                .child(glyph.to_string()),
        );
    } else {
        name_cell = name_cell.child(div().w(px(18.0)).flex_shrink_0());
    }

    // A verified desktop identity renders its provider-resolved asset when one
    // exists; a token without validated bytes keeps the honest generic glyph.
    if let Some(identity) = &row.application_identity {
        #[cfg(any(test, feature = "test-support"))]
        let has_asset = identity.icon_asset.is_some();
        let app_icon_size: Pixels = ui_size.icon_size().into();
        let icon = identity.icon_asset.as_ref().map_or_else(
            || {
                taskmanager_icons::icon(IconId::Applications)
                    .size(app_icon_size)
                    .into_any_element()
            },
            |asset| {
                icons::application_image(asset)
                    .size(app_icon_size)
                    .with_fallback(move || {
                        taskmanager_icons::icon(IconId::Applications)
                            .size(app_icon_size)
                            .into_any_element()
                    })
                    .into_any_element()
            },
        );
        let marker = div()
            .w(ui_size.icon_size())
            .h(ui_size.icon_size())
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .mr(tokens::SPACE_4)
            .child(icon);
        #[cfg(any(test, feature = "test-support"))]
        let marker = marker.debug_selector(move || {
            if has_asset {
                "tm-proc-app-image".to_string()
            } else {
                "tm-proc-app-icon".to_string()
            }
        });
        name_cell = name_cell.child(marker);
    }

    // `.flex_1().min_w(px(0.0)).truncate()` bounds the text to the name_cell's remaining
    // width (after indent + chevron) so long names ellipsis instead of spilling into User.
    // The highlight ranges arrive precomputed from the visible-row projection
    // (`VisibleRow::name_highlights`): a repaint never re-runs the match engine.
    name_cell = name_cell.child(div().flex_1().min_w(px(0.0)).truncate().child(
        crate::gpui_app::elements::highlighted_text_with_ranges(
            &row.name,
            &row.name_highlights,
            theme,
        ),
    ));
    if let Some(b) = &row.badge {
        name_cell = name_cell.child(
            div()
                .ml(tokens::SPACE_8)
                .text_size(ui_size.caption_font_size())
                .text_color(theme.fg_dim)
                .child(b.clone()),
        );
    }

    #[cfg(any(test, feature = "test-support"))]
    let name_cell = name_cell.debug_selector(|| "tm-proc-b-name".to_string());

    // `row_idx` (not pid) keys the row id so group mode can't produce duplicate ids when
    // an aggregate row and one of its instances share the main PID.
    let line = div()
        .id(("proc", row_idx))
        .on_click(move |ev, _win, cx: &mut App| {
            ent_click.update(cx, |v, cx| {
                let modifiers = ev.modifiers();
                if let Some(pid) = process_pid {
                    if modifiers.shift {
                        // The shell-owned selection resolves the anchor→end span
                        // against the live display-order pid projection.
                        v.extend_process_selection(&click_pids, pid);
                    } else if modifiers.control || modifiers.platform {
                        v.toggle_process_selection(pid);
                    } else {
                        v.select_process_single(pid);
                    }
                } else if let Some(ProcessRowId::Application(root)) = selection_key {
                    v.select_application_root(root.pid());
                }
                // Mission Center treats a primary double-click on an aggregate/tree
                // row as the same expand/collapse operation as its chevron. Keep the
                // selection update and the structural projection in one row event so
                // the cached render + keyboard projection observe the same state.
                if double_click_enabled && ev.standard_click() && ev.click_count() == 2 {
                    toggle_expansion(v, &double_click_toggle);
                }
                cx.notify();
            });
        })
        .on_hover(move |is_hov: &bool, _win, cx: &mut App| {
            ent_hover.update(cx, |v, cx| {
                v.set_hover(
                    if *is_hov {
                        process_pid.map(Hover::Proc)
                    } else {
                        None
                    },
                    cx,
                );
            });
        })
        // Focusable so clicking a row routes keyboard focus here (the row's own
        // on_key_down then handles ArrowUp/Down/Left/Right). gpui 0.2.2 lays uniform_list
        // rows out as independent roots, so a key handler on the list itself wouldn't
        // reliably receive bubbled row key events — hence per-row handlers.
        //
        // WCAG 2.4.7: .focus(elements::focus_ring(theme)) paints the 2px accent outset
        // ring while the row holds focus. Additive to the selection/hover bg above (an
        // outset box-shadow, no layout perturbation) — mirrors root/chrome.rs.
        .focusable()
        // Roving row tab stop: one entry reaches the virtualized list, while
        // ArrowUp/Down retains efficient navigation across thousands of rows.
        .tab_stop(is_sel || row_idx == 0)
        .key_context("ProcessList")
        .focus(elements::focus_ring(theme))
        .on_key_down(move |ev: &KeyDownEvent, _win, cx: &mut App| {
            let key = ev.keystroke.key.as_str();
            match key {
                "down" | "up" => {
                    let down = key == "down";
                    // Bare arrow collapses to the focused row (standard list
                    // semantics); Ctrl/Shift preserves an existing multi
                    // selection so the user can roam across a batch without
                    // losing it.
                    let preserve = ev.keystroke.modifiers.control || ev.keystroke.modifiers.shift;
                    ent_key.update(cx, |v, cx| {
                        let n = nav_rows.len();
                        if n == 0 {
                            return;
                        }
                        // Advance from the LIVE selection (not this row's static index),
                        // so repeated presses keep moving past the focused row.
                        let next = match v.selected_process_row().and_then(|active| {
                            nav_rows.iter().position(|candidate| *candidate == active)
                        }) {
                            Some(cur) => {
                                if down {
                                    (cur + 1).min(n - 1)
                                } else {
                                    cur.saturating_sub(1)
                                }
                            }
                            // Nothing selected (or selection filtered out): Down→first, Up→last.
                            None => {
                                if down {
                                    0
                                } else {
                                    n - 1
                                }
                            }
                        };
                        // Bare arrow collapses to the focused row; Ctrl/Shift
                        // preserves the multi-selection (shell-owned rule).
                        v.move_process_row_selection(nav_rows.get(next).copied(), preserve);
                        cx.notify();
                    });
                    // Arrow keys are also bound on the root key handler
                    // (`MoveSelection(Next|Previous)` via the typed command
                    // router). Stop propagation so a focused row doesn't both
                    // advance here AND bubble to the root, which would
                    // double-move the selection by one row each press.
                    cx.stop_propagation();
                }
                "left" | "right" => {
                    let right = key == "right";
                    // Alt/Shift reserves the keys for column navigation on
                    // every row, structural or leaf.
                    let column_modifier =
                        ev.keystroke.modifiers.alt || ev.keystroke.modifiers.shift;
                    if column_modifier {
                        ent_key.update(cx, |v, cx| {
                            v.processes_state.column_cursor = process_column_step(
                                v.processes_state.column_cursor,
                                right,
                                &v.processes_state.hidden_cols,
                            );
                            cx.notify();
                        });
                        cx.stop_propagation();
                    } else {
                        // Bare structural key. The acted-on row is the LIVE
                        // selection — focus and selection diverge after the
                        // root router moves it (Home/End/PageUp) — resolved
                        // through the shared projection; with nothing
                        // selected the focused row keeps the historical
                        // behavior. The decision itself is the pure
                        // `structural_arrow_action` fold (iced-parity tree
                        // matrix), so the renderer only executes it.
                        ent_key.update(cx, |v, cx| {
                            let target = v.selected_process_row().and_then(|active| {
                                all_rows.iter().find(|r| r.selection_key == Some(active))
                            });
                            let (has_children, collapsed, parent_key, toggle) = match target {
                                Some(row) => {
                                    (row.has_children, row.collapsed, row.parent_key, &row.toggle)
                                }
                                None => {
                                    (key_has_children, key_collapsed, key_parent_key, &key_toggle)
                                }
                            };
                            if !has_children {
                                // Leaf rows keep the historical bare-key
                                // meaning: step the header column cursor
                                // without touching sort or selection.
                                v.processes_state.column_cursor = process_column_step(
                                    v.processes_state.column_cursor,
                                    right,
                                    &v.processes_state.hidden_cols,
                                );
                                cx.notify();
                            } else {
                                match structural_arrow_action(collapsed, parent_key, right) {
                                    Some(StructuralArrow::Collapse) => {
                                        set_expansion(v, toggle, false);
                                        cx.notify();
                                    }
                                    Some(StructuralArrow::Expand) => {
                                        set_expansion(v, toggle, true);
                                        cx.notify();
                                    }
                                    Some(StructuralArrow::GotoParent(parent)) => {
                                        // Bare key collapses the multi-select
                                        // set onto the parent — the same rule
                                        // the Up/Down mover applies.
                                        v.move_process_row_selection(Some(parent), false);
                                        cx.notify();
                                    }
                                    // Right on an expanded row / Left with no
                                    // selectable ancestor: honest no-op, no
                                    // repaint.
                                    None => {}
                                }
                            }
                        });
                        cx.stop_propagation();
                    }
                }
                _ => {}
            }
        })
        .flex()
        .items_center()
        // Fill the uniform_list's allocated width so the body row is exactly as
        // wide as the header row (which stretches to the scroll container).
        // Without this the body sizes to its content while the header stretches
        // to the viewport, and every column diverges.
        .w_full()
        // The row is the positioning context for the selection rail (an
        // absolutely-positioned leading-edge accent bar, see below).
        .relative()
        .px(tokens::SPACE_8)
        .py(density.row_padding_y())
        .line_height(density.line_height())
        .rounded(tokens::small_radius(theme))
        .bg(bg);
    // ── Selection rail ─────────────────────────────────────────────────────
    // Selected rows carry a 4px accent rail on the leading edge (Win11 TM /
    // Mission Center parity) as the PRIMARY selection identity; the row's
    // translucent `selection_bg` tint backs it up. The rail is an absolute
    // child (no layout perturbation): it paints beneath the row content, and
    // its outer corners follow the row's own radius so it never overhangs the
    // rounded corner. Non-selected rows get no rail (nothing to misalign).
    let rail = div()
        .absolute()
        .left_0()
        .top_0()
        .bottom_0()
        .w(tokens::SELECTION_RAIL)
        .rounded_tl(tokens::small_radius(theme))
        .rounded_bl(tokens::small_radius(theme))
        .bg(theme.accent);
    #[cfg(any(test, feature = "test-support"))]
    let rail = rail.debug_selector(|| "tm-proc-rail".to_string());
    let line = if is_sel { line.child(rail) } else { line };
    // ── Column cells ────────────────────────────────────────────────────────
    // Every cell after Name is gated by `hidden_cols` so the "Choose columns"
    // picker drops header AND body in lockstep (see sort_header_row's matching
    // `if` gates). Name is the identity column — always rendered. Built
    // incrementally via `if` + reassign (no FluentBuilder import); `.child()`
    // preserves the `Stateful<Div>` type so the trailing `.context_menu(..)`
    // (which requires ParentElement + Styled) still type-checks.
    let line = line.child(name_cell);
    // Geometry breakpoint on the final row root (before cells attach) — the
    // render-path assertion looks this up to prove rows paint.
    #[cfg(any(test, feature = "test-support"))]
    let line = line.debug_selector(move || format!("tm-proc-row-root:{row_idx}"));
    append_body_cells(
        line,
        cells::AppendBodyCellsProps {
            theme,
            row,
            row_idx,
            pid: process_pid,
            hidden_cols,
            col_widths,
            entity,
            gray_zero_values,
            ui_size,
        },
    )
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_processes_view_rows_tests.rs"]
mod tests;
