//! The motion contract: shared transition ROLES and the rule that a shape
//! realizes them with its own animation engine.
//!
//! `taskmanager-theme` owns the duration tokens and the motion policy
//! (ADR-026); this crate stays independent of that codec, so the tokens
//! arrive as plain [`MotionDurations`] values and the policy's effect is
//! applied at the frontend's binding seam. This module owns what the theme
//! cannot: which user-facing transition roles exist, which curve class each
//! role means, which visual properties may move, and what is FORBIDDEN to
//! animate. A frontend binds the role to GPUI's `Animation`, Iced's `Easing`,
//! Bevy's ECS tween or a terminal repaint, but never invents a new role or
//! spends a bare duration.

use std::time::Duration;

use crate::keybindings::FrontendShape;

/// The theme's duration tokens, as the contract consumes them. ADR-063 owns
/// the numbers; this struct only names the class slots, so the role owner
/// never imports the token codec and the frontend binding stays the one weld.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MotionDurations {
    /// The micro-interaction token.
    pub fast: Duration,
    /// The hover/state token.
    pub hover: Duration,
    /// The appear/normal token.
    pub medium: Duration,
}

impl MotionDurations {
    /// The token the class spends.
    #[must_use]
    pub const fn for_class(self, class: DurationClass) -> Duration {
        match class {
            DurationClass::Fast => self.fast,
            DurationClass::Hover => self.hover,
            DurationClass::Medium => self.medium,
        }
    }
}

/// The one vocabulary of user-visible transitions. The role is a semantic
/// obligation; a shape's engine is an implementation detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MotionRole {
    /// A tooltip, modal, or panel enters the screen.
    Appear,
    /// A surface leaves the screen; the final state is applied on completion.
    Exit,
    /// Hover/selection/enable state change on a non-virtualized control.
    StateChange,
    /// A bounded layout reflow (sidebar width, stacked rail admission).
    LayoutReflow,
    /// A staged entrance for a small, bounded collection.
    Stagger,
    /// Continuous progress feedback (spinner, indeterminate progress).
    Indeterminate,
}

impl MotionRole {
    /// Every motion role, in stable report order.
    pub const ALL: [Self; 6] = [
        Self::Appear,
        Self::Exit,
        Self::StateChange,
        Self::LayoutReflow,
        Self::Stagger,
        Self::Indeterminate,
    ];

    /// Stable identity for gate output.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Appear => "appear",
            Self::Exit => "exit",
            Self::StateChange => "state-change",
            Self::LayoutReflow => "layout-reflow",
            Self::Stagger => "stagger",
            Self::Indeterminate => "indeterminate",
        }
    }

    /// The shared duration class. The theme maps each class to its token; a
    /// frontend never reads a millisecond literal.
    #[must_use]
    pub const fn duration_class(self) -> DurationClass {
        match self {
            Self::Appear | Self::Exit | Self::LayoutReflow => DurationClass::Medium,
            Self::StateChange | Self::Stagger => DurationClass::Hover,
            Self::Indeterminate => DurationClass::Fast,
        }
    }

    /// The shared easing class. GPUI's `ease_in_out` and Iced's
    /// `EaseOutCubic` are both bindings of these classes, not two authorities.
    #[must_use]
    pub const fn easing(self) -> EasingClass {
        match self {
            Self::Appear | Self::LayoutReflow => EasingClass::EaseOut,
            Self::Exit => EasingClass::EaseIn,
            Self::StateChange | Self::Stagger => EasingClass::EaseInOut,
            Self::Indeterminate => EasingClass::Linear,
        }
    }

    /// Whether the role is a one-shot transition (a 0→1 sweep with a final
    /// state). Continuous feedback is not one-shot and is never reduced to a
    /// zero-duration animation.
    #[must_use]
    pub const fn is_one_shot(self) -> bool {
        !matches!(self, Self::Indeterminate)
    }
}

/// The shared duration classes, resolved through [`MotionDurations`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurationClass {
    /// 80ms micro-interaction.
    Fast,
    /// 120ms hover/state class.
    Hover,
    /// 180ms appear/normal class.
    Medium,
}

impl DurationClass {
    pub const ALL: [Self; 3] = [Self::Fast, Self::Hover, Self::Medium];

    /// Stable identity for traces.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Hover => "hover",
            Self::Medium => "medium",
        }
    }
}

/// The toolkit-neutral easing classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EasingClass {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl EasingClass {
    pub const ALL: [Self; 4] = [Self::Linear, Self::EaseIn, Self::EaseOut, Self::EaseInOut];

    /// Stable identity for gate output.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::EaseIn => "ease-in",
            Self::EaseOut => "ease-out",
            Self::EaseInOut => "ease-in-out",
        }
    }
}

/// The visual properties a role may animate. Layout geometry is NEVER in this
/// set: a shared budget decision is applied immediately, not tweened, so a
/// capture cannot observe a half-allocated frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimatedProperty {
    Opacity,
    BackgroundColor,
    ForegroundColor,
    BorderColor,
    /// Bounded progress 0..1 (spinner phase, indeterminate bar).
    Progress,
}

impl AnimatedProperty {
    pub const ALL: [Self; 5] = [
        Self::Opacity,
        Self::BackgroundColor,
        Self::ForegroundColor,
        Self::BorderColor,
        Self::Progress,
    ];
}

/// One resolved motion intent: the shape binds the classes but never changes
/// the role, class or property set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MotionResolution {
    pub role: MotionRole,
    pub duration_class: DurationClass,
    pub easing: EasingClass,
    /// `None` means apply the final state without constructing an animation.
    pub duration: Option<Duration>,
    pub frontend: FrontendShape,
}

/// Resolve one motion role for one shape. The caller's binding seam applies
/// the theme's policy to the class token first, so `None` means the caller
/// paints the final state without constructing a zero-duration animation.
#[must_use]
pub fn resolve_motion(
    frontend: FrontendShape,
    role: MotionRole,
    duration: Option<Duration>,
) -> MotionResolution {
    MotionResolution {
        role,
        duration_class: role.duration_class(),
        easing: role.easing(),
        duration,
        frontend,
    }
}

/// The forbidden animation sites. A frontend that tweens a cached projection,
/// a virtualized row's hover state, or a layout budget creates per-frame work
/// and lets a capture see a half-decided frame; the ratchet rejects it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForbiddenAnimation {
    /// Hover/selection on a virtualized row (10k-item lists).
    VirtualizedRowState,
    /// Any layout extent, slot budget or scroll offset.
    LayoutGeometry,
    /// A cached application projection's value/text.
    ProjectionValue,
    /// A control's hit target or focus ring geometry.
    HitTargetGeometry,
}

impl ForbiddenAnimation {
    pub const ALL: [Self; 4] = [
        Self::VirtualizedRowState,
        Self::LayoutGeometry,
        Self::ProjectionValue,
        Self::HitTargetGeometry,
    ];

    /// Stable identity for the ratchet.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::VirtualizedRowState => "virtualized-row-state",
            Self::LayoutGeometry => "layout-geometry",
            Self::ProjectionValue => "projection-value",
            Self::HitTargetGeometry => "hit-target-geometry",
        }
    }
}

/// The four shapes' declared motion binding. `Native` means the shape uses its
/// own engine to realize the shared roles; `CellRepaint` is TUI's honest
/// terminal binding; a shape that does not bind a role is a finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionBinding {
    /// Realizes the role through a toolkit animation primitive.
    Native { engine: &'static str },
    /// Terminal repaint: no sub-cell interpolation, final state only.
    CellRepaint { reason: &'static str },
}

impl MotionBinding {
    /// The driver this binding must carry.
    #[must_use]
    pub const fn explanation(self) -> &'static str {
        match self {
            Self::Native { engine } | Self::CellRepaint { reason: engine } => engine,
        }
    }
}

/// The current binding registry. Every shape×role cell is explicit; a new
/// shape or role must fill it in the same change (hard cutover, no wait-list).
#[must_use]
pub fn binding(frontend: FrontendShape, role: MotionRole) -> Option<MotionBinding> {
    match (frontend, role) {
        (FrontendShape::Gpui, _) => Some(MotionBinding::Native {
            engine: "taskmanager-ui::primitives::motion",
        }),
        (FrontendShape::Iced, _) => Some(MotionBinding::Native {
            engine: "taskmanager-iced::app::motion + iced::Animation",
        }),
        (FrontendShape::Bevy, _) => Some(MotionBinding::Native {
            engine: "taskmanager-bevy-ui::widgets::switch + ECS tween",
        }),
        (FrontendShape::Tui, MotionRole::Appear | MotionRole::Exit | MotionRole::StateChange) => {
            Some(MotionBinding::CellRepaint {
                reason: "ratatui paints the final cell state on the next frame; a terminal has no \
                     sub-cell interpolation",
            })
        }
        (
            FrontendShape::Tui,
            MotionRole::LayoutReflow | MotionRole::Stagger | MotionRole::Indeterminate,
        ) => Some(MotionBinding::CellRepaint {
            reason: "the terminal spinner/layout roles advance one whole cell row per frame",
        }),
    }
}

/// Findings for the motion contract: every shape×role×policy must resolve, and
/// every binding must carry a driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionFindingKind {
    MissingBinding,
    EmptyDriver,
}

/// One motion finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MotionFinding {
    pub frontend: FrontendShape,
    pub role: MotionRole,
    pub kind: MotionFindingKind,
}

/// Validate the motion registry: every shape×role cell must be explicit and
/// carry a driver. The duration weld (which token each class spends, what a
/// policy does to it) is proven at each frontend's binding seam.
#[must_use]
pub fn motion_findings() -> Vec<MotionFinding> {
    let mut findings = Vec::new();
    for frontend in FrontendShape::ALL {
        for role in MotionRole::ALL {
            let binding = binding(frontend, role);
            let mut push = |kind| {
                findings.push(MotionFinding {
                    frontend,
                    role,
                    kind,
                });
            };
            match binding {
                None => push(MotionFindingKind::MissingBinding),
                Some(binding) if binding.explanation().trim().is_empty() => {
                    push(MotionFindingKind::EmptyDriver);
                }
                Some(_) => {}
            }
        }
    }
    findings
}
