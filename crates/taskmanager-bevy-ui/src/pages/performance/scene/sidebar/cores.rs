//! The per-core CPU grid: heterogeneous class groups, socket envelopes, the
//! responsive column policy, and the per-core utilization bars.
//!
//! One authority per fact. The shell's CPU projection owns the per-core
//! utilization, the per-logical-CPU class inventory, and the package
//! (socket) inventory; this module only projects them into the grid's
//! shape. A missing inventory stays a flat grid — a class or a socket is
//! never guessed from absent data.
//!
//! The grid is a *measured* layout: the cell width follows the row's
//! measured width (the column policy folds the wide four-column grid to two
//! columns when the row cannot afford four readable cells), and every cell
//! keeps an independent label slot and value slot so a narrow row can never
//! squeeze the core number into the percentage.

use crate::app::ShellTrack;
use crate::drain::ShellProjectionFolded;
use crate::pages::alerts::page_observer;
use crate::pages::performance::metrics::cpu::{
    CoreClassGroup, CoreSocketSection, CpuField, core_socket_sections, cpu_field_text,
};
use crate::pages::performance::metrics::{core_usage_count, core_usage_fill_pct, core_usage_pct};
use crate::pages::performance::{DynBar, DynField, DynText};
use crate::palette::{UiPalette, space_1, space_2, space_4, space_8, space_12};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole, WindowPalette};
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::Without;
use bevy::ecs::system::{Query, Res, SystemParam};
use bevy::scene::{EntityScene, Scene, bsn, on};
use bevy::text::{LineBreak, TextLayout};
use bevy::time::Time;
use bevy::ui::BorderColor;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, ComputedNode, FlexDirection, FlexWrap,
    JustifyContent, Node, Overflow, UiRect, Val, percent, px, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_core::core::hardware::CpuType;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::missing_value;

/// The widest column count the per-core grid lays out.
pub(crate) const CORE_GRID_MAX_COLUMNS: usize = 4;
/// The compact column count the grid folds to when the measured row width
/// cannot afford the wide grid at a readable cell width.
pub(crate) const CORE_GRID_COMPACT_COLUMNS: usize = 2;
/// The utilization band above which a core bar warns (amber ink).
pub(crate) const CORE_BAR_WARN_PCT: f32 = 80.0;
/// The utilization band above which a core bar alarms (red ink).
pub(crate) const CORE_BAR_DANGER_PCT: f32 = 95.0;
/// The seconds the collapse/expand height easing runs.
pub(crate) const CORE_GRID_REVEAL_SECONDS: f32 = 0.18;

/// The row container that holds one group's cells: the node whose measured
/// width drives the column policy.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct CoreGridRow;

/// One per-core cell: the identity, the bar, and the value slots.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct CoreCell(pub(crate) usize);

/// One class section: the typed class caption and the core count it owns.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct CoreClassSection {
    /// The observed class the section groups.
    pub(crate) class: CpuType,
    /// The number of cores the class owns.
    pub(crate) count: usize,
}

/// One socket envelope: the physical package the section's cores belong to.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct CoreSocketEnvelope {
    /// The observed package index.
    pub(crate) socket_id: u32,
    /// The NUMA node the package reported, when the platform exposed one.
    pub(crate) numa_node: Option<u32>,
}

/// The identity slot of one cell: the core's number. It owns its own width
/// so a narrow row can never squeeze the number into the percentage.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct CoreLabelSlot;

/// The value slot of one cell: the core's utilization percentage. It owns
/// its own width, independent of the identity slot.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct CoreValueSlot;

/// The fill node of one per-core bar: the live utilization magnitude. The
/// fill's ink follows the same observation the bar's width does.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct CoreBarFill(pub(crate) usize);

/// The collapse/expand intent the "show all logical cores" toggle owns.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct CoreGridToggle {
    /// Whether the grid is currently expanded.
    pub(crate) expanded: bool,
}

/// The eased height transition the grid body runs through. The transition
/// is anchored to the clock, not to a step count: whichever driver advances
/// it — the fold observer or a per-frame animator — reads the same eased
/// height for the same instant.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CoreGridTransition {
    /// The height the transition starts from, in rem.
    pub(crate) from: f32,
    /// The height the transition ends at, in rem.
    pub(crate) to: f32,
    /// The clock instant the transition started at, in seconds.
    pub(crate) started_at: f32,
}

impl CoreGridTransition {
    /// The eased progress at `now` (seconds): a monotone ease-out over the
    /// reveal duration, clamped to the settled end state.
    pub(crate) fn progress(&self, now: f32) -> f32 {
        let elapsed = now - self.started_at;
        if elapsed >= CORE_GRID_REVEAL_SECONDS {
            return 1.0;
        }
        let ratio = (elapsed / CORE_GRID_REVEAL_SECONDS).clamp(0.0, 1.0);
        1.0 - (1.0 - ratio).powi(3)
    }

    /// The height the body shows at `now`, in rem.
    pub(crate) fn value(&self, now: f32) -> f32 {
        self.from + (self.to - self.from) * self.progress(now)
    }
}

/// The grid body's reveal state: the transition it eases through.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct CoreGridReveal {
    /// The transition the body eases through.
    pub(crate) transition: CoreGridTransition,
    /// Whether the body's natural height has been measured yet.
    pub(crate) measured: bool,
    /// The open grid's natural height, in rem, captured while the body is
    /// OPEN.
    ///
    /// While the body is collapsed the content is measured INSIDE the
    /// collapsed body, so re-measuring the target every frame would shrink it
    /// towards zero and the reopen would never reach the real height. The
    /// natural height is therefore captured once, while the body is open, and
    /// kept through the collapse (the same rule the storage surface follows).
    pub(crate) natural_height: Option<f32>,
}

/// The grid body's content: the measured natural height of the open grid.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct CoreGridContent;

/// The label slot's width, in rem: the widest core caption ("Core 01 (P)")
/// plus its breathing room.
fn core_label_slot_rem(palette: &UiPalette) -> f32 {
    palette.control_height_rem * 2.0
}

/// The value slot's width, in rem: the widest utilization readout ("100.0%")
/// plus its breathing room.
fn core_value_slot_rem(palette: &UiPalette) -> f32 {
    palette.control_height_rem * 1.6
}

/// The bar's height, in rem: a hairline track that rides the reading size.
fn core_bar_height_rem(palette: &UiPalette) -> f32 {
    palette.control_height_rem * 0.16
}

/// The narrowest width one cell may take: below this the identity slot and
/// the value slot would collide.
fn core_cell_floor_rem(palette: &UiPalette) -> f32 {
    palette.control_height_rem * 4.5
}

/// The width one cell needs to carry its identity slot, its bar, and its
/// value slot without truncating the core number.
fn core_cell_readable_rem(palette: &UiPalette) -> f32 {
    palette.control_height_rem * 4.9
}

/// The column policy: the wide grid while the measured row can afford
/// `cell_count` readable cells, the compact grid while it can still afford
/// two cells at their floor width, and a single column when even that does
/// not fit. The count never exceeds the cells the row actually holds.
pub(crate) fn core_grid_columns(
    cell_count: usize,
    available_px: f32,
    readable_px: f32,
    floor_px: f32,
) -> usize {
    let base = cell_count.clamp(1, CORE_GRID_MAX_COLUMNS);
    if base <= CORE_GRID_COMPACT_COLUMNS {
        return base;
    }
    let gap = space_8();
    if available_px >= readable_px * base as f32 + gap * (base - 1) as f32 {
        return base;
    }
    if available_px >= floor_px * CORE_GRID_COMPACT_COLUMNS as f32 + gap {
        return CORE_GRID_COMPACT_COLUMNS;
    }
    1
}

/// The cell width, in percent, for a column count: the columns share the
/// row, minus the slack the inter-cell gap needs.
pub(crate) fn core_cell_width_percent(columns: usize) -> f32 {
    (100.0 / columns.max(1) as f32) - 2.0
}

/// The ink of one core bar: the accent while the core is calm, the warning
/// ink above the warn band, the danger ink above the alarm band. An
/// unobserved core keeps the calm ink — a missing fact is never painted as
/// a warning.
pub(crate) fn core_bar_ink(usage_pct: Option<f32>, palette: &UiPalette) -> bevy::color::Color {
    let Some(pct) = usage_pct.filter(|value| value.is_finite()) else {
        return palette.accent;
    };
    let pct = pct.clamp(0.0, 100.0);
    if pct > CORE_BAR_DANGER_PCT {
        palette.danger_color
    } else if pct > CORE_BAR_WARN_PCT {
        palette.warning_color
    } else {
        palette.accent
    }
}

/// The identity caption of one core: its number plus the observed class
/// suffix.
fn core_label(shell: &ShellApp, index: usize) -> String {
    let suffix = shell
        .projection()
        .hardware
        .as_ref()
        .and_then(|hardware| hardware.cpu_types.get(index).copied())
        .map_or("", |class| match class {
            CpuType::Performance => " (P)",
            CpuType::Efficient => " (E)",
            CpuType::LowPower => " (LP)",
            _ => "",
        });
    format!("Core {:02}{suffix}", index + 1)
}

/// One per-core cell: the identity slot, the bounded bar, the value slot,
/// and the micro subtitle that carries the core's frequency and
/// temperature.
fn core_cell_scene(shell: &ShellApp, index: usize, palette: &UiPalette) -> impl Scene + use<> {
    let label = core_label(shell, index);
    let usage_field = CpuField::CoreUsage(index);
    let telemetry_field = CpuField::CoreTelemetry(index);
    let bar_field = CpuField::Core(index);
    let fill_pct = core_usage_fill_pct(shell, index);
    let fill_ink = core_bar_ink(core_usage_pct(shell, index), palette);
    bsn! {
        Node {
            width: percent(core_cell_width_percent(CORE_GRID_COMPACT_COLUMNS)),
            min_width: rem(core_cell_floor_rem(palette)),
            flex_grow: 1.0,
            flex_shrink: 1.0,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        CoreCell(index)
        Children [

                Node {
                    width: percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_8()),
                }
                Children [

                        Node {
                            width: rem(core_label_slot_rem(palette)),
                            flex_shrink: 0.0,
                            overflow: Overflow::clip_x(),
                        }
                        CoreLabelSlot
                        Children [
                             Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
                        ]
                    --

                        Node {
                            flex_grow: 1.0,
                            min_width: px(0.0),
                            height: rem(core_bar_height_rem(palette)),
                            border_radius: BorderRadius::all(Val::Px(space_2())),
                            overflow: Overflow::clip_x(),
                        }
                        BackgroundColor({ palette.nav_active_bg })
                        Children [

                                Node {
                                    width: percent(fill_pct),
                                    height: percent(100.0),
                                    border_radius: BorderRadius::all(Val::Px(space_2())),
                                }
                                BackgroundColor({ fill_ink })
                                CoreBarFill(index)
                                DynBar(bar_field)
                        ]
                    --

                        Node {
                            width: rem(core_value_slot_rem(palette)),
                            flex_shrink: 0.0,
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::FlexEnd,
                            overflow: Overflow::clip_x(),
                        }
                        CoreValueSlot
                        Children [

                                Text(cpu_field_text(shell, usage_field))
                                TextRole(Role::Mono)
                                DynText(DynField::Cpu(usage_field))
                                TextLayout { linebreak: LineBreak::NoWrap }

                        ]

                ]
            --
                Node {
                    width: percent(100.0),
                    min_width: px(0.0),
                    overflow: Overflow::clip_x(),
                }
                Children [

                        Text(cpu_field_text(shell, telemetry_field))
                        TextRole(Role::Caption)
                        DynText(DynField::Cpu(telemetry_field))
                        TextLayout { linebreak: LineBreak::NoWrap }

                ]

        ]
    }
}

/// One class section: the typed class caption with its core count, and the
/// row of cells the class owns.
fn core_class_section_scene(
    shell: &ShellApp,
    group: &CoreClassGroup,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let cells: Vec<Box<dyn Scene>> = group
        .indices
        .iter()
        .map(|&index| Box::new(core_cell_scene(shell, index, palette)) as Box<dyn Scene>)
        .collect();
    let caption = t(group.label_key()).to_owned();
    let count = group.indices.len().to_string();
    let class = group.class;
    let section_count = group.indices.len();
    bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        CoreClassSection { class, count: section_count }
        Children [

                Node {
                    width: percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_8()),
                    overflow: Overflow::clip_x(),
                }
                Children [
                     Text(caption) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } --
                     Text(count) TextRole(Role::Caption)
                ]
            --
                Node {
                    width: percent(100.0),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(space_8()),
                    row_gap: Val::Px(space_4()),
                }
                CoreGridRow
                Children [
                    { cells }
                ]

        ]
    }
}

/// One socket envelope: the physical package's caption and the class
/// sections its cores form. The envelope is only drawn for a package the
/// inventory actually reported; a host without a package inventory keeps
/// the classes without an envelope.
fn core_socket_scene(
    shell: &ShellApp,
    section: &CoreSocketSection,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let groups: Vec<Box<dyn Scene>> = section
        .classes
        .iter()
        .map(|group| Box::new(core_class_section_scene(shell, group, palette)) as Box<dyn Scene>)
        .collect();
    let socket = section.socket_id;
    let numa_node = section.numa_node;
    let header: Vec<Box<dyn Scene>> = socket
        .map(|socket_id| format!("{} {socket_id}", t("common.sockets")))
        .into_iter()
        .chain(numa_node.map(|node| format!("NUMA {node}")))
        .map(|line| {
            Box::new(bsn! {
                Text({ line })
                TextRole(Role::Caption)
                TextLayout { linebreak: LineBreak::NoWrap }
            }) as Box<dyn Scene>
        })
        .collect();
    match socket {
        Some(socket_id) => {
            let border = palette.border_color;
            Box::new(bsn! {
                Node {
                    width: percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_4()),
                    padding: UiRect::all(Val::Px(space_8())),
                    border: UiRect::all(Val::Px(space_1())),
                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                }
                BorderColor { top: border, right: border, bottom: border, left: border }
                CoreSocketEnvelope { socket_id, numa_node }
                Children [

                        Node {
                            width: percent(100.0),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(space_8()),
                            overflow: Overflow::clip_x(),
                        }
                        Children [
                            { header }
                        ]
                    --
                        Node {
                            width: percent(100.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(space_8()),
                        }
                        Children [
                            { groups }
                        ]

                ]
            })
        }
        None => Box::new(bsn! {
            Node {
                width: percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(space_8()),
            }
            Children [
                { groups }
            ]
        }),
    }
}

/// The grid body: the collapse/expand toggle beside the revealed grid.
/// The card chrome around it is the shared graph card (see
/// [`super::cpu::cpu_core_grid_scene`]).
pub(super) fn core_grid_body_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
    let grid = core_grid_sections_scene(shell, palette);
    let toggle = core_grid_toggle_scene(palette);
    let transition = CoreGridTransition::default();
    let measured = false;
    bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
        }
        Children [

                Node {
                    width: percent(100.0),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::FlexEnd,
                    column_gap: Val::Px(space_8()),
                }
                Children [
                     @{ toggle }
                ]
            --
                Node {
                    width: percent(100.0),
                    height: Val::Auto,
                    overflow: Overflow::clip_y(),
                }
                CoreGridReveal { transition, measured }
                Children [
                    Node {
                        width: percent(100.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(space_4()),
                    }
                    CoreGridContent
                    Children [
                        @{ grid }
                    ]
                ]
            --
                { EntityScene(page_observer(core_fill_observer)) }
            --
                { EntityScene(page_observer(core_layout_observer)) }

        ]
    }
}

/// The grid sections: the socket envelopes with their class groups and
/// cells.
fn core_grid_sections_scene(shell: &ShellApp, palette: &UiPalette) -> impl Scene + use<> {
    let core_count = core_usage_count(shell);
    // No cores measured yet → one honest empty row, never a fabricated
    // grid of zero-width cores.
    let sections: Vec<Box<dyn Scene>> = if core_count == 0 {
        vec![Box::new(bsn! {
            Node { width: percent(100.0) }
            Children [
                 Text(missing_value()) TextRole(Role::Caption)
            ]
        }) as Box<dyn Scene>]
    } else {
        core_socket_sections(shell, core_count)
            .iter()
            .map(|section| core_socket_scene(shell, section, palette))
            .collect()
    };
    bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
        }
        Children [
            { sections }
        ]
    }
}

/// The grid's subtitle: the shared per-core caption plus the observed core
/// class breakdown.
pub(super) fn core_grid_subtitle(shell: &ShellApp) -> String {
    let breakdown = shell.projection().hardware.as_ref().and_then(|hardware| {
        let breakdown = &hardware.core_breakdown;
        if breakdown.total() == 0 {
            return None;
        }
        let mut parts: Vec<String> = Vec::new();
        if breakdown.p_cores > 0 {
            parts.push(format!("{} P-Cores", breakdown.p_cores));
        }
        if breakdown.e_cores > 0 {
            parts.push(format!("{} E-Cores", breakdown.e_cores));
        }
        if breakdown.lp_cores > 0 {
            parts.push(format!("{} LP-Cores", breakdown.lp_cores));
        }
        Some(parts.join(" · "))
    });
    match breakdown {
        Some(breakdown) => format!("{} · {breakdown}", t("cpu.utilization_by_core")),
        None => t("cpu.utilization_by_core").to_owned(),
    }
}

/// The "show all logical cores" toggle: one activation flips the grid's
/// expanded intent and starts the reveal transition toward the new height.
/// The observer never writes the height itself — the shared advance owns
/// the eased value.
fn core_grid_toggle_activated(
    activate: On<Activate>,
    time: Res<Time>,
    palette: Res<WindowPalette>,
    mut toggles: Query<&mut CoreGridToggle>,
    mut bodies: Query<(&mut CoreGridReveal, &Children)>,
    contents: Query<&ComputedNode>,
) {
    let Ok(mut toggle) = toggles.get_mut(activate.event().entity) else {
        return;
    };
    toggle.expanded = !toggle.expanded;
    let now = time.elapsed_secs();
    for (mut reveal, children) in &mut bodies {
        let natural = children
            .first()
            .and_then(|content| contents.get(*content).ok())
            .map(|computed| computed.size().y / palette.inner.rem_px)
            .unwrap_or(0.0);
        let target = if toggle.expanded { natural } else { 0.0 };
        reveal.transition = CoreGridTransition {
            from: reveal.transition.value(now),
            to: target,
            started_at: now,
        };
    }
}

/// The toggle control: the shared pill skin plus the page's own intent.
fn core_grid_toggle_scene(palette: &UiPalette) -> impl Scene + use<> {
    let label = t("common.logical_cores").to_owned();
    bsn! {
        Node {
            min_width: rem(palette.control_height_rem * 2.8),
            height: rem(palette.control_height_rem),
            padding: UiRect::horizontal(Val::Px(space_12())),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ palette.nav_active_bg })
        ControlVisual(ControlTone::Surface, true)
        Button
        CoreGridToggle { expanded: true }
        on(core_grid_toggle_activated)
        Children [
             Text(label) TextRole(Role::Caption)
        ]
    }
}

/// The marker queries the grid's layout seam rewrites: the measured class
/// rows, the cells they hold, and the reveal bodies. Grouping them keeps
/// the observer's parameter list its dependency list, and lets a per-frame
/// animator share the same surface.
#[derive(SystemParam)]
pub(crate) struct CoreGridSurface<'w, 's> {
    /// The measured class rows and the cells they hold.
    pub(crate) rows: Query<
        'w,
        's,
        (
            &'static CoreGridRow,
            &'static ComputedNode,
            &'static Children,
        ),
    >,
    /// The per-core cells whose width the column policy owns.
    pub(crate) cells: Query<'w, 's, &'static mut Node, Without<CoreGridReveal>>,
    /// The collapse/expand intent the toggle publishes.
    pub(crate) toggles: Query<'w, 's, &'static CoreGridToggle>,
    /// The measured content heights the reveal follows.
    pub(crate) contents: Query<'w, 's, &'static ComputedNode>,
    /// The reveal bodies and the heights they show.
    pub(crate) bodies: Query<
        'w,
        's,
        (
            &'static mut CoreGridReveal,
            &'static mut Node,
            &'static Children,
        ),
    >,
}

/// The fold observer for the per-core bar ink: every core bar's ink
/// follows the same folded observation its fill width does, so a bar can
/// never warn about one reading while it shows another.
fn core_fill_observer(
    _fold: On<ShellProjectionFolded>,
    track: ShellTrack,
    palette: Res<WindowPalette>,
    mut fills: Query<(&CoreBarFill, &mut BackgroundColor), Without<DynText>>,
) {
    for (fill, mut color) in &mut fills {
        let wanted = core_bar_ink(core_usage_pct(track.shell(), fill.0), &palette.inner);
        if color.0 != wanted {
            color.0 = wanted;
        }
    }
}

/// The fold observer for the grid's layout: the same fold re-applies the
/// measured column policy and advances the reveal easing.
fn core_layout_observer(
    _fold: On<ShellProjectionFolded>,
    time: Res<Time>,
    palette: Res<WindowPalette>,
    mut surface: CoreGridSurface,
) {
    apply_core_grid_columns(&palette.inner, &surface.rows, &mut surface.cells);
    advance_core_grid(
        time.elapsed_secs(),
        &palette.inner,
        &surface.toggles,
        &surface.contents,
        &mut surface.bodies,
    );
}

/// The column policy: each row's cells take the width the row's measured
/// width affords, so the wide grid folds to the compact grid instead of
/// squeezing four cells into a narrow row.
pub(crate) fn apply_core_grid_columns(
    palette: &UiPalette,
    rows: &Query<(&CoreGridRow, &ComputedNode, &Children)>,
    cells: &mut Query<&mut Node, Without<CoreGridReveal>>,
) {
    for (_, computed, children) in rows.iter() {
        let columns = core_grid_columns(
            children.len(),
            computed.size().x,
            core_cell_readable_rem(palette) * palette.rem_px,
            core_cell_floor_rem(palette) * palette.rem_px,
        );
        let width = Val::Percent(core_cell_width_percent(columns));
        for child in children.iter() {
            if let Ok(mut cell) = cells.get_mut(*child)
                && cell.width != width
            {
                cell.width = width;
            }
        }
    }
}

/// The reveal advance: the grid body eases its height toward the toggled
/// state and follows the content's own height while it stays open. The
/// clock is passed in, so the fold observer and a per-frame animator drive
/// the same easing.
pub(crate) fn advance_core_grid(
    now: f32,
    palette: &UiPalette,
    toggles: &Query<&CoreGridToggle>,
    contents: &Query<&ComputedNode>,
    bodies: &mut Query<(&mut CoreGridReveal, &mut Node, &Children)>,
) {
    let expanded = toggles.iter().next().is_none_or(|toggle| toggle.expanded);
    for (mut reveal, mut node, children) in bodies.iter_mut() {
        let Some(content) = children.first() else {
            continue;
        };
        let Ok(computed) = contents.get(*content) else {
            continue;
        };
        let measured = computed.size().y / palette.rem_px;
        // The natural height is captured ONCE, on the first frame the body is
        // open. A collapsed body measures its content as the collapsed height,
        // so re-measuring the target from there would shrink the reopen
        // towards zero — the same rule the storage surface follows.
        if reveal.natural_height.is_none() && measured > 0.0 {
            reveal.natural_height = Some(measured);
        }
        let natural = reveal.natural_height.unwrap_or(measured);
        let target = if expanded { natural } else { 0.0 };
        if !reveal.measured {
            reveal.measured = true;
            reveal.transition = CoreGridTransition {
                from: natural,
                to: target,
                started_at: now,
            };
        } else if (reveal.transition.to - target).abs() > f32::EPSILON {
            // The intent changed: start a new transition from the height the
            // body currently shows.
            reveal.transition = CoreGridTransition {
                from: reveal.transition.value(now),
                to: target,
                started_at: now,
            };
        } else if expanded {
            // The open grid follows its content: a folded projection that
            // reports more cores grows the open body with it.
            reveal.transition.to = natural;
            if reveal.transition.progress(now) >= 1.0 {
                reveal.transition.from = natural;
            }
        }
        node.height = Val::Rem(reveal.transition.value(now).max(0.0));
    }
}

#[cfg(test)]
#[path = "../../../../../tests/headless/pages/cpu_cores_parity.rs"]
mod cpu_cores_parity_tests;
