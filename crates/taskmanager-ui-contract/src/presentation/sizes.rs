//! The control measurement matrix: the shared ROLE vocabulary every frontend
//! resolves against the design tokens.
//!
//! ADR-063 keeps every design NUMBER with one owner (`taskmanager-theme`).
//! This crate is the ROLE owner and stays independent of that owner's token
//! codec, so the theme's axis values arrive as plain [`UiSizePoint`] /
//! [`DensityPoint`] inputs lifted by each frontend's binding seam. This
//! module owns what the theme cannot: which control roles exist, what each
//! role's minimum extent/hit target means, which states change the
//! measurement, and the rule that a shape must resolve the same role through
//! the same axis. A frontend supplies its own unit conversion — GPUI window
//! rem, Iced scaled px, Bevy rem, TUI cells — but never a second matrix.
//!
//! The matrix is exhaustive over [`ControlRole`]: a new role with no row is a
//! finding, not an omission a renderer may fill in locally.

use crate::keybindings::FrontendShape;

/// One point of the theme's UI-size axis, as the contract consumes it. The
/// numbers are the theme's (ADR-063); this struct only names the slots the
/// measurement fold spends, so the role owner never mirrors the token codec.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiSizePoint {
    /// The theme's control-height token at this axis point, logical px.
    pub control_height: f32,
    /// The theme's body font-size token at this axis point, logical px.
    pub body_font_size: f32,
    /// The renderer scale the frontend applies at this axis point.
    pub renderer_scale: f32,
}

/// One point of the theme's row-density axis, as the contract consumes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DensityPoint {
    /// The density's body-row vertical padding, logical px.
    pub row_padding_y: f32,
    /// The density's body-row line-height ratio.
    pub line_height: f32,
}

/// The control roles whose measurement every frontend must agree on. A role
/// names a semantic obligation, not a widget class: a segmented control and a
/// button both spend `Button` when they serve one activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControlRole {
    /// A standard activation control (button, icon button, action pill).
    Button,
    /// A tab or navigation destination.
    Tab,
    /// A text/search/select input.
    Input,
    /// A switch, checkbox or toggle.
    Toggle,
    /// A selectable data row in a table or list.
    DataRow,
    /// A menu or context-menu item.
    MenuItem,
    /// A dialog header/action band.
    DialogAction,
    /// An icon-only affordance (close, overflow, disclosure).
    IconButton,
    /// A badge or status pill (non-interactive).
    Badge,
    /// A tooltip or transient-feedback surface.
    Tooltip,
}

impl ControlRole {
    /// Every control role, in stable report order.
    pub const ALL: [Self; 10] = [
        Self::Button,
        Self::Tab,
        Self::Input,
        Self::Toggle,
        Self::DataRow,
        Self::MenuItem,
        Self::DialogAction,
        Self::IconButton,
        Self::Badge,
        Self::Tooltip,
    ];

    /// Stable identity for gate output and resolution traces.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Tab => "tab",
            Self::Input => "input",
            Self::Toggle => "toggle",
            Self::DataRow => "data-row",
            Self::MenuItem => "menu-item",
            Self::DialogAction => "dialog-action",
            Self::IconButton => "icon-button",
            Self::Badge => "badge",
            Self::Tooltip => "tooltip",
        }
    }

    /// Whether the role is interactive. A non-interactive role has no hit
    /// target; asserting one would fabricate a pointer obligation that TUI
    /// cannot spend.
    #[must_use]
    pub const fn is_interactive(self) -> bool {
        !matches!(self, Self::Badge | Self::Tooltip)
    }

    /// The minimum hit-target extent (logical px at Standard UI size) this
    /// role must keep in a pointer-capable shape. 44px is the WCAG 2.5.8
    /// floor; dense rows and icon affordances keep it through padding, not by
    /// shrinking the glyph. `None` means the role has no pointer target.
    #[must_use]
    pub const fn hit_target_floor(self) -> Option<f32> {
        match self {
            Self::Button | Self::Tab | Self::Input | Self::MenuItem | Self::DialogAction => {
                Some(44.0)
            }
            Self::Toggle => Some(32.0),
            Self::DataRow | Self::IconButton => Some(44.0),
            Self::Badge | Self::Tooltip => None,
        }
    }

    /// Whether density is allowed to change this role's vertical extent. A
    /// compact table tightens row padding, never a button or a focus target.
    #[must_use]
    pub const fn admits_density(self) -> bool {
        matches!(self, Self::DataRow)
    }
}

/// The state axes a resolver must answer. Each is a semantic choice, not a
/// toolkit unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControlState {
    /// Ordinary enabled control.
    Normal,
    /// Keyboard/pointer focus is visible.
    Focused,
    /// Hover/pressed/feedback state with no measurement change.
    Active,
    /// Disabled: the hit target stays reachable for semantics, never shrinks.
    Disabled,
    /// Row-density compact mode (meaningful only for [`ControlRole::DataRow`]).
    Compact,
}

impl ControlState {
    /// Every control state, in stable order.
    pub const ALL: [Self; 5] = [
        Self::Normal,
        Self::Focused,
        Self::Active,
        Self::Disabled,
        Self::Compact,
    ];

    /// Whether this state may change the measured extent at all. Focus,
    /// active and disabled change PAINT, never geometry: a focus ring never
    /// eats a slot budget, and a disabled control never loses its target.
    #[must_use]
    pub const fn changes_measurement(self) -> bool {
        matches!(self, Self::Compact)
    }
}

/// The unit a shape resolves measurements into. This is the "存异" half: the
/// shared matrix stays in logical px, the shape names what those px mean in
/// its own toolkit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionUnit {
    /// GPUI's window-relative rem, driven by `UiSize`.
    WindowRem,
    /// Iced's scaled logical pixels, driven by `UiSize::renderer_scale`.
    ScaledPixels,
    /// Bevy's rem axis, driven by the root font size.
    SceneRem,
    /// Terminal cells: one cell wide, one row tall, with no sub-cell geometry.
    TerminalCells,
}

impl ResolutionUnit {
    /// Every shape's declared unit. Set equality is gate-checked, so a new
    /// shape cannot silently inherit another shape's rendering model.
    #[must_use]
    pub const fn for_frontend(frontend: FrontendShape) -> Self {
        match frontend {
            FrontendShape::Gpui => Self::WindowRem,
            FrontendShape::Iced => Self::ScaledPixels,
            FrontendShape::Bevy => Self::SceneRem,
            FrontendShape::Tui => Self::TerminalCells,
        }
    }

    /// Stable identity for traces.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::WindowRem => "window-rem",
            Self::ScaledPixels => "scaled-pixels",
            Self::SceneRem => "scene-rem",
            Self::TerminalCells => "terminal-cells",
        }
    }

    /// Whether this unit can express a sub-unit hit target. A terminal cell
    /// cannot: TUI reports its target in whole cells and the trace records the
    /// rounding, never a fabricated exact px.
    #[must_use]
    pub const fn supports_sub_unit_targets(self) -> bool {
        !matches!(self, Self::TerminalCells)
    }
}

/// One resolved control measurement: the shared logical-px intent plus the
/// shape's declared unit. The shape converts `extent` into its own geometry;
/// `extent` itself never differs between shapes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlMeasurement {
    pub role: ControlRole,
    pub state: ControlState,
    /// Logical px extent on the Standard UI size axis.
    pub extent: f32,
    /// Minimum hit target in logical px, or `None` for a non-interactive role.
    pub hit_target: Option<f32>,
    /// Horizontal padding inside the control, in logical px.
    pub padding_x: f32,
    /// Vertical padding inside the control, in logical px.
    pub padding_y: f32,
    /// The shape's conversion unit; never a second measurement authority.
    pub unit: ResolutionUnit,
    /// The shape's realized extent in its own unit. For a terminal this is a
    /// whole-cell count; the shared intent stays in `extent`.
    pub realized_extent: f32,
}

/// Resolve one role/state measurement for one shape. The numeric ladder
/// arrives as theme-owned values: Small 32 / Standard 38 / Large 46 control
/// heights with row density spending the theme's paddings. This function is
/// the ONLY fold from those values into the shared matrix, so all four shapes
/// stay on one authority even though their units differ.
#[must_use]
pub fn resolve_size(
    frontend: FrontendShape,
    role: ControlRole,
    state: ControlState,
    ui_size: UiSizePoint,
    density: DensityPoint,
) -> ControlMeasurement {
    let unit = ResolutionUnit::for_frontend(frontend);
    let (extent, padding_y) = if role.admits_density() {
        let row_padding = density.row_padding_y;
        // The row extent rides the theme's own type ladder and the theme's own
        // line-height ratio; this layer never re-numbers either.
        let body_line = ui_size.body_font_size * density.line_height;
        (body_line + row_padding * 2.0, row_padding)
    } else {
        let inset = match role {
            ControlRole::Badge | ControlRole::Tooltip => 4.0,
            ControlRole::IconButton => 6.0,
            _ => 8.0,
        };
        (ui_size.control_height, inset)
    };
    let hit_target = role
        .hit_target_floor()
        .map(|floor| (floor * ui_size.renderer_scale).max(extent));
    let realized_extent = if unit.supports_sub_unit_targets() {
        extent
    } else {
        extent.ceil()
    };
    ControlMeasurement {
        role,
        state,
        extent,
        hit_target,
        padding_x: match role {
            ControlRole::Badge | ControlRole::Tooltip => 6.0,
            ControlRole::DataRow => 8.0,
            _ => 12.0,
        },
        padding_y,
        unit,
        realized_extent,
    }
}

/// Findings for a shape's resolver coverage: every role, every state, every
/// UI size and every density must resolve to one finite measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeFindingKind {
    NonFiniteExtent,
    HitTargetTooSmall,
    GeometryChangedByFocusState,
    DensityChangedControlHeight,
    WrongResolutionUnit,
}

/// One size-matrix finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeFinding {
    pub frontend: FrontendShape,
    pub role: ControlRole,
    pub state: ControlState,
    pub kind: SizeFindingKind,
}

/// Validate one shape's whole resolution grid against the shared invariants.
/// The grid axes are the caller's theme points, so the exhaustive check runs
/// against the real ladder without this crate importing the token codec.
#[must_use]
pub fn size_findings(
    frontend: FrontendShape,
    ui_sizes: &[UiSizePoint],
    densities: &[DensityPoint],
) -> Vec<SizeFinding> {
    let mut findings = Vec::new();
    let expected_unit = ResolutionUnit::for_frontend(frontend);
    for role in ControlRole::ALL {
        for state in ControlState::ALL {
            for ui_size in ui_sizes {
                for kind in axis_findings(frontend, expected_unit, role, state, *ui_size, densities)
                {
                    findings.push(SizeFinding {
                        frontend,
                        role,
                        state,
                        kind,
                    });
                }
            }
        }
    }
    findings
}

/// The invariant checks one role/state/ui-size cell owes across the whole
/// density axis. Focus, active and disabled states never move geometry, and a
/// role that does not admit density never reads the density axis at all.
fn axis_findings(
    frontend: FrontendShape,
    expected_unit: ResolutionUnit,
    role: ControlRole,
    state: ControlState,
    ui_size: UiSizePoint,
    densities: &[DensityPoint],
) -> Vec<SizeFindingKind> {
    let mut kinds = Vec::new();
    for density in densities {
        let measured = resolve_size(frontend, role, state, ui_size, *density);
        if !measured.extent.is_finite() || measured.extent <= 0.0 {
            kinds.push(SizeFindingKind::NonFiniteExtent);
        }
        if measured.unit != expected_unit {
            kinds.push(SizeFindingKind::WrongResolutionUnit);
        }
        if let Some(target) = measured.hit_target
            && (target < measured.extent
                || (!expected_unit.supports_sub_unit_targets()
                    && target < measured.realized_extent))
        {
            kinds.push(SizeFindingKind::HitTargetTooSmall);
        }
        if !state.changes_measurement()
            && resolve_size(frontend, role, ControlState::Normal, ui_size, *density).extent
                != measured.extent
        {
            kinds.push(SizeFindingKind::GeometryChangedByFocusState);
        }
        if !role.admits_density() {
            for other in densities {
                if resolve_size(frontend, role, state, ui_size, *other).extent != measured.extent {
                    kinds.push(SizeFindingKind::DensityChangedControlHeight);
                }
            }
        }
    }
    kinds
}
